use futures::{
    StreamExt,
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
    join,
    lock::Mutex,
};
use gpui::{
    AsyncApp,
    Context,
    InteractiveElement,
    IntoElement,
    ParentElement,
    Render,
    StatefulInteractiveElement,
    Styled,
    WeakEntity,
    Window,
};
use serde::Deserialize;
use zbus::{Connection, proxy, zvariant};

use crate::widget::{Widget, widget_wrapper};

pub struct PowerProfile {
    tx: UnboundedSender<()>,
    error_message: Option<String>,
    active_profile: Option<String>,
}

impl Widget for PowerProfile {
    type Config = PowerProfileConfig;

    fn new(cx: &mut Context<Self>, config: &Self::Config) -> Self {
        let (tx, rx) = mpsc::unbounded();
        let cycle_direction = config.cycle_direction.clone();
        cx.spawn(async |this, cx| task(this, cx, rx, cycle_direction).await)
            .detach();

        Self {
            tx,
            error_message: None,
            active_profile: None,
        }
    }
}

impl Render for PowerProfile {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            widget_wrapper().child(e.clone()).into_any_element()
        } else if let Some(profile) = &self.active_profile {
            let icon_wrapper = || {
                widget_wrapper()
                    .id("power-profile")
                    .on_click(cx.listener(|this, _, _, _| {
                        let _ = this.tx.unbounded_send(());
                    }))
                    .font_family("Material Symbols Rounded")
            };
            match profile.as_str() {
                "power-saver" => icon_wrapper().child("\u{ec1a}").into_any_element(),
                "balanced" => icon_wrapper().child("\u{e9e4}").into_any_element(),
                "performance" => icon_wrapper().child("\u{eb9b}").into_any_element(),
                _ => widget_wrapper().child(profile.clone()).into_any_element(),
            }
        } else {
            widget_wrapper().child("?").into_any_element()
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PowerProfileConfig {
    cycle_direction: CycleDirection,
}

#[derive(Clone, Default, Deserialize)]
enum CycleDirection {
    Up,
    #[default]
    Down,
}

async fn task(
    this: WeakEntity<PowerProfile>,
    cx: &mut AsyncApp,
    mut rx: UnboundedReceiver<()>,
    cycle_direction: CycleDirection,
) {
    let connection = match Connection::system().await {
        Ok(x) => x,
        Err(e) => {
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("Failed to connect to system bus: {e}"));
                cx.notify();
            });
            tracing::error!(error = %e, "Failed to connect to system bus");
            return;
        }
    };
    let proxy = match PowerProfilesProxy::new(&connection).await {
        Ok(x) => x,
        Err(e) => {
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("Failed to create properties proxy: {e}"));
                cx.notify();
            });
            tracing::error!(error = %e, "Failed to create properties proxy");
            return;
        }
    };
    let mut active_profile_stream = proxy.receive_active_profile_changed().await;
    let active_profile = Mutex::new(None);
    let mut profiles_stream = proxy.receive_profiles_changed().await;
    let profiles = Mutex::new(None);
    join!(
        async {
            while let Some(new_active_profile) = active_profile_stream.next().await {
                match new_active_profile.get().await {
                    Ok(new_active_profile) => {
                        tracing::info!(new_active_profile, "Power profile changed");
                        let _ = this.update(cx, |this, cx| {
                            this.active_profile = Some(new_active_profile.clone());
                            cx.notify();
                        });
                        active_profile.lock().await.replace(new_active_profile);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to get new ActiveProfile");
                    }
                }
            }
            tracing::warn!("Receive ActiveProfile stream ended");
        },
        async {
            while let Some(new_profiles) = profiles_stream.next().await {
                match new_profiles.get().await {
                    Ok(new_profiles) => {
                        tracing::info!(?new_profiles, "Power profile changed");
                        let new_profiles = new_profiles
                            .into_iter()
                            .map(|Profile { profile }| profile)
                            .collect();
                        profiles.lock().await.replace(new_profiles);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to get new ActiveProfile");
                    }
                }
            }
            tracing::warn!("Receive ActiveProfile stream ended");
        },
        async {
            while let Some(()) = rx.next().await {
                let active_profile = active_profile.lock().await;
                let profiles = profiles.lock().await;
                let target_profile = match (
                    active_profile.as_ref(),
                    profiles.as_ref().map(Vec::as_slice),
                ) {
                    (Some(active_profile), Some(profiles)) => match (
                        profiles.iter().position(|x| x == active_profile),
                        &cycle_direction,
                    ) {
                        (Some(position), CycleDirection::Up) if position == profiles.len() => {
                            profiles.first()
                        }
                        (Some(position), CycleDirection::Up) => profiles.get(position + 1),
                        (Some(position), CycleDirection::Down) if position == 0 => profiles.last(),
                        (Some(position), CycleDirection::Down) => profiles.get(position - 1),
                        (None, _) => profiles.first(),
                    }
                    .map(String::as_str),
                    (Some(active_profile), None) => Some(active_profile.as_str()),
                    (None, Some([profile, ..])) => Some(profile.as_str()),
                    (None, Some([])) | (None, None) => None,
                };
                let target_profile = target_profile.unwrap_or("balanced"); // Default to balanced
                if let Err(e) = proxy.set_active_profile(target_profile).await {
                    tracing::error!(error = %e, "Failed to set active profile");
                }
            }
        },
    );
}

// <https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html>
#[proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_service = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PowerProfiles {
    fn hold_profile(&self, profile: &str, reason: &str, application_id: &str) -> zbus::Result<u32>;
    fn release_profile(&self, cookie: u32) -> zbus::Result<()>;
    fn set_action_enabled(&self, action: &str, enabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn profile_released(&self, cookie: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, active_profile: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<Profile>>;
}

#[derive(Debug, zvariant::Value)]
#[zvariant(signature = "a{sv}")]
struct Profile {
    #[zvariant(rename = "Profile")]
    profile: String,
}

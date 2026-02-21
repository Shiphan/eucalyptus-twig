use futures::{
    StreamExt,
    channel::mpsc::{self, UnboundedReceiver, UnboundedSender},
    join,
    lock::Mutex,
};
use gpui::{
    AsyncApp, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use serde::Deserialize;
use zbus::{Connection, proxy};

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
#[serde(deny_unknown_fields)]
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
    let mut stream = proxy.receive_active_profile_changed().await;
    let active_profile = Mutex::new(None::<String>);
    join!(
        async {
            while let Some(new_active_profile) = stream.next().await {
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
            while let Some(()) = rx.next().await {
                let target_profile =
                    if let Some(active_profile) = active_profile.lock().await.as_ref() {
                        ["power-saver", "balanced", "performance"][(match active_profile.as_str() {
                            "power-saver" => 0,
                            "balanced" => 1,
                            "performance" => 2,
                            _ => 1,
                        } + match cycle_direction {
                            CycleDirection::Up => 1,
                            CycleDirection::Down => 2,
                        }) % 3]
                    } else {
                        "balanced"
                    };
                if let Err(e) = proxy.set_active_profile(target_profile).await {
                    tracing::error!(error = %e, "Failed to set active profile");
                }
            }
        }
    );
}

// <https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html>
#[proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_service = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PowerProfiles {
    fn hold_profile(
        &self,
        profile: String,
        reason: String,
        application_id: String,
    ) -> zbus::Result<u32>;
    fn release_profile(&self, cookie: u32) -> zbus::Result<()>;
    fn set_action_enabled(&self, action: String, enabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn profile_released(&self, cookie: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, active_profile: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
}

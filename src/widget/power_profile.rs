use futures::StreamExt;
use gpui::{AsyncApp, Context, IntoElement, ParentElement, Render, Styled, WeakEntity, Window};
use zbus::{Connection, proxy};

use crate::widget::{Widget, widget_wrapper};

pub struct PowerProfile {
    error_message: Option<String>,
    active_profile: Option<String>,
}

impl Widget for PowerProfile {
    type Config = ();

    fn new(cx: &mut Context<Self>, _config: &Self::Config) -> Self {
        cx.spawn(task).detach();

        Self {
            error_message: None,
            active_profile: None,
        }
    }
}

impl Render for PowerProfile {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            widget_wrapper().child(e.clone())
        } else if let Some(profile) = &self.active_profile {
            let icon_wrapper = || widget_wrapper().font_family("Material Symbols Rounded");
            match profile.as_str() {
                "power-saver" => icon_wrapper().child(""),
                "balanced" => icon_wrapper().child(""),
                "performance" => icon_wrapper().child(""),
                _ => widget_wrapper().child(profile.clone()),
            }
        } else {
            widget_wrapper().child("?")
        }
    }
}

async fn task(this: WeakEntity<PowerProfile>, cx: &mut AsyncApp) {
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
    while let Some(active_profile) = stream.next().await {
        match active_profile.get().await {
            Ok(active_profile) => {
                tracing::info!(active_profile, "Power profile changed");
                let _ = this.update(cx, |this, cx| {
                    this.active_profile = Some(active_profile);
                    cx.notify();
                });
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to get new ActiveProfile");
            }
        }
    }
    tracing::warn!("Receive ActiveProfile stream ended");
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
    fn performance_degraded(&self) -> zbus::Result<String>;
}

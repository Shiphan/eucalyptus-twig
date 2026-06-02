use futures::StreamExt;
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
    div,
};
use serde::Deserialize;
use serde_repr::Deserialize_repr;
use zbus::zvariant;

use crate::widget::{Widget, spawn_detached_command, widget_wrapper};

pub struct Network {
    config: NetworkConfig,
    error_message: Option<String>,
    state: Option<NetworkManagerState>,
    primary_connection_type: Option<String>,
}

impl Widget for Network {
    type Config = NetworkConfig;

    fn new(cx: &mut Context<Self>, config: &Self::Config) -> Self {
        cx.spawn(task).detach();

        Self {
            config: config.clone(),
            error_message: None,
            state: None,
            primary_connection_type: None,
        }
    }
}

impl Render for Network {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // TODO: Add
        // 1. support of both wifi and wired network
        // 2. wifi strength: <https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.AccessPoint.html#gdbus-property-org-freedesktop-NetworkManager-AccessPoint.Strength>
        let _ = self.primary_connection_type;

        let widget = widget_wrapper().child(div().font_family("Material Symbols Rounded").child(
            match self.state {
                Some(NetworkManagerState::Disabled | NetworkManagerState::Disconnected) => {
                    "\u{e1da}"
                } // Signal Wifi Off
                Some(NetworkManagerState::Disconnecting) => "\u{f063}", // Signal Wifi Bad
                Some(NetworkManagerState::Connecting) => "\u{eb31}",    // Wifi Find
                Some(NetworkManagerState::ConnectedLocal | NetworkManagerState::ConnectedSite) => {
                    "\u{eb2f}"
                } // Lan
                Some(NetworkManagerState::ConnectedGlobal) => "\u{e80b}", // Public
                Some(NetworkManagerState::Unknown) | None => "?",
            },
        ));

        if let Some(command) = &self.config.settings_command {
            let command = command.clone();
            widget
                .id("network")
                .on_click(move |_, _, cx| {
                    spawn_detached_command(cx, command.as_ref(), "widget.network.settings_command")
                })
                .into_any_element()
        } else {
            widget.into_any_element()
        }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkConfig {
    pub settings_command: Option<Box<[String]>>,
}

async fn task(this: WeakEntity<Network>, cx: &mut AsyncApp) {
    let connection = match zbus::Connection::system().await {
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
    let proxy = match NetworkManagerProxy::new(&connection).await {
        Ok(x) => x,
        Err(e) => {
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("Failed to create network manager proxy: {e}"));
                cx.notify();
            });
            tracing::error!(error = %e, "Failed to create network manager proxy");
            return;
        }
    };
    let mut primary_connection_type_stream = proxy.receive_primary_connection_type_changed().await;
    let mut state_stream = proxy.receive_state_changed().await;

    futures::join!(
        {
            let this = &this;
            let mut cx = cx.clone();
            async move {
                while let Some(new_primary_connection_type) =
                    primary_connection_type_stream.next().await
                {
                    match new_primary_connection_type.get().await {
                        Ok(new_primary_connection_type) => {
                            tracing::info!(
                                new_primary_connection_type,
                                "PrimaryConnectionType changed"
                            );
                            let _ = this.update(&mut cx, |this, cx| {
                                this.primary_connection_type = Some(new_primary_connection_type);
                                cx.notify();
                            });
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to get new PrimaryConnectionType");
                        }
                    }
                }
            }
        },
        async {
            while let Some(new_state) = state_stream.next().await {
                match new_state.get().await {
                    Ok(new_state) => {
                        tracing::info!(?new_state, "State changed");
                        let _ = this.update(cx, |this, cx| {
                            this.state = Some(new_state);
                            cx.notify();
                        });
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to get new State");
                    }
                }
            }
        },
    );
}

// <https://networkmanager.dev/docs/api/latest/spec.html>
#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn primary_connection_type(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<NetworkManagerState>;
}

#[derive(Clone, Debug, Deserialize_repr, zvariant::OwnedValue)]
#[repr(u32)]
enum NetworkManagerState {
    Unknown = 0,
    Disabled = 10,
    Disconnected = 20,
    Disconnecting = 30,
    Connecting = 40,
    ConnectedLocal = 50,
    ConnectedSite = 60,
    ConnectedGlobal = 70,
}

use futures::StreamExt;
use iced_core::Font;
use iced_futures::Subscription;
use iced_runtime::{Task, task::{Sender}};
use serde::Deserialize;
use serde_repr::Deserialize_repr;
use zbus::zvariant;

use crate::{application::Element, widget::{Widget, WidgetPadding, spawn_detached_command}};

// TODO: Add
// 1. support of both wifi and wired network
// 2. wifi strength: <https://networkmanager.dev/docs/api/latest/gdbus-org.freedesktop.NetworkManager.AccessPoint.html#gdbus-property-org-freedesktop-NetworkManager-AccessPoint.Strength>

pub struct Network {
    config: Config,
    state: State,
}

enum State {
    Ok {
        state: Option<NetworkManagerState>,
        primary_connection_type: Option<String>,
    },
    Err {
        message: String,
    }
}

impl Widget for Network {
    type Config = Config;

    type Message = Message;

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>) {
        (Self { config: config.clone(), state: State::Ok { state: None, primary_connection_type: None } }, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match (&mut self.state, message) {
            (State::Ok { state, .. }, Message::NewState(network_manager_state)) => {
                *state = Some(network_manager_state);
                Task::none()
            }
            (State::Ok { primary_connection_type, .. }, Message::NewConnectionType(t)) => {
                *primary_connection_type = Some(t);
                Task::none()
            }
            (_, Message::LaunchSettings) => {
                if let Some(command) = &self.config.settings_command {
                    spawn_detached_command(command, "widget.network.settings_command").discard()
                } else {
                    Task::none()
                }
            }
            (s, Message::Error(message)) => {
                *s = State::Err { message };
                Task::none()
            }
            (State::Err { .. }, _) => Task::none(),
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let widget = match &self.state {
            State::Ok { state, primary_connection_type } => {
                let _ = primary_connection_type;

                let icon = match state {
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
                };

                iced_widget::text(icon).font(Font::with_name("Material Symbols Rounded"))
            }
            State::Err { message, .. } => iced_widget::text(message),
        };
        iced_widget::mouse_area(iced_widget::container(widget).widget_padding()).on_press(Message::LaunchSettings).into()
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        match self.state {
            State::Ok { .. } => Subscription::run(|| iced_runtime::task::sipper(task)),
            State::Err { .. } => Subscription::none(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub settings_command: Option<Box<[String]>>,
}

#[allow(private_interfaces)]
#[derive(Clone)]
pub enum Message {
    NewState(NetworkManagerState),
    NewConnectionType(String),
    LaunchSettings,
    Error(String),
}

async fn task(mut tx: Sender<Message>) {
    let connection = match zbus::Connection::system().await {
        Ok(x) => x,
        Err(e) => {
            tx.send(Message::Error(format!("Failed to connect to system bus: {e}"))).await;
            tracing::error!(error = %e, "Failed to connect to system bus");
            return;
        }
    };
    let proxy = match NetworkManagerProxy::new(&connection).await {
        Ok(x) => x,
        Err(e) => {
            tx.send(Message::Error(format!("Failed to create network manager proxy: {e}"))).await;
            tracing::error!(error = %e, "Failed to create network manager proxy");
            return;
        }
    };
    let mut primary_connection_type_stream = proxy.receive_primary_connection_type_changed().await;
    let mut state_stream = proxy.receive_state_changed().await;

    futures::join!(
        {
            let mut tx = tx.clone();
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
                            tx.send(Message::NewConnectionType(new_primary_connection_type)).await;
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
                        tx.send(Message::NewState(new_state)).await;
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

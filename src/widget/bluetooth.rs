use std::{collections::HashSet, pin::pin};

use bluer::{
    Adapter,
    AdapterEvent,
    AdapterProperty,
    Address,
    DeviceEvent,
    DeviceProperty,
    Session,
    SessionEvent,
};
use futures::StreamExt;
use iced_core::Font;
use iced_futures::Subscription;
use iced_runtime::Task;
use serde::Deserialize;

use crate::{
    application::Element,
    widget::{Widget, spawn_detached_command},
};

pub struct Bluetooth {
    config: Config,
    state: State,
}

enum State {
    Ok {
        powered: Option<bool>,
        discovering: Option<bool>,
        connected_devices: HashSet<Address>,
    },
    Err {
        message: String,
    },
}

impl Widget for Bluetooth {
    type Config = Config;

    type Message = Message;

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>) {
        (
            Self {
                config: config.clone(),
                state: State::Ok {
                    powered: None,
                    discovering: None,
                    connected_devices: HashSet::new(),
                },
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match (&mut self.state, message) {
            (
                State::Ok {
                    connected_devices, ..
                },
                Message::DeviceAdded(address),
            ) => {
                connected_devices.insert(address);
                Task::none()
            }
            (
                State::Ok {
                    connected_devices, ..
                },
                Message::DeviceRemoved(address),
            ) => {
                let was_connected = connected_devices.remove(&address);
                tracing::info!(%address, was_connected, "Removed a device");
                Task::none()
            }
            (State::Ok { powered, .. }, Message::Powered(p)) => {
                *powered = Some(p);
                Task::none()
            }
            (State::Ok { discovering, .. }, Message::Discovering(d)) => {
                *discovering = Some(d);
                Task::none()
            }
            (s, Message::Clear) => {
                *s = State::Ok {
                    powered: None,
                    discovering: None,
                    connected_devices: HashSet::new(),
                };
                Task::none()
            }
            (_, Message::LaunchSettings) => {
                if let Some(command) = &self.config.settings_command {
                    spawn_detached_command(command, "widget.bluetooth.settings_command").discard()
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
            State::Ok {
                powered: Some(true),
                discovering: Some(true),
                ..
            } => iced_widget::text("\u{e1aa}").font(Font::with_name("Material Symbols Rounded")),
            State::Ok {
                powered: Some(true),
                connected_devices,
                ..
            } if connected_devices.is_empty() => {
                iced_widget::text("\u{e1a7}").font(Font::with_name("Material Symbols Rounded"))
            }
            State::Ok {
                powered: Some(true),
                ..
            } => iced_widget::text("\u{e1a7}").font(Font::with_name("Material Symbols Rounded")),
            State::Ok {
                powered: Some(false),
                ..
            } => iced_widget::text("\u{e1a8}").font(Font::with_name("Material Symbols Rounded")),
            State::Ok { powered: None, .. } => {
                iced_widget::text("?").font(Font::with_name("Material Symbols Rounded"))
            }
            State::Err { message } => iced_widget::text(message),
        };
        iced_widget::mouse_area(widget)
            .on_press(Message::LaunchSettings)
            .into()
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

#[derive(Clone)]
pub enum Message {
    DeviceAdded(Address),
    DeviceRemoved(Address),
    Powered(bool),
    Discovering(bool),
    Clear,
    LaunchSettings,
    Error(String),
}

async fn task(mut tx: iced_runtime::task::Sender<Message>) {
    let session = match Session::new().await {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to system bluetooth daemon");
            return;
        }
    };

    let mut adapter = match session.default_adapter().await {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get default bluetooth adapter");
            tx.send(Message::Error(format!(
                "Failed to get default bluetooth adapter: {e}"
            ))).await;
            return;
        }
    };
    loop {
        tracing::info!(default_adapter_name = adapter.name());
        monitor_adapter(adapter, &mut tx).await;

        tx.send(Message::Clear).await;
        tracing::warn!("event stream of default adapter ended");

        match session.default_adapter().await {
            Ok(new_default_adapter) => {
                adapter = new_default_adapter;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to get default adapter, waiting for adapter added event");
                let mut events = pin!(session.events().await.unwrap());
                loop {
                    let event = events.next().await;
                    match event {
                        Some(SessionEvent::AdapterAdded(_)) => {
                            break;
                        }
                        Some(SessionEvent::AdapterRemoved(_)) => (),
                        None => {
                            tracing::warn!("Event stream of bluetooth session ended");
                            break;
                        }
                    }
                }
                adapter = match session.default_adapter().await {
                    Ok(x) => x,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to get default adapter");
                        break;
                    }
                }
            }
        }
    }
}

async fn monitor_adapter(adapter: Adapter, tx: &mut iced_runtime::task::Sender<Message>) {
    match adapter.is_powered().await {
        Ok(is_powered) => {
            tracing::info!(is_powered, "Adapter property");
            tx.send(Message::Powered(is_powered)).await;
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get if default adapter is powered");
        }
    }
    match adapter.is_discovering().await {
        Ok(discovering) => {
            tracing::info!(discovering, "Adapter property");
            tx.send(Message::Discovering(discovering)).await;
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get if default adapter is discovering");
        }
    }
    match adapter.device_addresses().await {
        Ok(addresses) => {
            for address in addresses {
                try_monitor_device(&adapter, address, tx).await;
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get addresses of discovered devices");
        }
    }
    let mut events = match adapter.events().await {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get event stream of default adapter");
            tx.send(Message::Error(format!(
                "Failed to get event stream of default adapter: {e}"
            )))
            .await;
            return;
        }
    };
    while let Some(event) = events.next().await {
        tracing::debug!(?event, "Bluetooth event");
        match event {
            AdapterEvent::DeviceAdded(address) => {
                try_monitor_device(&adapter, address, tx).await;
            }
            AdapterEvent::DeviceRemoved(address) => {
                tx.send(Message::DeviceRemoved(address)).await;
            }
            AdapterEvent::PropertyChanged(AdapterProperty::Powered(powered)) => {
                tracing::info!(powered, "Adapter property changed");
                tx.send(Message::Powered(powered)).await;
            }
            AdapterEvent::PropertyChanged(AdapterProperty::Discovering(discovering)) => {
                tracing::info!(discovering, "Adapter property changed");
                tx.send(Message::Discovering(discovering)).await;
            }
            _ => (),
        }
    }
}

async fn try_monitor_device(
    adapter: &Adapter,
    address: Address,
    tx: &mut iced_runtime::task::Sender<Message>,
) {
    let device = match adapter.device(address) {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(%address, error = %e, "Device added, but failed to get the device at that address");
            return;
        }
    };
    let device_name = device.name().await;
    match device.is_connected().await {
        Ok(is_connected) => {
            tracing::info!(%address, ?device_name, is_connected, "Device property");
            if is_connected {
                tx.send(Message::DeviceAdded(address)).await;
            }
        }
        Err(e) => {
            tracing::error!(%address, ?device_name, error = %e, "Failed to get if device is connected");
        }
    }
    let mut events = match device.events().await {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(%address, ?device_name, error = %e, "Failed to get device event stream");
            return;
        }
    };
    tracing::info!(%address, ?device_name, "Monitoring a device");
    let mut tx = tx.clone();
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                DeviceEvent::PropertyChanged(DeviceProperty::Connected(connected)) => {
                    tracing::info!(%address, connected, "Device property changed");
                    tx.send(if connected {
                        Message::DeviceAdded(address)
                    } else {
                        Message::DeviceRemoved(address)
                    })
                    .await;
                }
                _ => (),
            }
        }
    });
}

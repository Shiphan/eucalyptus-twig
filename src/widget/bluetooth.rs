use std::{collections::HashSet, pin::pin};

use async_compat::Compat;
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
use gpui::{
    AsyncApp,
    Context,
    InteractiveElement,
    IntoElement,
    ParentElement,
    Render,
    StatefulInteractiveElement,
    WeakEntity,
    Window,
};
use serde::Deserialize;

use crate::widget::{Widget, spawn_detached_command, widget_wrapper};

pub struct Bluetooth {
    config: BluetoothConfig,
    error_message: Option<String>,
    powered: Option<bool>,
    discovering: Option<bool>,
    connected_devices: HashSet<Address>,
}

impl Widget for Bluetooth {
    type Config = BluetoothConfig;

    fn new(cx: &mut Context<Self>, config: &Self::Config) -> Self {
        cx.spawn(async |this, cx| Compat::new(task(this, cx)).await)
            .detach();

        Self {
            config: config.clone(),
            error_message: None,
            powered: None,
            discovering: None,
            connected_devices: HashSet::new(),
        }
    }
}

impl Bluetooth {
    fn clear(&mut self) {
        self.error_message = None;
        self.powered = None;
        self.discovering = None;
        self.connected_devices = HashSet::new();
    }
}

impl Render for Bluetooth {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let widget = if let Some(e) = &self.error_message {
            widget_wrapper().child(e.clone())
        } else {
            match self.powered {
                Some(true) => {
                    if self.discovering == Some(true) {
                        widget_wrapper().child("\u{e1aa}")
                    } else if self.connected_devices.len() == 0 {
                        widget_wrapper().child("\u{e1a7}")
                    } else {
                        widget_wrapper().child("\u{e1a8}")
                    }
                }
                Some(false) => widget_wrapper().child("\u{e1a9}"),
                None => widget_wrapper().child("?"),
            }
        };

        if let Some(command) = &self.config.settings_command {
            let command = command.clone();
            widget
                .id("network")
                .on_click(move |_, _, cx| {
                    spawn_detached_command(
                        cx,
                        command.as_ref(),
                        "widget.bluetooth.settings_command",
                    )
                })
                .into_any_element()
        } else {
            widget.into_any_element()
        }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BluetoothConfig {
    pub settings_command: Option<Box<[String]>>,
}

async fn task(this: WeakEntity<Bluetooth>, cx: &mut AsyncApp) {
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
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("Failed to get default bluetooth adapter: {e}"));
                cx.notify();
            });
            return;
        }
    };
    loop {
        tracing::info!(default_adapter_name = adapter.name());
        monitor_adapter(adapter, &this, cx).await;

        let _ = this.update(cx, |this, cx| {
            this.clear();
            cx.notify();
        });
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

async fn monitor_adapter(adapter: Adapter, entity: &WeakEntity<Bluetooth>, cx: &mut AsyncApp) {
    match adapter.is_powered().await {
        Ok(is_powered) => {
            tracing::info!(is_powered, "Adapter property");
            let _ = entity.update(cx, |this, cx| {
                this.powered = Some(is_powered);
                cx.notify();
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get if default adapter is powered");
        }
    }
    match adapter.is_discovering().await {
        Ok(discovering) => {
            tracing::info!(discovering, "Adapter property");
            let _ = entity.update(cx, |this, cx| {
                this.discovering = Some(discovering);
                cx.notify();
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get if default adapter is discovering");
        }
    }
    match adapter.device_addresses().await {
        Ok(addresses) => {
            for address in addresses {
                try_monitor_device(&adapter, address, entity.clone(), cx).await;
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
            let _ = entity.update(cx, |this, cx| {
                this.error_message = Some(format!(
                    "Failed to get event stream of default adapter: {e}"
                ));
                cx.notify();
            });
            return;
        }
    };
    while let Some(event) = events.next().await {
        tracing::debug!(?event, "Bluetooth event");
        match event {
            AdapterEvent::DeviceAdded(address) => {
                try_monitor_device(&adapter, address, entity.clone(), cx).await;
            }
            AdapterEvent::DeviceRemoved(address) => {
                let _ = entity.update(cx, |this, cx| {
                    let was_connected = this.connected_devices.remove(&address);
                    tracing::info!(%address, was_connected, "Removed a device");
                    cx.notify();
                });
            }
            AdapterEvent::PropertyChanged(AdapterProperty::Powered(powered)) => {
                tracing::info!(powered, "Adapter property changed");
                let _ = entity.update(cx, |this, cx| {
                    this.powered = Some(powered);
                    cx.notify();
                });
            }
            AdapterEvent::PropertyChanged(AdapterProperty::Discovering(discovering)) => {
                tracing::info!(discovering, "Adapter property changed");
                let _ = entity.update(cx, |this, cx| {
                    this.discovering = Some(discovering);
                    cx.notify();
                });
            }
            _ => (),
        }
    }
}

async fn try_monitor_device(
    adapter: &Adapter,
    address: Address,
    entity: WeakEntity<Bluetooth>,
    cx: &mut AsyncApp,
) {
    let device = match adapter.device(address) {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(%address, error = %e, "Device added, but failed to get the device at that address");
            return;
        }
    };
    match device.is_connected().await {
        Ok(is_connected) => {
            tracing::info!(%address, name = ?device.name().await, is_connected, "Device property");
            let _ = entity.update(cx, |this, cx| {
                if is_connected {
                    this.connected_devices.insert(address);
                }
                cx.notify();
            });
        }
        Err(e) => {
            tracing::error!(%address, name = ?device.name().await, error = %e, "Failed to get if device is connected");
        }
    }
    let mut events = match device.events().await {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(%address, name = ?device.name().await, error = %e, "Failed to get device event stream");
            return;
        }
    };
    tracing::info!(%address, name = ?device.name().await, "Monitoring a device");
    cx.spawn(async move |cx| {
        while let Some(event) = events.next().await {
            match event {
                DeviceEvent::PropertyChanged(
                    DeviceProperty::Connected(connected),
                ) => {
                    let _ = entity.update(cx, |this, cx| {
                        let was_connected = if connected {
                            !this.connected_devices.insert(address)
                        } else {
                            this.connected_devices.remove(&address)
                        };
                        tracing::info!(%address, connected, was_connected, "Device property changed");
                        cx.notify();
                    });
                }
                _ => (),
            }
        }
    })
    .detach();
}

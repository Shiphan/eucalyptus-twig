use std::collections::HashSet;

use bluer::{
    Adapter, AdapterEvent, AdapterProperty, Address, DeviceEvent, DeviceProperty, Session,
};
use futures::StreamExt;
use gpui::{AsyncApp, Context, IntoElement, ParentElement, Render, WeakEntity, Window};
use gpui_tokio::Tokio;

use crate::widget::{Widget, widget_wrapper};

pub struct Bluetooth {
    error_message: Option<String>,
    powered: Option<bool>,
    discovering: Option<bool>,
    connected_devices: HashSet<Address>,
}

impl Widget for Bluetooth {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(task).detach();

        Self {
            error_message: None,
            powered: None,
            discovering: None,
            connected_devices: HashSet::new(),
        }
    }
}

impl Render for Bluetooth {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            widget_wrapper().child(e.clone())
        } else {
            match self.powered {
                Some(true) => {
                    if self.discovering == Some(true) {
                        widget_wrapper().child("")
                    } else if self.connected_devices.len() == 0 {
                        widget_wrapper().child("")
                    } else {
                        widget_wrapper().child("")
                    }
                }
                Some(false) => widget_wrapper().child(""),
                None => widget_wrapper().child("?"),
            }
        }
    }
}

async fn task(this: WeakEntity<Bluetooth>, cx: &mut AsyncApp) {
    let handle = match cx.update(|cx| Tokio::handle(cx)) {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get tokio handle, which is required for bluer crate to work");
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("Failed to get tokio handle: {e}"));
                cx.notify()
            });
            return;
        }
    };
    let _guard = handle.enter();

    let adapter = match default_adapter().await {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to get default bluetooth adapter");
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("Failed to get default bluetooth adapter: {e}"));
                cx.notify()
            });
            return;
        }
    };
    tracing::info!(default_adapter_name = adapter.name());
    match adapter.is_powered().await {
        Ok(is_powered) => {
            tracing::info!(is_powered, "Adapter property");
            let _ = this.update(cx, |this, cx| {
                this.powered = Some(is_powered);
                cx.notify()
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get if default adapter is powered");
        }
    }
    match adapter.is_discovering().await {
        Ok(discovering) => {
            tracing::info!(discovering, "Adapter property");
            let _ = this.update(cx, |this, cx| {
                this.discovering = Some(discovering);
                cx.notify()
            });
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to get if default adapter is discovering");
        }
    }
    match adapter.device_addresses().await {
        Ok(addresses) => {
            for address in addresses {
                try_monitor_device(&adapter, address, this.clone(), cx).await;
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
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!(
                    "Failed to get event stream of default adapter: {e}"
                ));
                cx.notify()
            });
            return;
        }
    };
    while let Some(event) = events.next().await {
        tracing::debug!(?event, "Bluetooth event");
        match event {
            AdapterEvent::DeviceAdded(address) => {
                try_monitor_device(&adapter, address, this.clone(), cx).await;
            }
            AdapterEvent::DeviceRemoved(address) => {
                let _ = this.update(cx, |this, cx| {
                    let was_connected = this.connected_devices.remove(&address);
                    tracing::info!(%address, was_connected, "Removed a device");
                    cx.notify()
                });
            }
            AdapterEvent::PropertyChanged(AdapterProperty::Powered(powered)) => {
                tracing::info!(powered, "Adapter property changed");
                let _ = this.update(cx, |this, cx| {
                    this.powered = Some(powered);
                    cx.notify()
                });
            }
            AdapterEvent::PropertyChanged(AdapterProperty::Discovering(discovering)) => {
                tracing::info!(discovering, "Adapter property changed");
                let _ = this.update(cx, |this, cx| {
                    this.discovering = Some(discovering);
                    cx.notify()
                });
            }
            _ => (),
        }
    }
    tracing::warn!("event stream of default adapter ended");
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
                            this.connected_devices.insert(address)
                        } else {
                            this.connected_devices.remove(&address)
                        };
                        tracing::info!(%address, connected, was_connected, "Device property changed");
                        cx.notify()
                    });
                }
                _ => (),
            }
        }
    })
    .detach();
}

async fn default_adapter() -> bluer::Result<Adapter> {
    let session = Session::new().await?;
    session.default_adapter().await
}

use std::collections::HashMap;

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
    devices_connected: HashMap<Address, bool>,
    connected_count: usize,
}

impl Widget for Bluetooth {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(task).detach();

        Self {
            error_message: None,
            powered: None,
            discovering: None,
            devices_connected: HashMap::new(),
            connected_count: 0,
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
                    } else if self.connected_count == 0 {
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
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("Error while getting tokio handle: {e}"));
                cx.notify()
            });

            println!("Error while getting tokio handle: {e}");
            return;
        }
    };
    let _guard = handle.enter();

    match default_adapter().await {
        Ok(adapter) => {
            match adapter.is_powered().await {
                Ok(is_powered) => {
                    let _ = this.update(cx, |this, cx| {
                        this.powered = Some(is_powered);
                        cx.notify()
                    });
                }
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.error_message = Some(format!("{} error: {e}", line!()));
                        cx.notify()
                    });
                }
            }
            match adapter.is_discovering().await {
                Ok(discovering) => {
                    let _ = this.update(cx, |this, cx| {
                        this.discovering = Some(discovering);
                        cx.notify()
                    });
                }
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.error_message = Some(format!("{} error: {e}", line!()));
                        cx.notify()
                    });
                }
            }
            let mut events = match adapter.events().await {
                Ok(x) => x,
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        this.error_message = Some(format!(
                            "error while getting events of default adapter: {e}"
                        ));
                        cx.notify()
                    });
                    return;
                }
            };
            while let Some(event) = events.next().await {
                println!("Bluetooth event = {event:#?}");
                match event {
                    AdapterEvent::DeviceAdded(address) => match adapter.device(address) {
                        // TODO: listen on device event
                        Ok(device) => {
                            match device.is_connected().await {
                                Ok(is_connected) => {
                                    let _ = this.update(cx, |this, cx| {
                                        if is_connected {
                                            this.connected_count += 1;
                                        }
                                        this.devices_connected.insert(address, is_connected);
                                        cx.notify();
                                    });
                                }
                                Err(e) => {
                                    let _ = this.update(cx, |this, cx| {
                                        this.error_message =
                                            Some(format!("Error {}: {e}", line!()));
                                        cx.notify()
                                    });
                                }
                            }
                            match device.events().await {
                                Ok(mut events) => {
                                    let this = this.clone();
                                    cx.spawn(async move |cx| {
                                        while let Some(event) = events.next().await {
                                            match event {
                                                DeviceEvent::PropertyChanged(
                                                    DeviceProperty::Connected(connected),
                                                ) => {
                                                    let _ = this.update(cx, |this, cx| {
                                                        let was_connected = this
                                                            .devices_connected
                                                            .insert(address, connected);
                                                        if let Some(was_connected) = was_connected
                                                            && was_connected != connected
                                                        {
                                                            if connected {
                                                                this.connected_count += 1;
                                                            } else {
                                                                this.connected_count -= 1;
                                                            }
                                                        }
                                                        cx.notify()
                                                    });
                                                }
                                                _ => (),
                                            }
                                        }
                                    })
                                    .detach();
                                }
                                Err(e) => {
                                    let _ = this.update(cx, |this, cx| {
                                        this.error_message =
                                            Some(format!("{} error: {e}", line!()));
                                        cx.notify()
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            let _ = this.update(cx, |this, cx| {
                                this.error_message = Some(format!("Error {}: {e}", line!()));
                                cx.notify()
                            });
                        }
                    },
                    AdapterEvent::DeviceRemoved(address) => {
                        let _ = this.update(cx, |this, cx| {
                            let was_connected = this.devices_connected.remove(&address);
                            if was_connected == Some(true) {
                                this.connected_count -= 1;
                            }
                            println!("remove device with address `{address}`: {was_connected:?}");
                            cx.notify()
                        });
                    }
                    AdapterEvent::PropertyChanged(AdapterProperty::Powered(powered)) => {
                        let _ = this.update(cx, |this, cx| {
                            this.powered = Some(powered);
                            cx.notify()
                        });
                    }
                    AdapterEvent::PropertyChanged(AdapterProperty::Discovering(discovering)) => {
                        let _ = this.update(cx, |this, cx| {
                            this.discovering = Some(discovering);
                            cx.notify()
                        });
                    }
                    _ => (),
                }
            }
        }
        Err(e) => {
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("error while getting the default adapter: {e}"));
                cx.notify()
            });
        }
    }
}

async fn default_adapter() -> bluer::Result<Adapter> {
    let session = Session::new().await?;
    session.default_adapter().await
}

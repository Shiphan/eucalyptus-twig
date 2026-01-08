use std::time::Duration;

use futures::{StreamExt, join};
use gpui::{
    AsyncApp, Context, IntoElement, ParentElement, Render, Styled, WeakEntity, Window, div, rems,
};
use zbus::{
    Connection, proxy,
    zvariant::{ObjectPath, OwnedObjectPath},
};

use crate::widget::{Widget, widget_wrapper};

#[derive(Clone)]
pub struct Power {
    error_message: Option<String>,
    type_: Option<u32>,
    state: Option<u32>,
    percentage: Option<f64>,
    time_to_empty: Option<Duration>,
    time_to_full: Option<Duration>,
}

impl Widget for Power {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(task).detach();

        Self {
            error_message: None,
            type_: None,
            state: None,
            percentage: None,
            time_to_empty: None,
            time_to_full: None,
        }
    }
}

impl Render for Power {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            widget_wrapper().child(e.clone())
        } else if self.type_ == Some(2)
            && let Some(state) = self.state
            && let Some(percentage) = self.percentage
        {
            match state {
                // Charging
                1 => widget_wrapper()
                    .flex()
                    .gap(rems(0.25))
                    .child(div().font_family("Material Symbols Rounded").child(
                        if percentage >= 100.0 {
                            ""
                        } else if percentage >= 80.0 {
                            ""
                        } else if percentage >= 70.0 {
                            ""
                        } else if percentage >= 50.0 {
                            ""
                        } else if percentage >= 40.0 {
                            ""
                        } else if percentage >= 20.0 {
                            ""
                        } else if percentage >= 10.0 {
                            ""
                        } else {
                            ""
                        },
                    ))
                    .child(format!("{:.0}", percentage)),
                // Discharging
                2 => widget_wrapper()
                    .flex()
                    .gap(rems(0.25))
                    .child(div().font_family("Material Symbols Rounded").child(
                        if percentage >= 100.0 {
                            ""
                        } else if percentage >= 80.0 {
                            ""
                        } else if percentage >= 70.0 {
                            ""
                        } else if percentage >= 50.0 {
                            ""
                        } else if percentage >= 40.0 {
                            ""
                        } else if percentage >= 20.0 {
                            ""
                        } else if percentage >= 10.0 {
                            ""
                        } else {
                            ""
                        },
                    ))
                    .child(format!("{:.0}", percentage)),
                // Empty
                3 => widget_wrapper()
                    .flex()
                    .gap(rems(0.25))
                    .child("")
                    .child(format!("{:.0}", percentage)),
                // Fully charged
                4 => widget_wrapper()
                    .flex()
                    .gap(rems(0.25))
                    .child("")
                    .child(format!("{:.0}", percentage)),
                _ => widget_wrapper().child(format!("Other state: {state}")),
            }
        } else {
            widget_wrapper().child("?")
            // let Self {
            //     error_message: _,
            //     type_,
            //     state,
            //     percentage,
            //     time_to_empty,
            //     time_to_full,
            // } = self.clone();
            // widget_wrapper().child(format!("type = {type_:?}, state = {state:?}, percentage = {percentage:?}, time_to_empty = {time_to_empty:?}, time_to_full = {time_to_full:?}"))
        }
    }
}

async fn task(this: WeakEntity<Power>, cx: &mut AsyncApp) {
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
    let display_device_proxy =
        match UpowerDeviceProxy::new(&connection, "/org/freedesktop/UPower/devices/DisplayDevice")
            .await
        {
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
    let mut type_stream = display_device_proxy.receive_type__changed().await;
    let mut state_stream = display_device_proxy.receive_state_changed().await;
    let mut percentage_stream = display_device_proxy.receive_percentage_changed().await;
    let mut time_to_empty_stream = display_device_proxy.receive_time_to_empty_changed().await;
    let mut time_to_full_stream = display_device_proxy.receive_time_to_full_changed().await;
    macro_rules! handle_stream {
        ($stream:expr, $field:ident, $name:literal $(, $and_then:expr)?) => {
            {
                let mut cx = cx.clone();
                let this = &this;
                async move {
                    while let Some($field) = $stream.next().await {
                        match $field.get().await {
                            Ok($field) => {
                                tracing::info!($field, concat!($name, " changed"));
                                let _ = this.update(&mut cx, |this, cx| {
                                    this.$field = Some($field)$(.and_then($and_then))?;
                                    cx.notify()
                                });
                            }
                            Err(e) => {
                                tracing::error!(error = %e, concat!("Failed to get new ", $name));
                            }
                        }
                    }
                    tracing::warn!(concat!("Receive ", $name ," stream ended"));
                }
            }
        };
    }
    join!(
        handle_stream!(type_stream, type_, "Type"),
        handle_stream!(state_stream, state, "State"),
        handle_stream!(percentage_stream, percentage, "Percentage"),
        handle_stream!(
            time_to_empty_stream,
            time_to_empty,
            "TimeToEmpty",
            |x| if x != 0
                && let Ok(x) = x.try_into()
            {
                Some(Duration::from_secs(x))
            } else {
                None
            }
        ),
        handle_stream!(
            time_to_full_stream,
            time_to_full,
            "TimeToFull",
            |x| if x != 0
                && let Ok(x) = x.try_into()
            {
                Some(Duration::from_secs(x))
            } else {
                None
            }
        ),
    );
}

// <https://upower.freedesktop.org/docs/UPower.html>
#[proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait Upower {
    fn enumerate_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn enumerate_kbd_backlights(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn get_display_device(&self) -> zbus::Result<OwnedObjectPath>;
    fn get_critical_Action(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn device_added(&self, device: ObjectPath<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_removed(&self, device: ObjectPath<'_>) -> zbus::Result<()>;

    #[zbus(property)]
    fn daemon_version(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn lid_is_closed(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn lid_is_present(&self) -> zbus::Result<bool>;
}

// <https://upower.freedesktop.org/docs/Device.html>
#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
trait UpowerDevice {
    fn refresh(&self) -> zbus::Result<()>;
    fn get_history(
        &self,
        type_: String,
        timespan: u32,
        resolution: u32,
    ) -> zbus::Result<Vec<(u32, f64, u32)>>;
    fn get_statistics(&self, type_: String) -> zbus::Result<Vec<(f64, f64)>>;
    fn enable_charge_threshold(&self, charge_threshold: bool) -> zbus::Result<()>;

    #[zbus(property)]
    fn native_path(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn vendor(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn serial(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn update_time(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn type_(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn power_supply(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn has_history(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn has_statistics(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn online(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn energy(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn energy_empty(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn energy_full(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn energy_full_design(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn energy_rate(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn voltage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn charge_cycles(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn luminosity(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn temperature(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn is_rechargeable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn capacity(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn technology(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn warning_level(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn battery_level(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn icon_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn charge_start_threshold(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn charge_end_threshold(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn charge_threshold_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn charge_threshold_supported(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn charge_threshold_settings_supported(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn voltage_min_design(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn voltage_max_design(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn capacity_level(&self) -> zbus::Result<String>;
}

// <https://upower.freedesktop.org/docs/KbdBacklight.html>
#[proxy(
    interface = "org.freedesktop.UPower.KbdBacklight",
    default_service = "org.freedesktop.UPower"
)]
trait UpowerKbdBacklight {
    fn get_max_brightness(&self) -> zbus::Result<i32>;
    fn get_brightness(&self) -> zbus::Result<i32>;
    fn set_brightness(&self, value: i32) -> zbus::Result<()>;

    #[zbus(signal)]
    fn brightness_changed(&self, value: i32) -> zbus::Result<()>;
    #[zbus(signal)]
    fn brightness_changed_with_source(&self, value: i32, source: String) -> zbus::Result<()>;

    #[zbus(property)]
    fn native_path(&self) -> zbus::Result<String>;
}

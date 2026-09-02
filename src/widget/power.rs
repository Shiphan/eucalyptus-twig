use std::time::Duration;

use futures::{StreamExt, join};
use iced_core::Font;
use iced_futures::Subscription;
use iced_runtime::Task;
use serde_repr::Deserialize_repr;
use zbus::{
    Connection,
    proxy,
    zvariant::{self, ObjectPath, OwnedObjectPath},
};

use crate::{
    application::Element,
    widget::{Widget, WidgetPadding},
};

#[allow(private_interfaces)]
pub enum Power {
    Ok {
        kind: Option<u32>,
        state: Option<BatteryPowerState>,
        percentage: Option<f64>,
        time_to_empty: Option<Duration>,
        time_to_full: Option<Duration>,
    },
    Err {
        message: String,
    },
}

impl Widget for Power {
    type Config = ();

    type Message = Message;

    fn new((): &Self::Config) -> (Self, Task<Self::Message>) {
        (
            Self::Ok {
                kind: None,
                state: None,
                percentage: None,
                time_to_empty: None,
                time_to_full: None,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match (self, message) {
            (Power::Ok { kind, .. }, Message::NewKind(k)) => {
                *kind = k;
            }
            (Power::Ok { state, .. }, Message::NewState(s)) => {
                *state = s;
            }
            (Power::Ok { percentage, .. }, Message::NewPercentage(p)) => {
                *percentage = p;
            }
            (Power::Ok { time_to_empty, .. }, Message::NewTimeToEmpty(t)) => {
                *time_to_empty = t;
            }
            (Power::Ok { time_to_full, .. }, Message::NewTimeToFull(t)) => {
                *time_to_full = t;
            }
            (s, Message::Error(message)) => {
                *s = Self::Err { message };
            }
            (Power::Err { .. }, _) => (),
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self {
            Power::Ok {
                kind: Some(2),
                state: Some(state),
                percentage: Some(percentage),
                time_to_empty,
                time_to_full,
            } => {
                let _ = (time_to_empty, time_to_full);
                let percentage = *percentage;
                match state {
                    BatteryPowerState::Charging => iced_widget::row![
                        iced_widget::text(if percentage >= 100.0 {
                            "\u{e1a4}"
                        } else if percentage >= 80.0 {
                            "\u{f0a7}"
                        } else if percentage >= 70.0 {
                            "\u{f0a6}"
                        } else if percentage >= 50.0 {
                            "\u{f0a5}"
                        } else if percentage >= 40.0 {
                            "\u{f0a4}"
                        } else if percentage >= 20.0 {
                            "\u{f0a3}"
                        } else if percentage >= 10.0 {
                            "\u{f0a2}"
                        } else {
                            "\u{e1a3}"
                        },)
                        .font(Font::with_name("Material Symbols Rounded")),
                        iced_widget::text!("{:.0}", percentage),
                    ]
                    .widget_padding()
                    .into(),
                    BatteryPowerState::Discharging => iced_widget::row![
                        iced_widget::text(if percentage >= 100.0 {
                            "\u{e1a4}"
                        } else if percentage >= 80.0 {
                            "\u{ebd2}"
                        } else if percentage >= 70.0 {
                            "\u{ebd4}"
                        } else if percentage >= 50.0 {
                            "\u{ebe2}"
                        } else if percentage >= 40.0 {
                            "\u{ebdd}"
                        } else if percentage >= 20.0 {
                            "\u{ebe0}"
                        } else if percentage >= 10.0 {
                            "\u{ebd9}"
                        } else {
                            "\u{ebdc}"
                        },)
                        .font(Font::with_name("Material Symbols Rounded")),
                        iced_widget::text!("{:.0}", percentage),
                    ]
                    .widget_padding()
                    .into(),
                    BatteryPowerState::Empty => iced_widget::row![
                        iced_widget::text("\u{ebdc}")
                            .font(Font::with_name("Material Symbols Rounded")),
                        iced_widget::text!("{:.0}", percentage),
                    ]
                    .widget_padding()
                    .into(),
                    BatteryPowerState::FullyCharged
                    | BatteryPowerState::PendingCharge
                    | BatteryPowerState::PendingDischarge => iced_widget::row![
                        iced_widget::text("\u{f7eb}")
                            .font(Font::with_name("Material Symbols Rounded")),
                        iced_widget::text!("{:.0}", percentage),
                    ]
                    .widget_padding()
                    .into(),
                    BatteryPowerState::Unknown => iced_widget::container(iced_widget::text("State: Unknown")).widget_padding().into(),
                }
            }
            Power::Ok { .. } => iced_widget::container(iced_widget::text("?")).widget_padding().into(),
            Power::Err { message } => iced_widget::container(iced_widget::text(message)).widget_padding().into(),
        }
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        match self {
            Self::Ok { .. } => Subscription::run(|| iced_runtime::task::sipper(task)),
            Self::Err { .. } => Subscription::none(),
        }
    }
}

#[allow(private_interfaces)]
pub enum Message {
    NewKind(Option<u32>),
    NewState(Option<BatteryPowerState>),
    NewPercentage(Option<f64>),
    NewTimeToEmpty(Option<Duration>),
    NewTimeToFull(Option<Duration>),
    Error(String),
}

async fn task(mut tx: iced_runtime::task::Sender<Message>) {
    let connection = match Connection::system().await {
        Ok(x) => x,
        Err(e) => {
            tx.send(Message::Error(format!(
                "Failed to connect to system bus: {e}"
            )))
            .await;
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
                tx.send(Message::Error(format!(
                    "Failed to create properties proxy: {e}"
                )))
                .await;
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
        ($stream:expr, $message:ident, $name:literal $(, $and_then:expr)?) => {
            {
                let mut tx = tx.clone();
                async move {
                    while let Some(new) = $stream.next().await {
                        match new.get().await {
                            Ok(new) => {
                                tracing::info!(?new, concat!($name, " changed"));
                                tx.send(Message::$message(Some(new)$( .and_then($and_then) )?)).await;
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
        handle_stream!(type_stream, NewKind, "Type"),
        handle_stream!(state_stream, NewState, "State"),
        handle_stream!(percentage_stream, NewPercentage, "Percentage"),
        handle_stream!(
            time_to_empty_stream,
            NewTimeToEmpty,
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
            NewTimeToFull,
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

#[derive(Clone, Debug, Deserialize_repr, zvariant::OwnedValue)]
#[repr(u32)]
enum BatteryPowerState {
    Unknown = 0,
    Charging = 1,
    Discharging = 2,
    Empty = 3,
    FullyCharged = 4,
    PendingCharge = 5,
    PendingDischarge = 6,
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
    fn state(&self) -> zbus::Result<BatteryPowerState>;
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

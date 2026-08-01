use futures::{
    StreamExt, channel::mpsc::{self, UnboundedSender}, join, lock::Mutex
};
use iced_core::Font;
use iced_futures::Subscription;
use iced_runtime::Task;
use serde::Deserialize;
use zbus::{Connection, proxy, zvariant};

use crate::{application::Element, widget::Widget};

pub struct PowerProfile {
    config: Config,
    tx: Option<UnboundedSender<CycleDirection>>,
    state: State,
}

enum State {
    Ok {
        active_profile: Option<String>,
    },
    Err {
        message: String,
    },
}

impl Widget for PowerProfile {
    type Config = Config;

    type Message = Message;

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>) {
        (Self { config: config.clone(), tx: None, state: State::Ok { active_profile: None } }, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match (&mut self.state, message) {
            (State::Ok { active_profile }, Message::NewActiveProfile(a)) => {
                *active_profile = Some(a);
            }
            (State::Ok { .. }, Message::Cycle) => {
                if let Some(tx) = &self.tx {
                    let _ = tx.unbounded_send(self.config.cycle_direction);
                }
            }
            (s, Message::Error(message)) => {
                *s = State::Err { message };
            }
            (_, Message::NewCycleTx(tx)) => {
                self.tx = Some(tx);
            }
            (State::Err { .. }, _) => ()
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match &self.state {
            State::Ok { active_profile: Some(active_profile) } => iced_widget::mouse_area(match active_profile.as_str() {
                "power-saver" => iced_widget::text("\u{ec1a}").font(Font::with_name("Material Symbols Rounded")),
                "balanced" => iced_widget::text("\u{e9e4}").font(Font::with_name("Material Symbols Rounded")),
                "performance" => iced_widget::text("\u{eb9b}").font(Font::with_name("Material Symbols Rounded")),
                _ => iced_widget::text(active_profile),
            }).on_press(Message::Cycle).into(),
            State::Ok { active_profile: None } => iced_widget::text("?").into(),
            State::Err { message } => iced_widget::text(message).into(),
        }
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
    cycle_direction: CycleDirection,
}

#[derive(Clone, Copy, Default, Deserialize)]
enum CycleDirection {
    Up,
    #[default]
    Down,
}

#[allow(private_interfaces)]
#[derive(Clone)]
pub enum Message {
    NewActiveProfile(String),
    Cycle,
    NewCycleTx(UnboundedSender<CycleDirection>),
    Error(String),
}

async fn task(
    mut tx: iced_runtime::task::Sender<Message>,
) {
    let (cycle_tx, mut cycle_rx) = mpsc::unbounded();
    tx.send(Message::NewCycleTx(cycle_tx)).await;

    let connection = match Connection::system().await {
        Ok(x) => x,
        Err(e) => {
            tx.send(Message::Error(format!("Failed to connect to system bus: {e}"))).await;
            tracing::error!(error = %e, "Failed to connect to system bus");
            return;
        }
    };
    let proxy = match PowerProfilesProxy::new(&connection).await {
        Ok(x) => x,
        Err(e) => {
            tx.send(Message::Error(format!("Failed to create properties proxy: {e}"))).await;
            tracing::error!(error = %e, "Failed to create properties proxy");
            return;
        }
    };
    let mut active_profile_stream = proxy.receive_active_profile_changed().await;
    let active_profile = Mutex::new(None);
    let mut profiles_stream = proxy.receive_profiles_changed().await;
    let profiles = Mutex::new(None);
    join!(
        async {
            while let Some(new_active_profile) = active_profile_stream.next().await {
                match new_active_profile.get().await {
                    Ok(new_active_profile) => {
                        tracing::info!(new_active_profile, "Power profile changed");
                        tx.send(Message::NewActiveProfile(new_active_profile.clone())).await;
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
            while let Some(new_profiles) = profiles_stream.next().await {
                match new_profiles.get().await {
                    Ok(new_profiles) => {
                        tracing::info!(?new_profiles, "Power profile changed");
                        let new_profiles = new_profiles
                            .into_iter()
                            .map(|Profile { profile }| profile)
                            .collect();
                        profiles.lock().await.replace(new_profiles);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to get new ActiveProfile");
                    }
                }
            }
            tracing::warn!("Receive ActiveProfile stream ended");
        },
        async {
            while let Some(cycle_direction) = cycle_rx.next().await {
                let active_profile = active_profile.lock().await;
                let profiles = profiles.lock().await;
                let target_profile = match (
                    active_profile.as_ref(),
                    profiles.as_ref().map(Vec::as_slice),
                ) {
                    (Some(active_profile), Some(profiles)) => match (
                        profiles.iter().position(|x| x == active_profile),
                        cycle_direction,
                    ) {
                        (Some(position), CycleDirection::Up) if position == profiles.len() => {
                            profiles.first()
                        }
                        (Some(position), CycleDirection::Up) => profiles.get(position + 1),
                        (Some(position), CycleDirection::Down) if position == 0 => profiles.last(),
                        (Some(position), CycleDirection::Down) => profiles.get(position - 1),
                        (None, _) => profiles.first(),
                    }
                    .map(String::as_str),
                    (Some(active_profile), None) => Some(active_profile.as_str()),
                    (None, Some([profile, ..])) => Some(profile.as_str()),
                    (None, Some([])) | (None, None) => None,
                };
                let target_profile = target_profile.unwrap_or("balanced"); // Default to balanced
                if let Err(e) = proxy.set_active_profile(target_profile).await {
                    tracing::error!(error = %e, "Failed to set active profile");
                }
            }
        },
    );
}

// <https://upower.pages.freedesktop.org/power-profiles-daemon/gdbus-org.freedesktop.UPower.PowerProfiles.html>
#[proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_service = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PowerProfiles {
    fn hold_profile(&self, profile: &str, reason: &str, application_id: &str) -> zbus::Result<u32>;
    fn release_profile(&self, cookie: u32) -> zbus::Result<()>;
    fn set_action_enabled(&self, action: &str, enabled: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn profile_released(&self, cookie: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, active_profile: &str) -> zbus::Result<()>;
    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<Profile>>;
}

#[derive(Debug, zvariant::Value)]
#[zvariant(signature = "a{sv}")]
struct Profile {
    #[zvariant(rename = "Profile")]
    profile: String,
}

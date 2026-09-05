use eucalyptus_cellulose::{Element, task::{Task, TaskExt}};
use futures::{
    StreamExt, channel::mpsc::{self, UnboundedSender}
};
use iced_core::Font;
use iced_futures::Subscription;
use serde::Deserialize;

use crate::{widget::{Widget, WidgetPadding}};

pub struct PowerProfile {
    config: Config,
    tx: Option<UnboundedSender<eucalyptus_root::power_profile::Action>>,
    state: State,
}

enum State {
    Ok {
        active_profile: Option<String>,
        profiles: Option<Vec<String>>,
    },
    Err {
        message: String,
    },
}

impl Widget for PowerProfile {
    type Config = Config;

    type Message = Message;

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>) {
        (Self { config: config.clone(), tx: None, state: State::Ok { active_profile: None, profiles: None } }, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match (&mut self.state, message) {
            (State::Ok { active_profile, .. }, Message::NewActiveProfile(a)) => {
                *active_profile = Some(a);
            }
            (State::Ok { profiles, .. }, Message::NewProfiles(p)) => {
                *profiles = Some(p);
            }
            (State::Ok { active_profile, profiles }, Message::Cycle) => {
                let target_profile = match (active_profile, profiles.as_ref().map(Vec::as_slice)) {
                    (Some(active_profile), Some(profiles)) => match (
                        profiles.iter().position(|x| x == active_profile),
                        self.config.cycle_direction,
                    ) {
                        (Some(position), CycleDirection::Up) if position == profiles.len() => {
                            profiles.first()
                        }
                        (Some(position), CycleDirection::Up) => profiles.get(position + 1),
                        (Some(position), CycleDirection::Down) if position == 0 => profiles.last(),
                        (Some(position), CycleDirection::Down) => profiles.get(position - 1),
                        (None, _) => profiles.first(),
                    },
                    (Some(active_profile), None) => Some(active_profile as &_),
                    (None, Some([profile, ..])) => Some(profile),
                    (None, Some([])) | (None, None) => None,
                };
                let target_profile = target_profile.cloned().unwrap_or("balanced".to_owned()); // Default to balanced
                if let Some(tx) = &self.tx {
                    let _ = tx.unbounded_send(eucalyptus_root::power_profile::Action::SetActiveProfile(target_profile));
                }
            }
            (s, Message::Error(message)) => {
                *s = State::Err { message };
            }
            (_, Message::NewTaskTx(tx)) => {
                self.tx = Some(tx);
            }
            (State::Err { .. }, _) => ()
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match &self.state {
            State::Ok { active_profile: Some(active_profile), .. } => iced_widget::mouse_area(iced_widget::container(match active_profile.as_str() {
                "power-saver" => iced_widget::text("\u{ec1a}").font(Font::with_name("Material Symbols Rounded")),
                "balanced" => iced_widget::text("\u{e9e4}").font(Font::with_name("Material Symbols Rounded")),
                "performance" => iced_widget::text("\u{eb9b}").font(Font::with_name("Material Symbols Rounded")),
                _ => iced_widget::text(active_profile),
            }).widget_padding()).on_press(Message::Cycle).into(),
            State::Ok { active_profile: None, .. } => iced_widget::mouse_area(iced_widget::container(iced_widget::text("?")).widget_padding()).on_press(Message::Cycle).into(),
            State::Err { message } => iced_widget::container(iced_widget::text(message)).widget_padding().into(),
        }
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        match self.state {
            State::Ok { .. } => Subscription::run(|| {
                let (action_tx, action_rx) = mpsc::unbounded();
                futures::stream::once(async { Message::NewTaskTx(action_tx) })
                    .chain(eucalyptus_root::stream(async |tx| eucalyptus_root::power_profile::task(tx, action_rx).await.unwrap()).map(|message| match message {
                            eucalyptus_root::power_profile::Message::NewActiveProfile(active_profile) => Message::NewActiveProfile(active_profile),
                            eucalyptus_root::power_profile::Message::NewProfiles(profiles) => Message::NewProfiles(profiles),
                        }))
            }),
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

#[derive(Clone)]
pub enum Message {
    NewActiveProfile(String),
    NewProfiles(Vec<String>),
    Cycle,
    NewTaskTx(UnboundedSender<eucalyptus_root::power_profile::Action>),
    #[expect(unused)]
    Error(String),
}

use std::collections::HashMap;

use eucalyptus_cellulose::{
    Element,
    task::{Task, TaskExt},
};
use futures::{FutureExt, StreamExt, channel::mpsc};
use iced_core::Font;
use iced_futures::Subscription;
use serde::Deserialize;

use crate::item::Item;

pub struct PowerProfile {
    config: Config,
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

impl Item for PowerProfile {
    type Config = Config;

    type Message = Message;

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>) {
        (
            Self {
                config: config.clone(),
                state: State::Ok {
                    active_profile: None,
                    profiles: None,
                },
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match (&mut self.state, message) {
            (State::Ok { active_profile, .. }, Message::NewActiveProfile(a)) => {
                *active_profile = Some(a);
            }
            (State::Ok { profiles, .. }, Message::NewProfiles(p)) => {
                *profiles = Some(p);
            }
            (State::Err { .. }, _) => (),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let map_profile = |profile, is_active_profile| {
            let profile: Element<_> =
                if let Some(profile_config) = self.config.profiles.get(profile) {
                    iced_widget::row![
                        iced_widget::text(&profile_config.icon)
                            .font(Font::with_name("Material Symbols Rounded")),
                        iced_widget::text(&profile_config.name),
                    ]
                    .into()
                } else {
                    iced_widget::text(profile).into()
                };
            if is_active_profile {
                iced_widget::container(profile)
                    .style(iced_widget::container::primary)
                    .into()
            } else {
                profile
            }
        };
        match &self.state {
            State::Ok {
                active_profile,
                profiles: Some(profiles),
            } => iced_widget::row(
                profiles
                    .into_iter()
                    .map(|profile| map_profile(profile, Some(profile) == active_profile.as_ref())),
            )
            .into(),
            State::Ok {
                active_profile: Some(active_profile),
                profiles: None,
            } => map_profile(active_profile, true),
            State::Ok {
                active_profile: None,
                profiles: None,
            } => iced_widget::text("both active_profile and profile are empty").into(),
            State::Err { message } => iced_widget::text(message).into(),
        }
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        match self.state {
            State::Ok { .. } => Subscription::run(|| {
                let (message_tx, message_rx) = mpsc::unbounded();
                let (_task_tx, task_rx) = mpsc::unbounded();
                let task = eucalyptus_root::power_profile::task(message_tx, task_rx);
                futures::stream_select!(
                    Box::pin(task.into_stream().filter_map(async |_| None)),
                    message_rx,
                )
            }),
            State::Err { .. } => Subscription::none(),
        }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub profiles: HashMap<String, SingleProfileConfig>,
}

#[derive(Clone, Deserialize)]
pub struct SingleProfileConfig {
    pub name: String,
    pub icon: String,
}

pub type Message = eucalyptus_root::power_profile::Message;

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use iced_core::Length;
use iced_runtime::Task;
use tracing_subscriber::{field::MakeExt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    config::Config,
    widget::{Message, WidgetKind, WidgetState},
};

mod application;
mod config;
// mod power_menu;
mod widget;

fn main() {
    let log_directory = if let Some(xdg_state_home) = std::env::var_os("XDG_STATE_HOME") {
        Cow::Owned(PathBuf::from(xdg_state_home).join("eucalyptus-twig"))
    } else if let Some(home) = std::env::home_dir() {
        Cow::Owned(home.join(".local/state/eucalyptus-twig"))
    } else {
        Cow::Borrowed(Path::new("~/.local/state/eucalyptus-twig"))
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env()) // TODO: set logging level in config file
        .with(tracing_subscriber::fmt::layer().map_fmt_fields(|f| f.debug_alt()))
        .with(
            // TODO: consider tracing_appender::non_blocking
            tracing_subscriber::fmt::layer()
                .map_fmt_fields(|f| f.debug_alt())
                .with_ansi(false)
                .with_writer(tracing_appender::rolling::hourly(
                    &log_directory,
                    "eucalyptus-twig.log",
                )),
        )
        .init();
    std::panic::set_hook(Box::new(|panic| {
        if let Some(location) = panic.location() {
            tracing::error!(
                message = %panic,
                panic.file = location.file(),
                panic.line = location.line(),
                panic.column = location.column(),
            );
        } else {
            tracing::error!(message = %panic);
        }
    }));
    tracing::info!(log_directory = %log_directory.display());

    let config = match Config::load() {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to load config, fallback to default");
            Config::default()
        }
    };

    let (state, task) = State::new(config);
    application::Application::new(state, task)
        .unwrap()
        .run()
        .unwrap();
}

struct State {
    left: Box<[Box<[WidgetKind]>]>,
    middle: Box<[Box<[WidgetKind]>]>,
    right: Box<[Box<[WidgetKind]>]>,
    widget_state: WidgetState,
}

impl State {
    fn new(config: Config) -> (Self, Task<Message>) {
        let mut widget_state = WidgetState::default();

        let tasks = [
            config.left.iter(),
            config.middle.iter(),
            config.right.iter(),
        ]
        .into_iter()
        .flatten()
        .flat_map(|group| group.into_iter())
        .filter_map(|widget_kind| widget_state.init_widget(widget_kind, &config))
        .collect::<Vec<_>>();

        (
            Self {
                left: config.left.into_iter().map(Into::into).collect(),
                middle: config.middle.into_iter().map(Into::into).collect(),
                right: config.right.into_iter().map(Into::into).collect(),
                widget_state,
            },
            Task::batch(tasks),
        )
    }
}

impl application::State for State {
    type Message = Message;

    fn update(&mut self, message: Self::Message) -> impl Into<iced_runtime::Task<Self::Message>> {
        self.widget_state.update(message)
    }

    fn view(&self) -> impl Into<application::Element<'_, Self::Message>> {
        let map_widget_kind_group_to_row = |widget_kind_groups: &Box<[Box<[WidgetKind]>]>| {
            iced_widget::row(widget_kind_groups.iter().map(|widgets| {
                iced_widget::container(iced_widget::row(
                    widgets
                        .iter()
                        .filter_map(|widget_kind| self.widget_state.view(widget_kind)),
                ))
                .style(iced_widget::container::primary)
                .into()
            }))
            .spacing(6.0)
        };
        iced_widget::row![
            iced_widget::container(map_widget_kind_group_to_row(&self.left))
                .align_left(Length::Fill),
            map_widget_kind_group_to_row(&self.middle),
            iced_widget::container(map_widget_kind_group_to_row(&self.right))
                .align_right(Length::Fill),
        ]
    }

    fn subscription(&self) -> impl Into<iced_futures::Subscription<Self::Message>> {
        self.widget_state.subscription()
    }
}

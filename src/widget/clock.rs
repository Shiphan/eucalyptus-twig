use std::time::Duration;

use iced_core::{Vector, alignment::Vertical};
use iced_futures::Subscription;
use iced_runtime::{Task, task::sipper};
use iced_widget::canvas::{LineCap, Stroke};
use serde::Deserialize;
use time::{
    OffsetDateTime,
    Time,
    format_description::{self, OwnedFormatItem},
};

use crate::{application::Element, widget::Widget};

// TODO: maybe we should use icu4x for localized formatting?

pub enum Clock {
    Ok {
        format_description: OwnedFormatItem,
        formatted_time: String,
        now: OffsetDateTime,
    },
    Err {
        message: String,
    },
}

impl Widget for Clock {
    type Config = Config;

    type Message = ();

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>) {
        let format_description = match format_description::parse_owned::<2>(&config.format) {
            Ok(x) => x,
            Err(e) => {
                return (
                    Self::Err {
                        message: format!("Error while parsing time format description: {e}"),
                    },
                    Task::none(),
                );
            }
        };

        let now = match OffsetDateTime::now_local() {
            Ok(x) => x,
            Err(e) => {
                return (
                    Self::Err {
                        message: format!("Error while getting local time: {e}"),
                    },
                    Task::none(),
                );
            }
        };

        let formatted_time = match now.format(&format_description) {
            Ok(x) => x,
            Err(e) => {
                return (
                    Self::Err {
                        message: format!("Error while formatting time `{now}`: {e}"),
                    },
                    Task::none(),
                );
            }
        };

        (
            Self::Ok {
                format_description,
                formatted_time,
                now,
            },
            Task::none(),
        )
    }

    fn update(&mut self, (): Self::Message) -> impl Into<Task<Self::Message>> {
        let Self::Ok {
            format_description,
            formatted_time,
            now,
        } = self
        else {
            return;
        };

        *now = match OffsetDateTime::now_local() {
            Ok(x) => x,
            Err(e) => {
                *self = Self::Err {
                    message: format!("Error while getting local time: {e}"),
                };
                return;
            }
        };

        *formatted_time = match now.format(&format_description) {
            Ok(x) => x,
            Err(e) => {
                *self = Self::Err {
                    message: format!("Error while formatting time `{now}`: {e}"),
                };
                return;
            }
        };
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self {
            Self::Ok {
                formatted_time,
                now,
                ..
            } => iced_widget::container(
                iced_widget::row![
                    iced_widget::canvas(AnalogClock {
                        hour_hand_radians: (90.0
                            - now.hour() as f32 * 30.0
                            - now.minute() as f32 * 0.5)
                            .to_radians(),
                        minute_hand_radians: (90.0 - now.minute() as f32 * 6.0).to_radians(),
                    })
                    .width(16)
                    .height(16),
                    iced_widget::text(formatted_time),
                ]
                .align_y(Vertical::Center),
            )
            .into(),
            Self::Err { message } => iced_widget::text(message).into(),
        }
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        match self {
            Self::Ok { .. } => Subscription::run(|| {
                sipper(async |mut tx| {
                    loop {
                        let now = OffsetDateTime::now_local().unwrap();
                        let next = Time::from_hms(now.time().hour(), now.time().minute(), 0)
                            .unwrap()
                            + Duration::from_mins(1);
                        tokio::time::sleep(now.time().duration_until(next).unsigned_abs()).await;
                        tx.send(()).await;
                    }
                })
            }),
            Self::Err { .. } => Subscription::none(),
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_format_string")]
    format: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            format: default_format_string(),
        }
    }
}

fn default_format_string() -> String {
    "[month padding:none repr:numerical]/[day padding:none] [weekday repr:short] [hour padding:none repr:12]:[minute padding:zero] [period case:upper]".to_owned()
}

struct AnalogClock {
    hour_hand_radians: f32,
    minute_hand_radians: f32,
}

impl<Message> iced_widget::canvas::Program<Message> for AnalogClock {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced_renderer::Renderer,
        theme: &iced_widget::renderer::core::Theme,
        bounds: iced_core::Rectangle,
        _cursor: iced_core::mouse::Cursor,
    ) -> Vec<iced_widget::canvas::Geometry> {
        use iced_widget::canvas;

        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let max_radius = frame.width().min(frame.height()) / 2.0;

        let background = canvas::Path::circle(frame.center(), max_radius * 0.8);
        frame.fill(&background, theme.palette().text);

        let stroke = Stroke::default()
            .with_width(2.0)
            .with_color(theme.palette().primary)
            .with_line_cap(LineCap::Round);

        let hour_hand_length = max_radius * 0.3;
        let hour_hand = canvas::Path::line(
            frame.center(),
            frame.center()
                + Vector::new(self.hour_hand_radians.cos(), -self.hour_hand_radians.sin())
                    * hour_hand_length,
        );
        frame.stroke(&hour_hand, stroke);

        let minute_hand_length = max_radius * 0.5;
        let minute_hand = canvas::Path::line(
            frame.center(),
            frame.center()
                + Vector::new(
                    self.minute_hand_radians.cos(),
                    -self.minute_hand_radians.sin(),
                ) * minute_hand_length,
        );
        frame.stroke(&minute_hand, stroke);

        vec![frame.into_geometry()]
    }
}

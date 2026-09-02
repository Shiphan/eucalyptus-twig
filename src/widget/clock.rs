use iced_core::{Vector, alignment::Vertical};
use iced_futures::Subscription;
use iced_runtime::{Task, task::sipper};
use iced_widget::canvas::{LineCap, Stroke};
use jiff::{Zoned, ZonedRound};
use serde::Deserialize;

use crate::{application::Element, widget::Widget};

// TODO: maybe we should use icu4x for localized formatting?

pub struct Clock {
    format: String,
    formatted_time: String,
    now: Zoned,
}

impl Widget for Clock {
    type Config = Config;

    type Message = ();

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>) {
        let now = Zoned::now();

        (
            Self {
                format: config.format.clone(),
                formatted_time: now.strftime(&config.format).to_string(),
                now,
            },
            Task::none(),
        )
    }

    fn update(&mut self, (): Self::Message) -> impl Into<Task<Self::Message>> {
        self.now = Zoned::now();
        self.formatted_time = self.now.strftime(&self.format).to_string();
    }

    fn view(&self) -> Element<'_, Self::Message> {
        iced_widget::container(
            iced_widget::row![
                iced_widget::canvas(AnalogClock {
                    hour_hand_radians: (90.0
                        - self.now.hour() as f32 * 30.0
                        - self.now.minute() as f32 * 0.5)
                        .to_radians(),
                    minute_hand_radians: (90.0 - self.now.minute() as f32 * 6.0).to_radians(),
                })
                .width(16)
                .height(16),
                iced_widget::text(&self.formatted_time),
            ]
            .align_y(Vertical::Center)
            .spacing(4)
        ).into()
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        Subscription::run(|| {
            sipper(async |mut tx| {
                loop {
                    let now = Zoned::now();
                    let next = now.round(ZonedRound::new().smallest(jiff::Unit::Minute).mode(jiff::RoundMode::Trunc)).unwrap_or_else(|_| now.clone()) + jiff::SignedDuration::from_mins(1);
                    tokio::time::sleep(now.duration_until(&next).try_into().unwrap_or_default()).await;
                    tx.send(()).await;
                }
            })
        })
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
    "%-m/%-d %a %-I:%M %p".to_owned()
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
        frame.fill(&background, theme.extended_palette().background.base.text);

        let stroke = Stroke::default()
            .with_width(2.0)
            .with_color(theme.extended_palette().background.base.color)
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

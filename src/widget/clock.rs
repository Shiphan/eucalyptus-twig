use std::time::Duration;

use gpui::{
    Context, Div, IntoElement, ParentElement, PathBuilder, PathStyle, Render, StrokeOptions,
    Styled, Window, black, canvas, div, point, px, rems, white,
};
use lyon::path::LineCap;
use serde::Deserialize;
use time::{
    OffsetDateTime, Time,
    error::InvalidFormatDescription,
    format_description::{self, OwnedFormatItem},
};

use crate::widget::{Widget, widget_wrapper};

pub struct Clock {
    format_description: Result<OwnedFormatItem, InvalidFormatDescription>,
}

impl Widget for Clock {
    type Config = ClockConfig;

    fn new(cx: &mut Context<Self>, config: &Self::Config) -> Self {
        let format_description = format_description::parse_owned::<2>(&config.format);
        if format_description.is_ok() {
            cx.spawn(async move |this, cx| {
                loop {
                    let _ = this.update(cx, |_, cx| cx.notify());
                    let now = OffsetDateTime::now_local().unwrap();
                    let next = Time::from_hms(now.time().hour(), now.time().minute(), 0).unwrap()
                        + Duration::from_mins(1);
                    cx.background_executor()
                        .timer(now.time().duration_until(next).unsigned_abs())
                        .await;
                }
            })
            .detach();
        }

        Self { format_description }
    }
}

impl Render for Clock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let format_description = match &self.format_description {
            Ok(x) => x,
            Err(e) => {
                return widget_wrapper()
                    .child(format!("Error while parsing time format description: {e}"));
            }
        };
        match current_time(format_description) {
            Ok((clock, formatted_time)) => widget_wrapper()
                .flex()
                .items_center()
                .gap(rems(0.25))
                .child(clock)
                .child(formatted_time),
            Err(e) => widget_wrapper().child(e),
        }
    }
}

#[derive(Deserialize)]
pub struct ClockConfig {
    #[serde(default = "default_format_string")]
    format: String,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: default_format_string(),
        }
    }
}

fn default_format_string() -> String {
    "[month padding:none repr:numerical]/[day padding:none] [weekday repr:short] [hour padding:none repr:12]:[minute padding:zero] [period case:upper]".to_owned()
}

// TODO: maybe we should use icu4x for localized formatting?
fn current_time(format_description: &OwnedFormatItem) -> Result<(Div, String), String> {
    let time =
        OffsetDateTime::now_local().map_err(|e| format!("Error while getting local time: {e}"))?;
    let clock = div().relative().size_4().rounded_full().bg(white()).child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let mut path = PathBuilder::default().with_style(PathStyle::Stroke(
                    StrokeOptions::default()
                        .with_start_cap(LineCap::Round)
                        .with_end_cap(LineCap::Round)
                        .with_line_width(2.0),
                ));
                path.move_to(point(px(0.0), px(0.0)));
                path.line_to(point(px(0.0), px(-4.4)));
                path.rotate(time.time().minute() as f32 * 6.0);
                path.translate(bounds.center());
                match path.build() {
                    Ok(path) => window.paint_path(path, black()),
                    Err(e) => tracing::error!(error = %e, "Failed to build path for minute hand"),
                }

                let mut path = PathBuilder::default().with_style(PathStyle::Stroke(
                    StrokeOptions::default()
                        .with_start_cap(LineCap::Round)
                        .with_end_cap(LineCap::Round)
                        .with_line_width(2.0),
                ));
                path.move_to(point(px(0.0), px(0.0)));
                path.line_to(point(px(0.0), px(-2.6)));
                path.rotate(time.time().hour() as f32 * 30.0 + time.time().minute() as f32 * 0.5);
                path.translate(bounds.center());
                match path.build() {
                    Ok(path) => window.paint_path(path, black()),
                    Err(e) => tracing::error!(error = %e, "Failed to build path for hour hand"),
                }
            },
        )
        .size_full(),
    );
    let formatted_time = time
        .format(format_description)
        .map_err(|e| format!("Error while formatting time `{time}`: {e}"))?;

    Ok((clock, formatted_time))
}

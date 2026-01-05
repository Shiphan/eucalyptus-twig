use std::{ops::Deref, pin::Pin, task::Poll, time::Duration};

use gpui::{
    AnyView, App, Application, Bounds, Context, Entity, Pixels, PlatformDisplay, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div,
    layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
    point,
    prelude::*,
    px, rems,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::widget::Widget;

mod power_menu;
mod widget;

const WIDTH: f32 = 1440.0;
const HEIGHT: f32 = 40.0;

fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::filter::Targets::new()
                .with_default(tracing::Level::WARN)
                .with_target(env!("CARGO_CRATE_NAME"), tracing::Level::INFO),
        )
        .init();

    Application::new().run(|cx: &mut App| {
        gpui_tokio::init(cx);

        cx.spawn(async |cx| {
            // TODO: by default, gpui will not wait for wayland to tell us displays information
            // wait 10 poll for wayland to tell us all screens
            PollCounter::new(10).await;
            // or wait a bit for wayland to tell us all screens
            cx.background_executor()
                // .timer(Duration::from_nanos(1000))
                .timer(Duration::from_millis(1))
                .await;

            cx.update(|cx| {
                let displays = cx.displays();

                println!("displays: {:#?}", displays);

                if displays.len() == 0 {
                    println!("[WARN] there is no display in the context!!!");
                }

                for display in displays {
                    cx.open_window(Bar::window_options(Some(display)), Bar::build_root_view)
                        .unwrap();
                }
            })
            .unwrap();
        })
        .detach();
    });
}

struct Bar {
    left: Vec<AnyView>,
    center: Vec<AnyView>,
    right: Vec<AnyView>,
}

impl Bar {
    pub fn build_root_view(_window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            left: vec![
                cx.new(widget::PowerMenu::new).into(),
                cx.new(widget::Clock::new).into(),
                cx.new(widget::Display::new).into(),
            ],
            center: vec![cx.new(widget::HyprlandWorkspace::new).into()],
            right: vec![
                cx.new(widget::Volume::new).into(),
                cx.new(widget::Bluetooth::new).into(),
                cx.new(widget::Quit::new).into(),
            ],
        })
    }
    pub fn window_options(
        display: Option<impl Deref<Target = impl PlatformDisplay + ?Sized>>,
    ) -> WindowOptions {
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(
                // TODO: I want the window height to fit the content, and the width based on screen width
                if let Some(display) = display.as_ref()
                    && false
                {
                    let mut bounds = display.bounds();
                    bounds.size.height = px(HEIGHT);
                    bounds
                } else {
                    Bounds {
                        origin: point(px(0.0), px(0.0)),
                        size: Size::new(px(WIDTH), px(HEIGHT)),
                    }
                },
            )),
            titlebar: None,
            kind: WindowKind::LayerShell(LayerShellOptions {
                namespace: "eucalyptus-twig".to_owned(),
                layer: Layer::Top,
                anchor: Anchor::TOP,
                // TODO: this height should also based on the content
                exclusive_zone: Some(Pixels::from(HEIGHT)),
                exclusive_edge: Some(Anchor::TOP),
                keyboard_interactivity: KeyboardInteractivity::None,
                ..Default::default()
            }),
            display_id: display.as_ref().map(|x| x.id()),
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        }
    }
}

impl Render for Bar {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_between()
            // .text_size(rems(1.2))
            // .font_weight(FontWeight::EXTRA_BOLD)
            // .text_color(white())
            // .bg(rgba(0x0000044))
            .rounded_xl()
            .p_1()
            .child(
                div()
                    .flex_grow()
                    .flex_basis(px(0.0))
                    .flex()
                    .justify_start()
                    .gap(rems(0.25))
                    .children(self.left.clone()),
            )
            .child(div().flex().gap(rems(0.25)).children(self.center.clone()))
            .child(
                div()
                    .flex_grow()
                    .flex_basis(px(0.0))
                    .flex()
                    .justify_end()
                    .gap(rems(0.25))
                    .children(self.right.clone()),
            )
    }
}

struct PollCounter {
    count: u32,
    max: u32,
}

impl PollCounter {
    pub fn new(max: u32) -> Self {
        Self { count: 0, max }
    }
}

impl Future for PollCounter {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        if self.count >= self.max {
            Poll::Ready(())
        } else {
            self.count += 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

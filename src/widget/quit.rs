use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Window,
};

use crate::widget::{Widget, widget_wrapper};

pub struct Quit;

impl Widget for Quit {
    type Config = ();

    fn new(_cx: &mut Context<Self>, _config: &Self::Config) -> Self {
        Self
    }
}

impl Render for Quit {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        widget_wrapper()
            .id("quit-button")
            .on_click(|_click_event, _window, cx| {
                cx.quit();
            })
            .child("Quit")
    }
}

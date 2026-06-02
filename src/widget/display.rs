use gpui::{Context, IntoElement, ParentElement, Render, Window, div};

use crate::widget::Widget;

pub struct Display;

impl Widget for Display {
    type Config = ();

    fn new(_cx: &mut Context<Self>, _config: &Self::Config) -> Self {
        Self
    }
}

impl Render for Display {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let display = match window.display(cx) {
            Some(display) => format!("display = {:?}", display.id()),
            None => "display not found".to_owned(),
        };
        div().child(display)
    }
}

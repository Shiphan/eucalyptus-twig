use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, rgb,
};

use crate::widget::{Widget, widget_wrapper};

pub struct PowerMenu;

impl Widget for PowerMenu {
    type Config = ();

    fn new(_cx: &mut Context<Self>, _config: &Self::Config) -> Self {
        Self
    }
}

impl Render for PowerMenu {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        widget_wrapper()
            .id("button_left")
            .on_click(|_click_event, window, cx| {
                cx.open_window(
                    crate::power_menu::PowerMenu::window_options(window.display(cx)),
                    crate::power_menu::PowerMenu::build_root_view,
                )
                .unwrap();
            })
            .text_color(rgb(0x7ebae4))
            .font_family("NotoSans Nerd Font Propo")
            .child("")
    }
}

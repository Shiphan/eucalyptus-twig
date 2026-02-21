use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, Window, rgb,
};
use serde::Deserialize;

use crate::widget::{Widget, widget_wrapper};

pub struct PowerMenu {
    config: PowerMenuConfig,
}

impl Widget for PowerMenu {
    type Config = PowerMenuConfig;

    fn new(_cx: &mut Context<Self>, config: &Self::Config) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

impl Render for PowerMenu {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let config = self.config.clone();
        widget_wrapper()
            .id("button_left")
            .on_click(move |_click_event, window, cx| {
                let config = config.clone();
                cx.open_window(
                    crate::power_menu::PowerMenu::window_options(window.display(cx)),
                    move |window, cx| {
                        crate::power_menu::PowerMenu::build_root_view(window, cx, config)
                    },
                )
                .unwrap();
            })
            .text_color(rgb(0x7ebae4))
            .font_family("NotoSans Nerd Font Propo")
            .child("\u{f313}")
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PowerMenuConfig {
    pub lock_command: Vec<String>,
    pub suspend_command: Vec<String>,
    pub hibernate_command: Vec<String>,
    pub reboot_command: Vec<String>,
    pub shutdown_command: Vec<String>,
}

use gpui::{Context, Div, Render, Styled, black, div, white};

pub use bluetooth::Bluetooth;
pub use clock::Clock;
pub use display::Display;
pub use hyprland::workspaces::HyprlandWorkspace;
pub use power_menu::PowerMenu;
pub use quit::Quit;

pub mod bluetooth;
pub mod clock;
pub mod display;
pub mod hyprland;
pub mod power_menu;
pub mod quit;

pub fn widget_wrapper() -> Div {
    div()
        .text_color(white())
        .bg(black())
        .rounded_lg()
        .px_2()
        .py_0p5()
}

pub trait Widget: Render {
    fn new(cx: &mut Context<Self>) -> Self;
}

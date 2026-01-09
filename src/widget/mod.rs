use gpui::{AnyView, AppContext, Context, Div, Render, Styled, black, div, white};
use serde::Deserialize;

pub use bluetooth::Bluetooth;
pub use clock::Clock;
pub use display::Display;
pub use hyprland::workspaces::HyprlandWorkspace;
pub use power::Power;
pub use power_menu::PowerMenu;
pub use power_profile::PowerProfile;
pub use quit::Quit;
pub use volume::Volume;
pub use workspaces::Workspaces;

pub mod bluetooth;
pub mod clock;
pub mod display;
pub mod hyprland;
pub mod power;
pub mod power_menu;
pub mod power_profile;
pub mod quit;
pub mod volume;
pub mod workspaces;

// TODO: unify widget naming, like Workspaces or Workspace

#[derive(Deserialize)]
pub enum WidgetOption {
    Bluetooth,
    Clock,
    Display,
    HyprlandWorkspace,
    Power,
    PowerMenu,
    PowerProfile,
    Quit,
    Volume,
    Workspaces,
}

impl WidgetOption {
    pub fn build(&self, cx: &mut impl AppContext) -> AnyView {
        match self {
            Self::Bluetooth => cx.new(Bluetooth::new).into(),
            Self::Clock => cx.new(Clock::new).into(),
            Self::Display => cx.new(Display::new).into(),
            Self::HyprlandWorkspace => cx.new(HyprlandWorkspace::new).into(),
            Self::Power => cx.new(Power::new).into(),
            Self::PowerMenu => cx.new(PowerMenu::new).into(),
            Self::PowerProfile => cx.new(PowerProfile::new).into(),
            Self::Quit => cx.new(Quit::new).into(),
            Self::Volume => cx.new(Volume::new).into(),
            Self::Workspaces => cx.new(Workspaces::new).into(),
        }
    }
}

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

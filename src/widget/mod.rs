use gpui::{AnyView, AppContext, Context, Div, Render, Styled, black, div, white};
use serde::{Deserialize, de::DeserializeOwned};

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

use crate::config::Config;

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
    pub fn build(&self, cx: &mut impl AppContext, config: &Config) -> AnyView {
        match self {
            Self::Bluetooth => cx.new(|cx| Bluetooth::new(cx, &())).into(),
            Self::Clock => cx.new(|cx| Clock::new(cx, &config.widget.clock)).into(),
            Self::Display => cx.new(|cx| Display::new(cx, &())).into(),
            Self::HyprlandWorkspace => cx.new(|cx| HyprlandWorkspace::new(cx, &())).into(),
            Self::Power => cx.new(|cx| Power::new(cx, &())).into(),
            Self::PowerMenu => cx.new(|cx| PowerMenu::new(cx, &())).into(),
            Self::PowerProfile => cx.new(|cx| PowerProfile::new(cx, &())).into(),
            Self::Quit => cx.new(|cx| Quit::new(cx, &())).into(),
            Self::Volume => cx.new(|cx| Volume::new(cx, &())).into(),
            Self::Workspaces => cx.new(|cx| Workspaces::new(cx, &())).into(),
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
    type Config: Default + DeserializeOwned;

    fn new(cx: &mut Context<Self>, config: &Self::Config) -> Self;
}

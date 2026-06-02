use std::ffi::OsStr;

use gpui::{
    AnyView,
    App,
    AppContext,
    Context,
    Div,
    IntoElement,
    ParentElement,
    Render,
    Styled,
    black,
    div,
    white,
};
use serde::de::DeserializeOwned;
use smol::process::Command;

pub use bluetooth::Bluetooth;
pub use clock::Clock;
pub use display::Display;
pub use hyprland::workspaces::HyprlandWorkspace;
pub use network::Network;
pub use power::Power;
pub use power_menu::PowerMenu;
pub use power_profile::PowerProfile;
pub use quit::Quit;
pub use system_information::SystemInformation;
pub use volume::Volume;
pub use workspaces::Workspaces;

use crate::config::{Config, WidgetOption, WidgetOptionGroup};

pub mod bluetooth;
pub mod clock;
pub mod display;
pub mod hyprland;
pub mod network;
pub mod power;
pub mod power_menu;
pub mod power_profile;
pub mod quit;
pub mod system_information;
pub mod volume;
pub mod workspaces;

// TODO: unify widget naming, like Workspaces or Workspace

impl WidgetOption {
    pub fn build(&self, cx: &mut impl AppContext, config: &Config) -> AnyView {
        match self {
            Self::Bluetooth => cx
                .new(|cx| Bluetooth::new(cx, &config.widget.bluetooth))
                .into(),
            Self::Clock => cx.new(|cx| Clock::new(cx, &config.widget.clock)).into(),
            Self::Display => cx.new(|cx| Display::new(cx, &())).into(),
            Self::HyprlandWorkspace => cx.new(|cx| HyprlandWorkspace::new(cx, &())).into(),
            Self::Network => cx.new(|cx| Network::new(cx, &config.widget.network)).into(),
            Self::Power => cx.new(|cx| Power::new(cx, &())).into(),
            Self::PowerMenu => cx
                .new(|cx| PowerMenu::new(cx, &config.widget.power_menu))
                .into(),
            Self::PowerProfile => cx
                .new(|cx| PowerProfile::new(cx, &config.widget.power_profile))
                .into(),
            Self::Quit => cx.new(|cx| Quit::new(cx, &())).into(),
            Self::SystemInformation => cx
                .new(|cx| SystemInformation::new(cx, &config.widget.system_information))
                .into(),
            Self::Volume => cx.new(|cx| Volume::new(cx, &config.widget.volume)).into(),
            Self::Workspaces => cx.new(|cx| Workspaces::new(cx, &())).into(),
        }
    }
}

impl WidgetOptionGroup {
    pub fn build(&self, cx: &mut impl AppContext, config: &Config) -> WidgetViewGroup {
        match self {
            Self::One(widget_option) => WidgetViewGroup::One(widget_option.build(cx, config)),
            Self::Array(widget_options) => {
                WidgetViewGroup::Array(widget_options.iter().map(|x| x.build(cx, config)).collect())
            }
        }
    }
}

#[derive(Clone)]
pub enum WidgetViewGroup {
    One(AnyView),
    Array(Box<[AnyView]>),
}

impl IntoElement for WidgetViewGroup {
    type Element = Div;

    fn into_element(self) -> Self::Element {
        let widget_wrapper = div()
            .text_color(white())
            .bg(black())
            .rounded_lg()
            .px_2()
            .py_0p5();

        match self {
            Self::One(widget) => widget_wrapper.child(widget),
            Self::Array(widgets) => widget_wrapper
                .flex()
                .flex_row()
                .gap_x_0p5()
                .children(widgets),
        }
    }
}

pub trait Widget: Render {
    type Config: Default + DeserializeOwned;

    fn new(cx: &mut Context<Self>, config: &Self::Config) -> Self;
}

fn spawn_detached_command<S>(cx: &mut App, command: &[S], option_name: &'static str)
where
    S: AsRef<OsStr>,
{
    let [program, args @ ..] = command else {
        tracing::warn!("{} is an empty array, no command is executed.", option_name);
        return;
    };

    match Command::new(program).args(args).spawn() {
        Ok(mut child) => {
            cx.spawn(async move |_| match child.status().await {
                Ok(status) if status.success() => {
                    tracing::info!("Child process successly exit");
                }
                Ok(status) => {
                    tracing::warn!("Child process exit with status: {status}");
                }
                Err(e) => {
                    tracing::error!("Failed to get child process statue: {e}");
                }
            })
            .detach();
        }
        Err(e) => {
            tracing::error!("Failed to spawn command: {e}");
        }
    }
}

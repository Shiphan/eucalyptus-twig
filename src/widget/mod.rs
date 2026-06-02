use std::ffi::OsStr;

use gpui::{AnyView, App, AppContext, Context, Div, Render, Styled, black, div, white};
use serde::{Deserialize, de::DeserializeOwned};
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

use crate::config::Config;

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

#[derive(Deserialize)]
pub enum WidgetOption {
    Bluetooth,
    Clock,
    Display,
    HyprlandWorkspace,
    Network,
    Power,
    PowerMenu,
    PowerProfile,
    Quit,
    SystemInformation,
    Volume,
    Workspaces,
}

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

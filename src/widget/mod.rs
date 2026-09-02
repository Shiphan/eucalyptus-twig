use std::ffi::OsStr;

use iced_futures::Subscription;
use iced_runtime::Task;
use serde::Deserialize;
use tokio::process::Command;

use crate::{application::Element, config::Config};

macro_rules! generate {
    ($( $mod_head:ident $( ::$mod_tail:ident )* : $type_ident:ident => $var_ident:ident ),* $( , )?) => {
        $( pub mod $mod_head; )*

        #[derive(Deserialize)]
        pub enum WidgetKind {
            $( $type_ident, )*
        }

        #[derive(Default)]
        pub struct WidgetState {
            $( pub $var_ident: Option<self::$mod_head $( ::$mod_tail )* ::$type_ident>, )*
        }

        impl WidgetState {
            pub fn init_widget(&mut self, widget_kind: &WidgetKind, config: &Config) -> Option<Task<Message>> {
                let mut task = None;
                match widget_kind {
                    $(
                        WidgetKind::$type_ident => {
                            self.$var_ident.get_or_insert_with(|| {
                                let (state, t) = self::$mod_head $( ::$mod_tail )* ::$type_ident::new(&config.widget.$var_ident);
                                task = Some(t.map(Message::$type_ident));
                                state
                            });
                        }
                    )*
                }
                task
            }

            pub fn update(&mut self, message: Message) -> Task<Message> {
                match message {
                    $( Message::$type_ident(message) if let Some(widget) = &mut self.$var_ident => widget.update(message).into().map(Message::$type_ident), )*
                    _ => Task::none(),
                }
            }

            pub fn view(&self, widget_kind: &WidgetKind) -> Option<Element<'_, Message>> {
                match widget_kind {
                    $( WidgetKind::$type_ident if let Some(widget) = &self.$var_ident => Some(widget.view().map(Message::$type_ident)), )*
                    _ => None,
                }
            }

            pub fn subscription(&self) -> Subscription<Message> {
                Subscription::batch(
                    [
                        $( self.$var_ident.as_ref().map(|widget| widget.subscription().into().map(Message::$type_ident)), )*
                    ]
                    .into_iter()
                    .flatten(),
                )
            }
        }

        // #[derive(Clone)]
        pub enum Message {
            $( $type_ident(<self::$mod_head $( ::$mod_tail )* ::$type_ident as Widget>::Message), )*
        }

        #[derive(Deserialize, Default)]
        #[serde(default, deny_unknown_fields)]
        pub struct WidgetConfig {
            $( pub $var_ident: <self::$mod_head $( ::$mod_tail )* ::$type_ident as Widget>::Config, )*
        }
    };
}

// TODO: unify widget naming, like Workspaces or Workspace
generate!(
    bluetooth: Bluetooth => bluetooth,
    clock: Clock => clock,
    hyprland::workspaces: HyprlandWorkspace => hyprland_workspace,
    network: Network => network,
    power: Power => power,
    // power_menu::PowerMenu => power_menu,
    power_profile: PowerProfile => power_profile,
    quit: Quit => quit,
    system_information: SystemInformation => system_information,
    volume: Volume => volume,
    workspaces: Workspaces => workspaces,
);

pub trait Widget: Sized {
    type Config;
    type Message;

    fn new(config: &Self::Config) -> (Self, Task<Self::Message>);

    fn update(&mut self, message: Self::Message) -> impl Into<iced_runtime::Task<Self::Message>>;

    fn view(&self) -> Element<'_, Self::Message>;

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        Subscription::none()
    }
}

const WIDGET_PADDING: iced_core::Padding = iced_core::Padding::new(2.0);

trait WidgetPadding {
    fn widget_padding(self) -> Self;
}

impl<Message> WidgetPadding for iced_widget::Container<'_, Message> {
    fn widget_padding(self) -> Self {
        self.padding(WIDGET_PADDING)
    }
}

impl<Message> WidgetPadding for iced_widget::Row<'_, Message> {
    fn widget_padding(self) -> Self {
        self.padding(WIDGET_PADDING)
    }
}

fn spawn_detached_command<S>(command: &[S], option_name: &'static str) -> Task<()>
where
    S: AsRef<OsStr>,
{
    let [program, args @ ..] = command else {
        tracing::warn!("{} is an empty array, no command is executed.", option_name);
        return Task::none();
    };

    let mut command = Command::new(program);
    command.args(args);
    Task::future(async move {
        match command.spawn() {
            Ok(mut child) => match child.wait().await {
                Ok(status) if status.success() => {
                    tracing::info!("Child process successly exit");
                }
                Ok(status) => {
                    tracing::warn!("Child process exit with status: {status}");
                }
                Err(e) => {
                    tracing::error!("Failed to get child process statue: {e}");
                }
            },
            Err(e) => {
                tracing::error!("Failed to spawn command: {e}");
            }
        }
    })
}

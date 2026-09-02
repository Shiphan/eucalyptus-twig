use std::{
    collections::BTreeMap,
    env,
    fmt::Display,
    path::Path,
};

use iced_futures::Subscription;
use iced_runtime::Task;
use serde::Deserialize;
use tokio::{io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}, net::UnixStream};

use crate::{
    application::Element,
    widget::{Widget, WidgetPadding},
};

pub struct HyprlandWorkspace {
    workspaces: BTreeMap<i64, WorkspaceInfo>,
    active_workspace: Option<i64>,
    active_special_workspace: Option<i64>,
    error_message: Option<String>,
}

impl Widget for HyprlandWorkspace {
    type Config = ();

    type Message = Message;

    fn new((): &Self::Config) -> (Self, Task<Self::Message>) {
        (
            Self {
                workspaces: BTreeMap::new(),
                active_workspace: None,
                active_special_workspace: None,
                error_message: None,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match message {
            Message::NewWorkspace { id, info } => {
                if let Some(old) = self.workspaces.insert(id, info) {
                    tracing::warn!("Received a `createworkspacev2` with id = {id} , but there is already an old workspace with name = {}", old.name);
                }
            }
            Message::RemoveWorkspace { id, name } => match self.workspaces.remove(&id) {
                Some(old) if old.name != name => {
                    tracing::warn!("Received a `destroyworkspacev2` with id = {id} and name = {name}, but the old name is not the same: `{}`", old.name);
                }
                None => {
                    tracing::error!("Received a `destroyworkspacev2` with id = {id}, but there is no workspace with same id");
                }
                _ => (),
            },
            Message::NewActiveWorkspace(id) => {
                self.active_workspace = id;
            }
            Message::NewActiveSpecialWorkspace(id) => {
                self.active_special_workspace = id;
            }
            Message::SetWorkspapces(workspaces) => {
                self.workspaces = workspaces;
            }
            Message::Error(message) => {
                self.error_message = Some(message);
            }
            Message::ClearError => {
                self.error_message = None;
            }
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self {
            Self {
                workspaces,
                active_workspace,
                active_special_workspace,
                error_message: None,
            } => iced_widget::row(workspaces.iter().map(|(&id, info)| {
                if Some(id) == *active_workspace || Some(id) == *active_special_workspace {
                    iced_widget::text!(" > {} < ", info.name)
                        .style(iced_widget::text::secondary)
                        .into()
                } else {
                    iced_widget::text(&info.name).into()
                }
            }))
            .widget_padding()
            .into(),
            Self {
                error_message: Some(message),
                ..
            } => iced_widget::container(iced_widget::text(message))
                .widget_padding()
                .into(),
        }
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        Subscription::run(|| iced_runtime::task::sipper(task))
    }
}

#[allow(private_interfaces)]
pub enum Message {
    NewWorkspace { id: i64, info: WorkspaceInfo },
    RemoveWorkspace { id: i64, name: String },
    NewActiveWorkspace(Option<i64>),
    NewActiveSpecialWorkspace(Option<i64>),
    SetWorkspapces(BTreeMap<i64, WorkspaceInfo>),
    Error(String),
    ClearError,
}

async fn task(mut tx: iced_runtime::task::Sender<Message>) {
    let hyprland_instance_signature = match env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        Ok(x) => x,
        Err(e) => {
            tx.send(Message::Error(format!(
                "error while getting HYPRLAND_INSTANCE_SIGNATURE: {e}"
            )))
            .await;
            return;
        }
    };
    let runtime_dir = match env::var("XDG_RUNTIME_DIR") {
        Ok(xdg_runtime_dir) => format!("{xdg_runtime_dir}/hypr"),
        Err(e) => {
            // TODO: use the fallback format!("/run/user/{uid}/hypr"):
            // <https://github.com/hyprwm/Hyprland/blob/main/hyprctl/src/main.cpp>
            tx.send(Message::Error(format!(
                "error while getting XDG_RUNTIME_DIR: {e}"
            )))
            .await;
            return;
        }
    };

    let event_socket_path = format!("{runtime_dir}/{hyprland_instance_signature}/.socket2.sock");
    let command_socket_path = format!("{runtime_dir}/{hyprland_instance_signature}/.socket.sock");

    try_update_with_get_workspace(&command_socket_path, &mut tx).await;

    let mut event_stream = match UnixStream::connect(&event_socket_path).await {
        Ok(x) => BufReader::new(x),
        Err(e) => {
            tx.send(Message::Error(format!(
                "error while connecting to hyprland socket ({event_socket_path}): {e}"
            )))
            .await;
            return;
        }
    };

    loop {
        let mut line = String::new();
        match event_stream.read_line(&mut line).await {
            Ok(_) => {
                tx.send(Message::ClearError).await;
            }
            Err(e) => {
                tx.send(Message::Error(format!(
                    "error while reading the socket: {e}"
                )))
                .await;
                break;
            }
        };
        let line = line.strip_suffix('\n').unwrap_or(line.as_str());

        if let Some(line) = line.strip_prefix("createworkspacev2>>") {
            if let Some((id, name)) = line.split_once(",") {
                match id.parse() {
                    Ok(id) => {
                        tx.send(Message::NewWorkspace {
                            id,
                            info: WorkspaceInfo {
                                name: name.to_owned(),
                            },
                        })
                        .await;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to parse the id ({id}) from `createworkspacev2`: {e}"
                        );
                        try_update_with_get_workspace(&command_socket_path, &mut tx).await;
                    }
                }
            } else {
                tracing::error!(
                    "Received a `createworkspacev2` update `{line}`, but it doesn't contain any `,`"
                );
                try_update_with_get_workspace(&command_socket_path, &mut tx).await;
            }
        } else if let Some(line) = line.strip_prefix("destroyworkspacev2>>") {
            if let Some((id, name)) = line.split_once(",") {
                match id.parse() {
                    Ok(id) => {
                        tx.send(Message::RemoveWorkspace {
                            id,
                            name: name.to_owned(),
                        })
                        .await;
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to parse the id ({id}) from `destroyworkspacev2`: {e}"
                        );
                        try_update_with_get_workspace(&command_socket_path, &mut tx).await;
                    }
                }
            } else {
                tracing::error!(
                    "Received a `destroyworkspacev2` update `{line}`, but it doesn't contain any `,`"
                );
                try_update_with_get_workspace(&command_socket_path, &mut tx).await;
            }
        } else if let Some(line) = line.strip_prefix("workspacev2>>") {
            let Some((id, _)) = line.split_once(",") else {
                tracing::error!(
                    "Received a `workspacev2` update `{line}`, but it doesn't contain any `,`"
                );
                continue;
            };
            let id = if id.is_empty() {
                None
            } else {
                match id.parse() {
                    Ok(x) => Some(x),
                    Err(e) => {
                        tracing::error!("Failed to parse the id ({id}) from `workspacev2`: {e}");
                        continue;
                    }
                }
            };

            tx.send(Message::NewActiveWorkspace(id)).await;
        } else if let Some(line) = line.strip_prefix("activespecialv2>>") {
            let Some((id, _)) = line.split_once(",") else {
                tracing::error!(
                    "Received a `activespecialv2` update `{line}`, but it doesn't contain any `,`"
                );
                continue;
            };
            let id = if id.is_empty() {
                None
            } else {
                match id.parse() {
                    Ok(x) => Some(x),
                    Err(e) => {
                        tracing::error!(
                            "Failed to parse the id ({id}) from `activespecialv2`: {e}"
                        );
                        continue;
                    }
                }
            };

            tx.send(Message::NewActiveSpecialWorkspace(id)).await;
        };
    }
}

async fn try_update_with_get_workspace<P>(
    command_socket_path: P,
    tx: &mut iced_runtime::task::Sender<Message>,
) where
    P: AsRef<Path> + Display + Copy,
{
    match get_workspaces(command_socket_path).await {
        Ok(workspaces) => {
            tx.send(Message::SetWorkspapces(workspaces)).await;
        }
        Err(e) => {
            tracing::error!(
                "Failed to get workspaces from hyprland socket at `{command_socket_path}`: {e}"
            );
            tx.send(Message::Error(e)).await;
        }
    }
}

struct WorkspaceInfo {
    name: String,
    // monitor: String,
    // monitor_id: i64,
    // windows: i32,
    // has_fullscreen: bool,
    // last_window: String, // TODO: should be i64, but use string for now
    // last_window_title: String,
    // is_persistent: bool,
}

async fn get_workspaces<P>(command_socket_path: P) -> Result<BTreeMap<i64, WorkspaceInfo>, String>
where
    P: AsRef<Path> + Display,
{
    let mut stream = UnixStream::connect(&command_socket_path)
        .await
        .map_err(|e| {
            format!("error while connecting to hyprland socket ({command_socket_path}): {e}")
        })?;

    stream
        .write_all(b"j/workspaces")
        .await
        .map_err(|e| format!("write_all error: {e}"))?;

    let mut buffer = vec![];
    stream
        .read_to_end(&mut buffer)
        .await
        .map_err(|e| format!("read_to_end error: {e}"))?;

    let _ = stream.shutdown().await;

    let workspaces = serde_json::from_slice::<Vec<WorkspaceInfoRaw>>(&buffer)
        .map_err(|e| format!("parsing `{:?}`: {e}", String::from_utf8(buffer)))?;
    // .map_err(|e| format!("parsing error: {e}"))?;

    Ok(BTreeMap::from_iter(
        workspaces.into_iter().map(|x| x.into()),
    ))
}

#[derive(Deserialize)]
struct WorkspaceInfoRaw {
    id: i64,
    name: String,
    // monitor: String,
    // #[serde(rename = "monitorID")]
    // monitor_id: i64,
    // windows: i32,
    // #[serde(rename = "hasfullscreen")]
    // has_fullscreen: bool,
    // #[serde(rename = "lastwindow")]
    // last_window: String, // TODO: should be i64, but use string for now
    // #[serde(rename = "lastwindowtitle")]
    // last_window_title: String,
    // #[serde(rename = "ispersistent")]
    // is_persistent: bool,
}

impl From<WorkspaceInfoRaw> for (i64, WorkspaceInfo) {
    fn from(value: WorkspaceInfoRaw) -> Self {
        (
            value.id,
            WorkspaceInfo {
                name: value.name,
                // monitor: value.monitor,
                // monitor_id: value.monitor_id,
                // windows: value.windows,
                // has_fullscreen: value.has_fullscreen,
                // last_window: value.last_window,
                // last_window_title: value.last_window_title,
                // is_persistent: value.is_persistent,
            },
        )
    }
}

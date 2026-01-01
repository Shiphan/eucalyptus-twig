use std::{collections::BTreeMap, env, fmt::Display, path::Path};

use futures::{
    AsyncReadExt, AsyncWriteExt,
    io::{AsyncBufReadExt, BufReader},
};
use gpui::{
    AsyncApp, Context, IntoElement, ParentElement, Render, Styled, WeakEntity, Window, black, div,
    opaque_grey, rems,
};
use gpui_net::async_net::UnixStream;
use serde::Deserialize;

use crate::widget::{Widget, widget_wrapper};

pub struct HyprlandWorkspace {
    error_message: Option<String>,
    workspaces: BTreeMap<i64, WorkspaceInfo>,
    active_workspace: Option<i64>,
    active_special_workspace: Option<i64>,
}

impl Widget for HyprlandWorkspace {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(info).detach();

        Self {
            error_message: None,
            workspaces: BTreeMap::new(),
            active_workspace: None,
            active_special_workspace: None,
        }
    }
}

impl Render for HyprlandWorkspace {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            return widget_wrapper().child(e.trim().to_owned());
        }

        widget_wrapper()
            .flex()
            .gap(rems(0.5))
            .children(self.workspaces.iter().map(|(&id, info)| {
                if Some(id) == self.active_workspace || Some(id) == self.active_special_workspace {
                    div()
                        .text_color(black())
                        .bg(opaque_grey(1.0, 0.75))
                        .rounded(rems(0.5))
                        .child(format!(" > {} < ", info.name))
                } else {
                    div().child(info.name.clone())
                }
            }))
        // .child(format!("special: {:?}", self.active_special_workspace))
        // .child(format!("workspace: {:?}", self.active_workspace))
    }
}

async fn info(this: WeakEntity<HyprlandWorkspace>, cx: &mut AsyncApp) {
    let hyprland_instance_signature = match env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        Ok(x) => x,
        Err(e) => {
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!(
                    "error while getting HYPRLAND_INSTANCE_SIGNATURE: {e}"
                ));
                cx.notify()
            });
            return;
        }
    };
    let runtime_dir = match env::var("XDG_RUNTIME_DIR") {
        Ok(xdg_runtime_dir) => format!("{xdg_runtime_dir}/hypr"),
        Err(e) => {
            // TODO: use the fallback format!("/run/user/{uid}/hypr"):
            // <https://github.com/hyprwm/Hyprland/blob/main/hyprctl/src/main.cpp>
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!("error while getting XDG_RUNTIME_DIR: {e}"));
                cx.notify()
            });
            return;
        }
    };

    let event_socket_path = format!("{runtime_dir}/{hyprland_instance_signature}/.socket2.sock");
    let command_socket_path = format!("{runtime_dir}/{hyprland_instance_signature}/.socket.sock");

    let mut event_stream = match UnixStream::connect(&event_socket_path).await {
        Ok(x) => BufReader::new(x),
        Err(e) => {
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(format!(
                    "error while connecting to hyprland socket ({event_socket_path}): {e}"
                ));
                cx.notify()
            });
            return;
        }
    };

    match get_workspaces(&command_socket_path).await {
        Ok(workspaces) => {
            let _ = this.update(cx, |this, cx| {
                this.workspaces = workspaces;
                cx.notify()
            });
        }
        Err(e) => {
            println!("{e}");
            let _ = this.update(cx, |this, cx| {
                this.error_message = Some(e);
                cx.notify()
            });
        }
    }

    loop {
        let mut line = String::new();
        match event_stream.read_line(&mut line).await {
            Ok(_) => (),
            Err(e) => {
                let _ = this.update(cx, |this, cx| {
                    this.error_message = Some(format!("error while reading the socket: {e}"));
                    cx.notify()
                });
                break;
            }
        };

        // FIXME: remove all unwrap!!!
        if let Some(_line) = line.strip_prefix("createworkspacev2>>") {
            let workspaces = get_workspaces(&command_socket_path).await.unwrap();
            let _ = this.update(cx, |this, cx| {
                this.workspaces = workspaces;
                cx.notify()
            });
        } else if let Some(line) = line.strip_prefix("destroyworkspacev2>>") {
            let (id, name) = line.split_once(",").unwrap();
            let _ = this.update(cx, |this, cx| {
                let workspace = this.workspaces.remove(&id.parse().unwrap()).unwrap();
                assert_eq!(workspace.name, name.trim_end());
                cx.notify()
            });
        } else if let Some(line) = line.strip_prefix("workspacev2>>") {
            let Some((id, _)) = line.split_once(",") else {
                let _ = this.update(cx, |this, cx| {
                    this.error_message = Some(format!("error while parsing workspacev2 `{line}`"));
                    cx.notify()
                });
                continue;
            };
            let id = if id.is_empty() {
                None
            } else {
                match id.parse() {
                    Ok(x) => Some(x),
                    Err(e) => {
                        let _ = this.update(cx, |this, cx| {
                            this.error_message =
                                Some(format!("error while parsing workspacev2 `{line}`: {e}"));
                            cx.notify()
                        });
                        continue;
                    }
                }
            };

            let _ = this.update(cx, |this, cx| {
                this.active_workspace = id;
                cx.notify()
            });
        } else if let Some(line) = line.strip_prefix("activespecialv2>>") {
            let Some((id, _)) = line.split_once(",") else {
                let _ = this.update(cx, |this, cx| {
                    this.error_message =
                        Some(format!("error while parsing activespecialv2 `{line}`"));
                    cx.notify()
                });
                continue;
            };
            let id = if id.is_empty() {
                None
            } else {
                match id.parse() {
                    Ok(x) => Some(x),
                    Err(e) => {
                        let _ = this.update(cx, |this, cx| {
                            this.error_message =
                                Some(format!("error while parsing activespecialv2 `{line}`: {e}"));
                            cx.notify()
                        });
                        continue;
                    }
                }
            };

            let _ = this.update(cx, |this, cx| {
                this.active_special_workspace = id;
                cx.notify()
            });
        };

        // if let Some(message) = message {
        //     let _ = this.update(cx, |this, cx| {
        //         this.info = message;
        //         cx.notify()
        //     });
        // }
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

    let _ = stream.close().await;

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

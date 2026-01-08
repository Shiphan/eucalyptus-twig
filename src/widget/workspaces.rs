use std::{collections::HashMap, thread};

use futures::{
    StreamExt,
    channel::mpsc::{self, UnboundedSender},
};
use gpui::{
    AsyncApp, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, WeakEntity, Window, black, div, opaque_grey, red, rems,
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    protocol::wl_registry::{self, WlRegistry},
};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};

use crate::widget::{Widget, widget_wrapper};

const IGNORE_HIDDEN: bool = true;

pub struct Workspaces {
    error_message: Option<String>,
    workspaces: HashMap<ExtWorkspaceHandleV1, Workspace>,
}

impl Widget for Workspaces {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(task).detach();

        Self {
            error_message: None,
            workspaces: HashMap::new(),
        }
    }
}

impl Render for Workspaces {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            return widget_wrapper().child(e.trim().to_owned());
        }

        widget_wrapper().flex().gap(rems(0.5)).children(
            self.workspaces
                .iter()
                .enumerate()
                .filter_map(|(index, (handle, workspace))| {
                    if !IGNORE_HIDDEN && workspace.state.hidden {
                        None
                    } else {
                        let name = if workspace.state.active {
                            format!(" > {} < ", workspace.name)
                        } else {
                            workspace.name.clone()
                        };

                        let div = if workspace.state.urgent {
                            div().text_color(black()).bg(red()).rounded(rems(0.5))
                        } else if workspace.state.active {
                            div()
                                .text_color(black())
                                .bg(opaque_grey(1.0, 0.75))
                                .rounded(rems(0.5))
                        } else {
                            div()
                        };
                        Some(if workspace.capabilities.activate {
                            div.id(format!("workspace-{index}"))
                                .on_click({
                                    let handle = handle.clone();
                                    move |_, _, _| {
                                        handle.activate();
                                    }
                                })
                                .child(name)
                                .into_any_element()
                        } else {
                            div.child(name).into_any_element()
                        })
                    }
                }),
        )
    }
}

async fn task(this: WeakEntity<Workspaces>, cx: &mut AsyncApp) {
    let (tx, mut rx) = mpsc::unbounded();
    // TODO: see if thread is avoidable using `event_queue.poll_dispatch_pending`
    thread::spawn(move || wayland_thread(tx));
    while let Some(update) = rx.next().await {
        let _ = this.update(cx, |this, cx| {
            match update {
                Update::NewWorkspace { handle, workspace } => {
                    this.workspaces.insert(handle, workspace);
                }
                Update::WorkspaceEvent { handle, event } => {
                    use ext_workspace_handle_v1::Event;

                    let Some(workspace) = this.workspaces.get_mut(&handle) else {
                        tracing::error!(?handle, ?event, "A new event for non-existing workspace");
                        return;
                    };
                    match event {
                        Event::Id { id } => {
                            tracing::info!(id);
                            workspace.id = Some(id);
                        }
                        Event::Name { name } => {
                            tracing::info!(name);
                            workspace.name = name;
                        }
                        Event::Coordinates { coordinates } => {
                            tracing::info!(?coordinates);
                            workspace.coordinates = Some(coordinates);
                        }
                        Event::State { state } => {
                            let state = match state.into_result() {
                                Ok(x) => x,
                                Err(e) => {
                                    tracing::error!(error = %e, "Failed to extract state");
                                    return;
                                }
                            };
                            tracing::info!(?state);
                            workspace.state = state.into();
                        }
                        Event::Capabilities { capabilities } => {
                            let capabilities = match capabilities.into_result() {
                                Ok(x) => x,
                                Err(e) => {
                                    tracing::error!(error = %e, "Failed to extract state");
                                    return;
                                }
                            };
                            tracing::info!(?capabilities);
                            workspace.capabilities = capabilities.into();
                        }
                        Event::Removed => {
                            if this.workspaces.remove(&handle).is_none() {
                                tracing::error!("Remove event for a non-existing workspace");
                            }
                            tracing::info!(?handle, "remove workspace");
                        }
                        _ => (),
                    }
                }
                Update::Error(e) => {
                    this.error_message = Some(e);
                }
            }
            cx.notify();
        });
    }
}

fn wayland_thread(tx: UnboundedSender<Update>) {
    let connection = match Connection::connect_to_env() {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to wayland server");
            if let Err(e) = tx.unbounded_send(Update::Error(format!(
                "Failed to connect to wayland server: {e}"
            ))) {
                tracing::error!(error = %e, "Failed to send update to ui thread");
            }
            return;
        }
    };
    let display = connection.display();
    let mut event_queue = connection.new_event_queue();
    let queue_handle = event_queue.handle();
    let _registry = display.get_registry(&queue_handle, ());
    let mut state = State::new(tx);
    loop {
        if let Err(e) = event_queue.blocking_dispatch(&mut state) {
            tracing::error!(error = %e, "Wayland dispatch error");
            if let Err(e) = state
                .tx
                .unbounded_send(Update::Error(format!("Wayland dispatch error: {e}")))
            {
                tracing::error!(error = %e, "Failed to send update to ui thread");
            }
            break;
        }
        tracing::info!("wayland dispatch");
    }
}

struct Workspace {
    id: Option<String>,
    name: String,
    coordinates: Option<Vec<u8>>,
    state: WorkspaceState,
    capabilities: WorkspaceCapabilities,
}

struct WorkspaceState {
    active: bool,
    urgent: bool,
    hidden: bool,
}

impl From<ext_workspace_handle_v1::State> for WorkspaceState {
    fn from(value: ext_workspace_handle_v1::State) -> Self {
        use ext_workspace_handle_v1::State;

        let active = value.contains(State::Active);
        let urgent = value.contains(State::Urgent);
        let hidden = value.contains(State::Hidden);
        Self {
            active,
            urgent,
            hidden,
        }
    }
}

// TODO: use other workspace capabilities
#[allow(dead_code)]
struct WorkspaceCapabilities {
    activate: bool,
    deactivate: bool,
    remove: bool,
    assign: bool,
}

impl From<ext_workspace_handle_v1::WorkspaceCapabilities> for WorkspaceCapabilities {
    fn from(value: ext_workspace_handle_v1::WorkspaceCapabilities) -> Self {
        use ext_workspace_handle_v1::WorkspaceCapabilities;

        let activate = value.contains(WorkspaceCapabilities::Activate);
        let deactivate = value.contains(WorkspaceCapabilities::Deactivate);
        let remove = value.contains(WorkspaceCapabilities::Remove);
        let assign = value.contains(WorkspaceCapabilities::Assign);
        Self {
            activate,
            deactivate,
            remove,
            assign,
        }
    }
}

#[derive(Debug, Default)]
struct PendingWorkspace {
    id: Option<String>,
    name: Option<String>,
    coordinates: Option<Vec<u8>>,
    state: Option<ext_workspace_handle_v1::State>,
    capabilities: Option<ext_workspace_handle_v1::WorkspaceCapabilities>,
}

enum Update {
    NewWorkspace {
        handle: ExtWorkspaceHandleV1,
        workspace: Workspace,
    },
    WorkspaceEvent {
        handle: ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
    },
    Error(String),
}

struct State {
    tx: UnboundedSender<Update>,
    workspace_manager: Option<ExtWorkspaceManagerV1>,
    pending_workspaces: HashMap<ExtWorkspaceHandleV1, PendingWorkspace>,
}

impl State {
    fn new(tx: UnboundedSender<Update>) -> Self {
        Self {
            tx,
            workspace_manager: None,
            pending_workspaces: HashMap::new(),
        }
    }
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        use wl_registry::Event;

        match event {
            Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "ext_workspace_manager_v1" => {
                    tracing::info!(name, interface, version);
                    let workspace_manager =
                        proxy.bind::<ExtWorkspaceManagerV1, _, _>(name, version, qhandle, ());
                    state.workspace_manager = Some(workspace_manager);
                }
                _ => (),
            },
            _ => (),
        }
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for State {
    fn event(
        state: &mut Self,
        _proxy: &ExtWorkspaceManagerV1,
        event: ext_workspace_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        use ext_workspace_manager_v1::Event;

        tracing::info!(?event, "ext_workspace_manager_v1");
        match event {
            Event::WorkspaceGroup { workspace_group } => {
                tracing::info!(?workspace_group);
            }
            Event::Workspace { workspace } => {
                tracing::info!(?workspace);
                state
                    .pending_workspaces
                    .insert(workspace, PendingWorkspace::default());
            }
            Event::Done => {}
            Event::Finished => {}
            _ => (),
        }
    }

    wayland_client::event_created_child!(State, ExtWorkspaceManagerV1, [
        ext_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE => (ExtWorkspaceGroupHandleV1, ()),
        ext_workspace_manager_v1::EVT_WORKSPACE_OPCODE => (ExtWorkspaceHandleV1, ()),
    ]);
}

// TODO: handle workspace group
#[allow(unused)]
impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ExtWorkspaceGroupHandleV1,
        event: ext_workspace_group_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        use ext_workspace_group_handle_v1::Event;

        tracing::info!(?event, "ext_workspace_group_handle_v1");
        match event {
            Event::Capabilities { capabilities } => {}
            Event::OutputEnter { output } => {}
            Event::OutputLeave { output } => {}
            Event::WorkspaceEnter { workspace } => {}
            Event::WorkspaceLeave { workspace } => {}
            Event::Removed => {}
            _ => (),
        }
    }
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        use ext_workspace_handle_v1::Event;

        tracing::info!(?event, "ext_workspace_handle_v1");
        if let Some((handle, mut pending_workspace)) = state.pending_workspaces.remove_entry(proxy)
        {
            match event {
                Event::Id { id } => {
                    tracing::info!(id);
                    pending_workspace.id = Some(id);
                }
                Event::Name { name } => {
                    tracing::info!(name);
                    pending_workspace.name = Some(name);
                }
                Event::Coordinates { coordinates } => {
                    tracing::info!(?coordinates);
                    pending_workspace.coordinates = Some(coordinates);
                }
                Event::State { state } => {
                    let state = match state.into_result() {
                        Ok(x) => x,
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to extract state");
                            return;
                        }
                    };
                    tracing::info!(?state);
                    pending_workspace.state = Some(state);
                }
                Event::Capabilities { capabilities } => {
                    let capabilities = match capabilities.into_result() {
                        Ok(x) => x,
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to extract state");
                            return;
                        }
                    };
                    tracing::info!(?capabilities);
                    pending_workspace.capabilities = Some(capabilities);
                }
                Event::Removed => {
                    tracing::info!(?pending_workspace, "remove pending workspace");
                    return;
                }
                _ => (),
            }

            if let PendingWorkspace {
                id,
                name: Some(name),
                coordinates,
                state: Some(workspace_state),
                capabilities: Some(capabilities),
            } = pending_workspace
            {
                if let Err(e) = state.tx.unbounded_send(Update::NewWorkspace {
                    handle,
                    workspace: Workspace {
                        id,
                        name,
                        coordinates,
                        state: workspace_state.into(),
                        capabilities: capabilities.into(),
                    },
                }) {
                    tracing::error!(error = %e, "Failed to send update to ui thread");
                }
            } else {
                tracing::info!(?pending_workspace);
                state.pending_workspaces.insert(handle, pending_workspace);
            }
            tracing::info!(pending_workspaces = state.pending_workspaces.len());
        } else {
            if let Err(e) = state.tx.unbounded_send(Update::WorkspaceEvent {
                handle: proxy.clone(),
                event,
            }) {
                tracing::error!(error = %e, "Failed to send update to ui thread");
            }
        }
    }
}

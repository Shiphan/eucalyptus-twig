use std::{borrow::Cow, collections::HashMap};

use iced_futures::Subscription;
use iced_runtime::Task;
use wayland_client::{
    Connection,
    Dispatch,
    QueueHandle,
    protocol::wl_registry::{self, WlRegistry},
};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};

use crate::{application::Element, widget::Widget};

const IGNORE_HIDDEN: bool = true;

#[allow(private_interfaces)]
pub enum Workspaces {
    Ok {
        workspaces: HashMap<ExtWorkspaceHandleV1, Workspace>,
    },
    Err {
        message: String,
    },
}

impl Widget for Workspaces {
    type Config = ();

    type Message = Message;

    fn new((): &Self::Config) -> (Self, Task<Self::Message>) {
        (
            Self::Ok {
                workspaces: HashMap::new(),
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>> {
        match (self, message) {
            (Self::Ok { workspaces }, Message::NewWorkspace { handle, workspace }) => {
                workspaces.insert(handle, workspace);
            }
            (Self::Ok { workspaces }, Message::WorkspaceEvent { handle, event }) => {
                use ext_workspace_handle_v1::Event;

                let Some(workspace) = workspaces.get_mut(&handle) else {
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
                        let (coordinates, remainder) = coordinates.as_chunks();
                        if !remainder.is_empty() {
                            tracing::warn!(remainder, "coordinates' length is not multiples of 4");
                        }
                        let coordinates =
                            coordinates.iter().map(|x| u32::from_ne_bytes(*x)).collect();
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
                        if workspaces.remove(&handle).is_none() {
                            tracing::error!("Remove event for a non-existing workspace");
                        }
                        tracing::info!(?handle, "remove workspace");
                    }
                    _ => (),
                }
            }
            (Self::Ok { .. }, Message::ActivateWorkspace(handle)) => {
                handle.activate();
            }
            (s, Message::Error(message)) => {
                *s = Self::Err { message };
            }
            (Self::Err { .. }, _) => (),
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        match self {
            Self::Ok { workspaces } => {
                let mut workspaces = workspaces
                    .iter()
                    .filter_map(|(handle, workspace)| {
                        if !IGNORE_HIDDEN && workspace.state.hidden {
                            None
                        } else {
                            let name = if workspace.state.active {
                                Cow::Owned(format!(" > {} < ", workspace.name))
                            } else {
                                Cow::Borrowed(workspace.name.as_str())
                            };

                            let widget = iced_widget::text(name).style(if workspace.state.urgent {
                                iced_widget::text::warning
                            } else if workspace.state.active {
                                iced_widget::text::secondary
                            } else {
                                iced_widget::text::default
                            });
                            Some((
                                &workspace.coordinates,
                                if workspace.capabilities.activate {
                                    Element::from(
                                        iced_widget::mouse_area(widget).on_press(handle.clone()),
                                    )
                                    .map(|handle| Message::ActivateWorkspace(handle))
                                } else {
                                    widget.into()
                                },
                            ))
                        }
                    })
                    .collect::<Vec<_>>();
                workspaces.sort_by_key(|(x, _)| match x {
                    Some(x) => x.as_slice(),
                    None => &[],
                });
                iced_widget::row(workspaces.into_iter().map(|(_, x)| x)).into()
            }
            Self::Err { message } => iced_widget::text(message.trim()).into(),
        }
    }

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        match self {
            Self::Ok { .. } => Subscription::run(|| iced_runtime::task::sipper(task)),
            Self::Err { .. } => Subscription::none(),
        }
    }
}

#[allow(private_interfaces)]
pub enum Message {
    NewWorkspace {
        handle: ExtWorkspaceHandleV1,
        workspace: Workspace,
    },
    WorkspaceEvent {
        handle: ExtWorkspaceHandleV1,
        event: ext_workspace_handle_v1::Event,
    },
    ActivateWorkspace(ExtWorkspaceHandleV1),
    Error(String),
}

async fn task(tx: iced_runtime::task::Sender<Message>) {
    // TODO: see if thread is avoidable using `event_queue.poll_dispatch_pending`
    if let Err(e) = tokio::task::spawn_blocking(move || wayland_thread(tx)).await {
        tracing::error!(%e, "Join error");
    }
}

fn wayland_thread(mut tx: iced_runtime::task::Sender<Message>) {
    let handle = tokio::runtime::Handle::current();

    let connection = match Connection::connect_to_env() {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(error = %e, "Failed to connect to wayland server");
            handle.block_on(tx.send(Message::Error(format!(
                "Failed to connect to wayland server: {e}"
            ))));
            return;
        }
    };
    let display = connection.display();
    let mut event_queue = connection.new_event_queue();
    let queue_handle = event_queue.handle();
    let _registry = display.get_registry(&queue_handle, ());
    let mut state = State::new(handle.clone(), tx.clone());
    loop {
        if let Err(e) = event_queue.blocking_dispatch(&mut state) {
            tracing::error!(error = %e, "Wayland dispatch error");
            handle.block_on(tx.send(Message::Error(format!("Wayland dispatch error: {e}"))));
            break;
        }
        tracing::info!("wayland dispatch");
    }
}

struct Workspace {
    id: Option<String>,
    name: String,
    coordinates: Option<Vec<u32>>,
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
    coordinates: Option<Vec<u32>>,
    state: Option<ext_workspace_handle_v1::State>,
    capabilities: Option<ext_workspace_handle_v1::WorkspaceCapabilities>,
}

struct State {
    runtime_handle: tokio::runtime::Handle,
    tx: iced_runtime::task::Sender<Message>,
    workspace_manager: Option<ExtWorkspaceManagerV1>,
    pending_workspaces: HashMap<ExtWorkspaceHandleV1, PendingWorkspace>,
}

impl State {
    fn new(
        runtime_handle: tokio::runtime::Handle,
        tx: iced_runtime::task::Sender<Message>,
    ) -> Self {
        Self {
            runtime_handle,
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
                    let (coordinates, remainder) = coordinates.as_chunks();
                    if !remainder.is_empty() {
                        tracing::warn!(remainder, "coordinates' length is not multiples of 4");
                    }
                    let coordinates = coordinates.iter().map(|x| u32::from_ne_bytes(*x)).collect();
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
                state
                    .runtime_handle
                    .block_on(state.tx.send(Message::NewWorkspace {
                        handle,
                        workspace: Workspace {
                            id,
                            name,
                            coordinates,
                            state: workspace_state.into(),
                            capabilities: capabilities.into(),
                        },
                    }));
            } else {
                tracing::info!(?pending_workspace);
                state.pending_workspaces.insert(handle, pending_workspace);
            }
            tracing::info!(pending_workspaces = state.pending_workspaces.len());
        } else {
            state
                .runtime_handle
                .block_on(state.tx.send(Message::WorkspaceEvent {
                    handle: proxy.clone(),
                    event,
                }));
        }
    }
}

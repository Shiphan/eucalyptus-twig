#![allow(unused)] // TODO: temporarily allow unused

use std::{
    collections::HashMap,
    fmt::Debug,
    num::NonZero,
    ptr::NonNull,
    time::{Duration, Instant},
};

use futures::{
    SinkExt,
    channel::mpsc::{self, TryRecvError, UnboundedReceiver, UnboundedSender},
};
use iced_core::{Font, Pixels, Point, Size, Theme};
use iced_futures::Subscription;
use iced_runtime::{Task, UserInterface, user_interface};
use iced_wgpu::{Renderer, wgpu};
use raw_window_handle::{WaylandDisplayHandle, WaylandWindowHandle};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    output::{OutputHandler, OutputState},
    reexports::{
        calloop::EventLoop,
        calloop_wayland_source::WaylandSource,
        client::{
            Connection,
            Proxy,
            QueueHandle,
            globals::registry_queue_init,
            protocol::{
                wl_keyboard::WlKeyboard,
                wl_output::{Transform, WlOutput},
                wl_pointer::WlPointer,
                wl_seat::WlSeat,
                wl_surface::WlSurface,
            },
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    seat::{
        self,
        Capability,
        SeatHandler,
        SeatState,
        keyboard::KeyboardHandler,
        pointer::{
            AxisScroll,
            BTN_BACK,
            BTN_FORWARD,
            BTN_LEFT,
            BTN_MIDDLE,
            BTN_RIGHT,
            PointerEvent,
            PointerEventKind,
            PointerHandler,
        },
    },
    shell::{
        WaylandSurface,
        wlr_layer::{self, LayerShell, LayerShellHandler, LayerSurface},
        xdg::{
            XdgShell,
            window::{Window, WindowConfigure, WindowHandler},
        },
    },
};

const HEIGHT: u32 = 40;

pub type Element<'a, Message, Theme = iced_core::Theme, Renderer = iced_wgpu::Renderer> =
    iced_core::Element<'a, Message, Theme, Renderer>;

pub struct Application<'a, State, Message>
where
    Message: Send + 'static,
    State: self::State,
{
    state: State,
    boot_task: Task<Message>,
    wayland_event_loop: EventLoop<'a, WaylandClient>,
    wayland_client: WaylandClient,
    wayland_event_rx: UnboundedReceiver<WaylandEvent>,
    wgpu_instance: wgpu::Instance,
    wgpu_adapter: wgpu::Adapter,
    wgpu_device: wgpu::Device,
    wgpu_engine: iced_wgpu::Engine,
    surfaces: HashMap<WlSurface, SurfaceState>,
    clipboard: Clipboard,
    // pending_event: Vec<iced_core::Event>,
    iced_futures_runtime: iced_futures::Runtime<
        iced_futures::backend::default::Executor,
        UnboundedSender<iced_runtime::Action<Message>>,
        iced_runtime::Action<Message>,
    >,
    future_message_rx: UnboundedReceiver<iced_runtime::Action<Message>>,
}

impl<State, Message> Application<'_, State, Message>
where
    Message: Send + 'static,
    State: self::State<Message = Message>,
{
    pub fn new(state: State, boot_task: Task<Message>) -> Result<Self, Box<dyn std::error::Error>> {
        let (wayland_event_tx, wayland_event_rx) = mpsc::unbounded();
        let (wayland_client, wayland_event_loop) = WaylandClient::new(wayland_event_tx)?;

        let wgpu_instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());

        let wgpu_adapter = futures::executor::block_on(
            wgpu_instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )?;
        let (wgpu_device, wgpu_queue) = futures::executor::block_on(
            wgpu_adapter.request_device(&wgpu::wgt::DeviceDescriptor::default()),
        )?;

        let wgpu_engine = iced_wgpu::Engine::new(
            &wgpu_adapter,
            wgpu_device.clone(),
            wgpu_queue,
            wgpu::TextureFormat::Bgra8UnormSrgb,
            Some(iced_graphics::Antialiasing::MSAAx4),
            iced_graphics::Shell::headless(),
        );

        let (future_message_tx, future_message_rx) = mpsc::unbounded();
        let iced_futures_runtime = iced_futures::Runtime::new(
            iced_futures::backend::default::Executor::new()?,
            future_message_tx,
        );

        Ok(Self {
            state,
            boot_task,
            wayland_event_loop,
            wayland_client,
            wayland_event_rx,
            wgpu_instance,
            wgpu_adapter,
            wgpu_device,
            wgpu_engine,
            surfaces: HashMap::new(),
            clipboard: Clipboard,
            // pending_event: vec![],
            iced_futures_runtime,
            future_message_rx,
        })
    }
    pub fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(stream) = iced_runtime::task::into_stream(std::mem::take(&mut self.boot_task)) {
            self.iced_futures_runtime.run(stream);
        }

        self.iced_futures_runtime
            .track(iced_futures::subscription::into_recipes(
                self.iced_futures_runtime
                    .enter(|| self.state.subscription().into())
                    .map(iced_runtime::Action::Output),
            ));

        // TODO: Make it only wake up when needed
        loop {
            self.wayland_event_loop
                .dispatch(Some(Duration::from_millis(10)), &mut self.wayland_client)?;

            match self.wayland_event_rx.try_recv() {
                Ok(event) => {
                    self.on_wayland_event(event)?;
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Closed) => {
                    tracing::warn!("channel `wayland_event_rx` closed");
                }
            }

            match self.future_message_rx.try_recv() {
                Ok(iced_runtime::Action::Output(message)) => {
                    let task = self.state.update(message).into();
                    if let Some(stream) = iced_runtime::task::into_stream(task) {
                        self.iced_futures_runtime.run(stream);
                    }
                }
                Ok(iced_runtime::Action::LoadFont { .. }) => {
                    tracing::warn!("Action::LoadFont is not being handled");
                }
                Ok(iced_runtime::Action::Widget(_)) => {
                    tracing::warn!("Action::Widget is not being handled");
                }
                Ok(iced_runtime::Action::Clipboard(_)) => {
                    tracing::warn!("Action::Clipboard is not being handled");
                }
                Ok(iced_runtime::Action::Window(_)) => {
                    tracing::warn!("Action::WIndow is not being handled");
                }
                Ok(iced_runtime::Action::System(_)) => {
                    tracing::warn!("Action::System is not being handled");
                }
                Ok(iced_runtime::Action::Image(_)) => {
                    tracing::warn!("Action::Image is not being handled");
                }
                Ok(iced_runtime::Action::Reload) => {
                    tracing::warn!("Action::Reload is not being handled");
                }
                Ok(iced_runtime::Action::Exit) => {
                    return Ok(());
                }
                Err(TryRecvError::Empty) => (),
                Err(TryRecvError::Closed) => {
                    tracing::warn!("channel `wayland_event_rx` closed");
                }
            }
        }
    }
    fn open_new_layer_surface(
        &mut self,
        wl_output: WlOutput,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let layer_surface = self.wayland_client.create_layer_surface(
            wlr_layer::Layer::Top,
            Some("ui-test::iced-with-custom-window"),
            Some(&wl_output),
            None,
            NonZero::new(HEIGHT),
            Some(wlr_layer::Anchor::TOP | wlr_layer::Anchor::LEFT | wlr_layer::Anchor::RIGHT),
            Some(HEIGHT as _),
        )?;

        let wgpu_surface = {
            let raw_display_handle = WaylandDisplayHandle::new(
                NonNull::new(self.wayland_client.connection.backend().display_ptr() as _)
                    .ok_or("wayland display pointer is null")?,
            )
            .into();
            let raw_window_handle = WaylandWindowHandle::new(
                NonNull::new(layer_surface.wl_surface().id().as_ptr() as _)
                    .ok_or("wl_surface is null")?,
            )
            .into();

            unsafe {
                self.wgpu_instance
                    .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                        raw_display_handle,
                        raw_window_handle,
                    })
            }?
        };

        let renderer = Renderer::new(self.wgpu_engine.clone(), Font::default(), Pixels(16.0));

        self.surfaces.insert(
            layer_surface.wl_surface().clone(),
            SurfaceState {
                wl_surface: layer_surface.wl_surface().clone(),
                layer_surface,
                wgpu_surface,
                renderer,
                user_interface_cache: user_interface::Cache::new(),
                view_port: None,
                cursor: iced_core::mouse::Cursor::Unavailable,
                pending_event: vec![],
                iced_size: None,
            },
        );

        Ok(())
    }
    fn configure_surface(
        &mut self,
        surface: &WlSurface,
        width: NonZero<u32>,
        height: NonZero<u32>,
    ) {
        if let Some(it) = self.surfaces.get_mut(surface) {
            it.view_port = Some(iced_graphics::Viewport::with_physical_size(
                Size::new(width.get(), height.get()),
                it.view_port
                    .as_ref()
                    .map_or(1.5, iced_graphics::Viewport::scale_factor),
            ));

            let mut wgpu_surface_configuration = it
                .wgpu_surface
                .get_default_config(&self.wgpu_adapter, width.get(), height.get())
                .unwrap(); // TODO: remove unwrap

            wgpu_surface_configuration.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;

            it.wgpu_surface
                .configure(&self.wgpu_device, &wgpu_surface_configuration);

            self.draw(surface);
        }
    }
    fn draw(&mut self, surface: &WlSurface) {
        let Some((Some(width), Some(height))) =
            self.wayland_client.surfaces.get(surface).map(|x| x.size())
        else {
            return;
        };
        let Some(it) = self.surfaces.get_mut(surface) else {
            return;
        };
        let surface_texture = match it.wgpu_surface.get_current_texture() {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!(error = %e, "wgpu_surface.get_current_texture() failed");
                it.wl_surface.frame(
                    &self.wayland_client.queue_handle,
                    FrameCallbackData(it.wl_surface.clone()),
                );
                it.wl_surface.commit();
                return;
            }
        };
        let texture_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut user_interface = UserInterface::build(
            self.state.view(),
            Size::new(width.get() as _, height.get() as _),
            std::mem::take(&mut it.user_interface_cache),
            &mut it.renderer,
        );

        let mut messages = vec![];
        let (user_interface_state, _event_statuses) = user_interface.update(
            std::mem::take(&mut it.pending_event).as_slice(),
            it.cursor,
            &mut it.renderer,
            &mut self.clipboard,
            &mut messages,
        );

        // TODO: this will make the window fit content size, but might worth more refinement on the solution
        #[derive(Debug)]
        struct InspectBounds(Option<Size<u32>>);

        impl iced_core::widget::Operation for InspectBounds {
            fn traverse(
                &mut self,
                _operate: &mut dyn FnMut(&mut dyn iced_core::widget::Operation<()>),
            ) {
            }
            fn container(&mut self, _id: Option<&iced_widget::Id>, bounds: iced_core::Rectangle) {
                let size = bounds.size();
                self.0 = Some(Size::new(size.width as _, size.height as _));
            }
        }

        let mut inspect_bounds = InspectBounds(None);

        user_interface.operate(&it.renderer, &mut inspect_bounds);

        if let Some(bounds) = inspect_bounds.0
            && it
                .iced_size
                .is_none_or(|(_width, height)| height != bounds.height)
        {
            tracing::info!(?inspect_bounds, "bounds");
            it.layer_surface.set_size(0, bounds.height);
            it.layer_surface.set_exclusive_zone(bounds.height as _);
            it.iced_size = Some((bounds.width, bounds.height));
        }

        user_interface.draw(
            &mut it.renderer,
            &Theme::Dark,
            &iced_core::renderer::Style::default(),
            it.cursor,
        );
        it.user_interface_cache = user_interface.into_cache();

        it.renderer.present(
            None,
            surface_texture.texture.format(),
            &texture_view,
            &iced_graphics::Viewport::with_physical_size(Size::new(width.get(), height.get()), 1.0),
        );

        surface_texture.present();

        it.wl_surface.frame(
            &self.wayland_client.queue_handle,
            FrameCallbackData(it.wl_surface.clone()),
        );
        it.wl_surface.commit();

        for message in messages {
            let task = self.state.update(message).into();
            if let Some(stream) = iced_runtime::task::into_stream(task) {
                self.iced_futures_runtime.run(stream);
            }
        }

        self.iced_futures_runtime
            .track(iced_futures::subscription::into_recipes(
                self.iced_futures_runtime
                    .enter(|| self.state.subscription().into())
                    .map(iced_runtime::Action::Output),
            ));
    }
    fn on_wayland_event(&mut self, event: WaylandEvent) -> Result<(), Box<dyn std::error::Error>> {
        match event {
            WaylandEvent::NewOutput(output) => {
                self.open_new_layer_surface(output)?;
            }
            WaylandEvent::SurfaceConfiure {
                wl_surface,
                width,
                height,
            } => {
                if let Some(it) = self.surfaces.get_mut(&wl_surface) {
                    it.pending_event.push(iced_core::Event::Window(
                        iced_core::window::Event::Resized(Size::new(
                            width.get() as _,
                            height.get() as _,
                        )),
                    ));
                }
                self.configure_surface(&wl_surface, width, height);
            }
            WaylandEvent::SurfaceFrame(surface) => {
                if let Some(it) = self.surfaces.get_mut(&surface) {
                    it.pending_event.push(iced_core::Event::Window(
                        iced_core::window::Event::RedrawRequested(Instant::now()),
                    ));
                }
                self.draw(&surface);
            }
            WaylandEvent::SurfaceClose(surface) => {
                self.surfaces.remove(&surface);
            }
            WaylandEvent::PointerEvents(events) => {
                for event in events {
                    if let Some(it) = self.surfaces.get_mut(&event.surface) {
                        let position = Point::new(event.position.0 as _, event.position.1 as _);
                        match event.kind {
                            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                                it.cursor = iced_core::mouse::Cursor::Available(position);
                            }
                            _ => (),
                        }

                        let button_from_u32 = |button| match button {
                            BTN_LEFT => iced_core::mouse::Button::Left,
                            BTN_RIGHT => iced_core::mouse::Button::Right,
                            BTN_MIDDLE => iced_core::mouse::Button::Middle,
                            BTN_BACK => iced_core::mouse::Button::Back,
                            BTN_FORWARD => iced_core::mouse::Button::Forward,
                            button => iced_core::mouse::Button::Other(button as _),
                        };
                        let mouse_event = match event.kind {
                            PointerEventKind::Enter { .. } => {
                                iced_core::mouse::Event::CursorEntered
                            }
                            PointerEventKind::Leave { .. } => iced_core::mouse::Event::CursorLeft,
                            PointerEventKind::Motion { .. } => {
                                iced_core::mouse::Event::CursorMoved { position }
                            }
                            PointerEventKind::Press { button, .. } => {
                                iced_core::mouse::Event::ButtonPressed(button_from_u32(button))
                            }
                            PointerEventKind::Release { button, .. } => {
                                iced_core::mouse::Event::ButtonReleased(button_from_u32(button))
                            }
                            PointerEventKind::Axis {
                                horizontal:
                                    AxisScroll {
                                        value120: horizontal_value120,
                                        ..
                                    },
                                vertical:
                                    AxisScroll {
                                        value120: vertical_value120,
                                        ..
                                    },
                                ..
                            } => iced_core::mouse::Event::WheelScrolled {
                                delta: iced_core::mouse::ScrollDelta::Lines {
                                    x: horizontal_value120 as f32 / 120.0,
                                    y: vertical_value120 as f32 / 120.0,
                                },
                            },
                        };
                        it.pending_event.push(iced_core::Event::Mouse(mouse_event));
                    }
                }
            }
        }

        Ok(())
    }
}

pub trait State: Sized {
    type Message: Send + 'static;

    fn update(&mut self, message: Self::Message) -> impl Into<Task<Self::Message>>;

    fn view(&self) -> impl Into<Element<'_, Self::Message>>;

    fn subscription(&self) -> impl Into<Subscription<Self::Message>> {
        Subscription::none()
    }
}

pub trait BootFn<State, Message> {
    fn boot(&self) -> (State, Task<Message>);
}

impl<T, State, Message, BootResult> BootFn<State, Message> for T
where
    T: Fn() -> BootResult,
    BootResult: Into<(State, Task<Message>)>,
{
    fn boot(&self) -> (State, Task<Message>) {
        self().into()
    }
}

pub trait UpdateFn<'a, State, Message> {
    fn update(&self, state: &'a mut State, message: Message) -> Task<Message>;
}

impl<'a, T, State, Message, UpdateResult> UpdateFn<'a, State, Message> for T
where
    T: Fn(&'a mut State, Message) -> UpdateResult,
    State: 'a,
    UpdateResult: Into<Task<Message>>,
{
    fn update(&self, state: &'a mut State, message: Message) -> Task<Message> {
        self(state, message).into()
    }
}

pub trait ViewFn<'a, State, Message> {
    fn view(&self, state: &'a State) -> Element<'a, Message>;
}

impl<'a, T, State, Message, ViewResult> ViewFn<'a, State, Message> for T
where
    T: Fn(&'a State) -> ViewResult,
    State: 'a,
    ViewResult: Into<Element<'a, Message>>,
{
    fn view(&self, state: &'a State) -> Element<'a, Message> {
        self(state).into()
    }
}

pub trait SubscriptionFn<'a, State, Message> {
    fn subscription(&self, state: &'a State) -> Subscription<Message>;
}

impl<'a, T, State, Message, SubscriptionResult> SubscriptionFn<'a, State, Message> for T
where
    T: Fn(&'a State) -> SubscriptionResult,
    State: 'a,
    SubscriptionResult: Into<Subscription<Message>>,
{
    fn subscription(&self, state: &'a State) -> Subscription<Message> {
        self(state).into()
    }
}

struct SurfaceState {
    wl_surface: WlSurface,
    layer_surface: LayerSurface,
    wgpu_surface: wgpu::Surface<'static>,
    renderer: iced_wgpu::Renderer,
    user_interface_cache: user_interface::Cache,
    view_port: Option<iced_graphics::Viewport>,
    cursor: iced_core::mouse::Cursor,
    pending_event: Vec<iced_core::Event>,
    iced_size: Option<(u32, u32)>,
}

struct Clipboard;

impl iced_core::Clipboard for Clipboard {
    fn read(&self, kind: iced_core::clipboard::Kind) -> Option<String> {
        None
    }

    fn write(&mut self, kind: iced_core::clipboard::Kind, contents: String) {}
}

struct WaylandClient {
    event_tx: UnboundedSender<WaylandEvent>,
    connection: Connection,
    queue_handle: QueueHandle<Self>,
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    // TODO: fractional_scale
    // fractional_scale_manager: WpFractionalScaleManagerV1,
    pointer: Option<WlPointer>,
    compositor_state: CompositorState,
    xdg_shell: XdgShell,
    layer_shell: LayerShell,
    surfaces: HashMap<WlSurface, Surface>,
}

impl WaylandClient {
    pub fn new<'a>(
        event_tx: UnboundedSender<WaylandEvent>,
    ) -> Result<(Self, EventLoop<'a, Self>), Box<dyn std::error::Error>> {
        let connection = Connection::connect_to_env()?;

        let (globals, event_queue) = registry_queue_init::<Self>(&connection)?;
        let qh = event_queue.handle();
        let event_loop = EventLoop::try_new()?;
        let loop_handle = event_loop.handle();
        WaylandSource::new(connection.clone(), event_queue).insert(loop_handle)?;

        let compositor_state = CompositorState::bind(&globals, &qh)?;
        let registry_state = RegistryState::new(&globals);
        let output_state = OutputState::new(&globals, &qh);
        let seat_state = SeatState::new(&globals, &qh);
        let xdg_shell = XdgShell::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)?;

        Ok((
            Self {
                event_tx,
                connection,
                queue_handle: qh,
                registry_state,
                output_state,
                seat_state,
                compositor_state,
                pointer: None,
                xdg_shell,
                layer_shell,
                surfaces: HashMap::new(),
            },
            event_loop,
        ))
    }
    fn create_layer_surface(
        &mut self,
        layer: wlr_layer::Layer,
        namespace: Option<impl Into<String>>,
        output: Option<&WlOutput>,
        width: Option<NonZero<u32>>,
        height: Option<NonZero<u32>>,
        anchor: Option<wlr_layer::Anchor>,
        exclusive_zone: Option<i32>,
    ) -> Result<LayerSurface, Box<dyn std::error::Error>> {
        let wl_surface = self.compositor_state.create_surface(&self.queue_handle);
        let layer_surface = self.layer_shell.create_layer_surface(
            &self.queue_handle,
            wl_surface.clone(),
            layer,
            namespace,
            output,
        );
        layer_surface.set_size(
            width.map_or(0, NonZero::get),
            height.map_or(0, NonZero::get),
        );
        if let Some(anchor) = anchor {
            layer_surface.set_anchor(anchor);
        }
        if let Some(zone) = exclusive_zone {
            layer_surface.set_exclusive_zone(zone);
        }
        layer_surface.commit();

        self.surfaces
            .insert(
                wl_surface.clone(),
                Surface::LayerSurface(LayerSurfaceInfo::new(layer_surface.clone())),
            )
            .map_or(Ok(()), |surface| {
                Err("A new WlSurface should not already be in surfaces")
            })?;

        Ok(layer_surface)
    }
}

enum Surface {
    Window(WindowInfo),
    LayerSurface(LayerSurfaceInfo),
}

impl Surface {
    fn size(&self) -> (Option<NonZero<u32>>, Option<NonZero<u32>>) {
        match self {
            Self::Window(WindowInfo { width, height, .. }) => (*width, *height),
            Self::LayerSurface(LayerSurfaceInfo { width, height, .. }) => (*width, *height),
        }
    }
    fn commit(&self) {
        match self {
            Self::Window(WindowInfo { window, .. }) => {
                window.commit();
            }
            Self::LayerSurface(LayerSurfaceInfo { layer_surface, .. }) => {
                layer_surface.commit();
            }
        }
    }
}

struct WindowInfo {
    window: Window,
    width: Option<NonZero<u32>>,
    height: Option<NonZero<u32>>,
    started: bool,
}

impl WindowInfo {
    fn new(window: Window) -> Self {
        Self {
            window,
            width: None,
            height: None,
            started: false,
        }
    }
}

struct LayerSurfaceInfo {
    layer_surface: LayerSurface,
    width: Option<NonZero<u32>>,
    height: Option<NonZero<u32>>,
    started: bool,
}

impl LayerSurfaceInfo {
    fn new(layer_surface: LayerSurface) -> Self {
        Self {
            layer_surface,
            width: None,
            height: None,
            started: false,
        }
    }
}

enum WaylandEvent {
    NewOutput(WlOutput),
    SurfaceConfiure {
        wl_surface: WlSurface,
        width: NonZero<u32>,
        height: NonZero<u32>,
    },
    SurfaceFrame(WlSurface),
    SurfaceClose(WlSurface),
    PointerEvents(Vec<PointerEvent>),
}

smithay_client_toolkit::delegate_registry!(WaylandClient);
smithay_client_toolkit::delegate_dispatch2!(WaylandClient);

impl ProvidesRegistryState for WaylandClient {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers![OutputState, SeatState];
}

impl OutputHandler for WaylandClient {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        let _ = futures::executor::block_on(self.event_tx.send(WaylandEvent::NewOutput(output)));
    }

    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}

    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {
    }
}

impl CompositorHandler for WaylandClient {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _new_transform: Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &WlSurface,
        _time: u32,
    ) {
        let _ = futures::executor::block_on(
            self.event_tx
                .send(WaylandEvent::SurfaceFrame(surface.clone())),
        );
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &WlSurface,
        _output: &WlOutput,
    ) {
    }
}

impl WindowHandler for WaylandClient {
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, window: &Window) {
        let _ = futures::executor::block_on(
            self.event_tx
                .send(WaylandEvent::SurfaceClose(window.wl_surface().clone())),
        );
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let Some(Surface::Window(window_info)) = self.surfaces.get_mut(window.wl_surface()) else {
            return;
        };

        window_info.width = configure.new_size.0;
        window_info.height = configure.new_size.1;

        if let (Some(width), Some(height)) = configure.new_size {
            let _ =
                futures::executor::block_on(self.event_tx.send(WaylandEvent::SurfaceConfiure {
                    wl_surface: window.wl_surface().clone(),
                    width,
                    height,
                }));
        }
    }
}

impl LayerShellHandler for WaylandClient {
    fn closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &wlr_layer::LayerSurface,
    ) {
        let _ = futures::executor::block_on(
            self.event_tx
                .send(WaylandEvent::SurfaceClose(layer.wl_surface().clone())),
        );
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &wlr_layer::LayerSurface,
        configure: wlr_layer::LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let Some(Surface::LayerSurface(layer_surface_info)) =
            self.surfaces.get_mut(layer.wl_surface())
        else {
            return;
        };

        let width = NonZero::new(configure.new_size.0);
        let height = NonZero::new(configure.new_size.1);

        layer_surface_info.width = width;
        layer_surface_info.height = height;

        if let (Some(width), Some(height)) = (width, height) {
            let _ =
                futures::executor::block_on(self.event_tx.send(WaylandEvent::SurfaceConfiure {
                    wl_surface: layer.wl_surface().clone(),
                    width,
                    height,
                }));
        }
    }
}

impl SeatHandler for WaylandClient {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}
}

impl PointerHandler for WaylandClient {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &WlPointer,
        events: &[PointerEvent],
    ) {
        let _ = futures::executor::block_on(
            self.event_tx
                .send(WaylandEvent::PointerEvents(events.to_vec())),
        );
    }
}

impl KeyboardHandler for WaylandClient {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _surface: &WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[seat::keyboard::Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _surface: &WlSurface,
        _serial: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _serial: u32,
        _event: seat::keyboard::KeyEvent,
    ) {
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _serial: u32,
        _event: seat::keyboard::KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _serial: u32,
        _event: seat::keyboard::KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &WlKeyboard,
        _serial: u32,
        _modifiers: seat::keyboard::Modifiers,
        _raw_modifiers: seat::keyboard::RawModifiers,
        _layout: u32,
    ) {
    }
}

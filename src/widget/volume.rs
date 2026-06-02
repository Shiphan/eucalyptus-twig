use std::{cell::RefCell, collections::HashMap, rc::Rc, thread};

use futures::{
    StreamExt,
    channel::mpsc::{self, UnboundedSender},
};
use gpui::{
    AsyncApp, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
    Styled, WeakEntity, Window, div, rems,
};
use pipewire::{
    context::ContextRc,
    device::{Device, DeviceChangeMask, DeviceInfoRef, DeviceListener},
    keys::{DEVICE_ID, MEDIA_CLASS, NODE_NAME},
    main_loop::MainLoopRc,
    metadata::{Metadata, MetadataListener},
    node::{Node, NodeChangeMask, NodeInfoRef, NodeListener},
    spa::{
        param::ParamType,
        pod::{Pod, deserialize::PodDeserializer},
        sys::{
            SPA_PARAM_ROUTE_device, SPA_PARAM_ROUTE_props, SPA_PROP_channelVolumes, SPA_PROP_mute,
        },
        utils::Id,
    },
    types::ObjectType,
};
use serde::Deserialize;
use smol::process::Command;

use crate::widget::{Widget, widget_wrapper};

pub struct Volume {
    error_message: Option<String>,
    volume: Option<f32>,
    mute: Option<bool>,
    config: VolumeConfig,
}

impl Widget for Volume {
    type Config = VolumeConfig;

    fn new(cx: &mut gpui::Context<Self>, config: &Self::Config) -> Self {
        cx.spawn(task).detach();

        Self {
            error_message: None,
            volume: None,
            mute: None,
            config: config.clone(),
        }
    }
}

impl Render for Volume {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let widget = if let Some(e) = &self.error_message {
            widget_wrapper().child(e.clone())
        } else if self.mute == Some(true) {
            widget_wrapper()
                .font_family("Material Symbols Rounded")
                .child("\u{e04f}")
        } else if let Some(volume) = self.volume {
            let volume = volume.cbrt() * 100.0;
            widget_wrapper()
                .flex()
                .gap(rems(0.25))
                .child(
                    div()
                        .font_family("Material Symbols Rounded")
                        .child(if volume <= 0.0 {
                            "\u{e04e}"
                        } else if volume < 50.0 {
                            "\u{e04d}"
                        } else {
                            "\u{e050}"
                        }),
                )
                .child(format!("{:.0}", volume))
        } else {
            widget_wrapper().child("?")
        };

        if let [program, args @ ..] = self.config.settings_command.as_slice() {
            let program = program.clone();
            let args = Box::<[_]>::from(args);
            widget
                .id("volume")
                .on_click(
                    move |_, _, cx| match Command::new(&program).args(&args).spawn() {
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
                    },
                )
                .into_any_element()
        } else {
            widget.into_any_element()
        }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeConfig {
    pub settings_command: Vec<String>,
}

async fn task(this: WeakEntity<Volume>, cx: &mut AsyncApp) {
    let (tx, mut rx) = mpsc::unbounded();
    thread::spawn(move || pipewire_thread(tx));
    while let Some(update) = rx.next().await {
        let _ = this.update(cx, |this, cx| {
            match update {
                Update::Volume(volume) => {
                    this.volume = volume;
                }
                Update::Mute(mute) => {
                    this.mute = mute;
                }
                Update::VolumeAndMute(volume, mute) => {
                    this.volume = volume;
                    this.mute = mute;
                }
                Update::ErrorMessage(e) => {
                    this.error_message = Some(e);
                }
            }
            cx.notify();
        });
    }
    tracing::warn!("No more update from pipewire");
}

enum Update {
    Volume(Option<f32>),
    Mute(Option<bool>),
    VolumeAndMute(Option<f32>, Option<bool>),
    ErrorMessage(String),
}

impl Update {
    fn send(self, context: &Context) {
        if let Err(e) = context.tx.unbounded_send(self) {
            tracing::error!(error = %e, "Failed to send update to ui thread");
            context.main_loop.quit();
        }
    }
}

fn pipewire_thread(tx: UnboundedSender<Update>) {
    tracing::trace!("pipewire_thread called");

    let main_loop = match MainLoopRc::new(None) {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(
                error = %e,
                "Failed to get PipeWire main loop"
            );
            if let Err(e) = tx.unbounded_send(Update::ErrorMessage(format!(
                "Failed to get PipeWire main loop: {e}"
            ))) {
                tracing::error!(error = %e, "Failed to send update to ui thread");
            }
            return;
        }
    };
    let pipewire_context = match ContextRc::new(&main_loop, None) {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(
                error = %e,
                "Failed to get PipeWire context"
            );
            if let Err(e) = tx.unbounded_send(Update::ErrorMessage(format!(
                "Failed to get PipeWire context: {e}"
            ))) {
                tracing::error!(error = %e, "Failed to send update to ui thread");
            }
            return;
        }
    };
    let core = match pipewire_context.connect_rc(None) {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(
                error = %e,
                "Failed to get PipeWire core"
            );
            if let Err(e) = tx.unbounded_send(Update::ErrorMessage(format!(
                "Failed to get PipeWire core: {e}"
            ))) {
                tracing::error!(error = %e, "Failed to send update to ui thread");
            }
            return;
        }
    };
    let registry = match core.get_registry_rc() {
        Ok(x) => x,
        Err(e) => {
            tracing::error!(
                error = %e,
                "Failed to get PipeWire registry"
            );
            if let Err(e) = tx.unbounded_send(Update::ErrorMessage(format!(
                "Failed to get PipeWire registry: {e}"
            ))) {
                tracing::error!(error = %e, "Failed to send update to ui thread");
            }
            return;
        }
    };

    let context = Rc::new(RefCell::new(Context {
        metadata_default_listener: None,
        node_audio_sink_listeners: HashMap::new(),
        device_listeners: HashMap::new(),
        default_sink_name: None,
        audio_sinks: HashMap::new(),
        waiting_device: HashMap::new(),
        tx,
        main_loop: main_loop.clone(),
    }));

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let registry = registry.clone();
            let context = context.clone();
            move |global| match global.type_ {
                ObjectType::Node
                    if global.props.and_then(|x| x.get(&MEDIA_CLASS)) == Some("Audio/Sink") =>
                {
                    let Some(node_name) = global.props.and_then(|x| x.get(&NODE_NAME)).map(|x| x.to_owned()) else {
                        tracing::warn!(
                            global.id, ?global.props,
                            "Got a node without a name"
                        );
                        return;
                    };
                    let node = match registry.bind::<Node, _>(global){
                        Ok(x) => x,
                        Err(e) => {
                            tracing::error!(error = %e, "Got a node object but failed to convert it to a real node");
                            return;
                        }
                    };
                    tracing::info!(node_name, "Got a node");
                    let listener = node
                        .add_listener_local()
                        .info({
                            let node_name = node_name.clone();
                            let context = context.clone();
                            move |info| {
                                node_info_listener(info, &node_name, &context);
                            }
                        })
                        .param({
                            let context = context.clone();
                            move |seq, id, index, next, param| {
                                node_param_listener(seq, id, index, next, param, &node_name, &mut context.borrow_mut());
                            }
                        })
                        .register();
                    node.subscribe_params(&[ParamType::Props]);

                    if let Some((node, _listener)) = context.borrow_mut().node_audio_sink_listeners.insert(global.id, (node, listener)) {
                        tracing::warn!(?node, "new audio sink listener that replace an old one with sane id");
                    }
                    tracing::info!(audio_sink_listeners_len = context.borrow().node_audio_sink_listeners.len());
                }
                ObjectType::Metadata
                    if global.props.and_then(|x| x.get("metadata.name")) == Some("default") =>
                {
                    let metadata = match registry.bind::<Metadata, _>(global) {
                        Ok(x) => x,
                        Err(e) => {
                            tracing::error!(error = %e, "Got a Metadata object but failed to convert it to a real Metadata");
                            return;
                        }
                    };
                    let listener = metadata
                        .add_listener_local()
                        .property({
                            let context = context.clone();
                            move |subject, key, type_, value| {
                                // TODO: what is this subject parameter
                                metadata_listener(subject, key, type_, value, &mut context.borrow_mut())
                            }
                        })
                        .register();

                    if let Some((old_metadata, _listener)) = context.borrow_mut().metadata_default_listener.replace((metadata, listener)) {
                        tracing::info!(?old_metadata, "Replacing old metadata listener");
                    }
                    tracing::info!("Create default listener");
                }
                ObjectType::Device => {
                    let device = match registry.bind::<Device, _>(global) {
                        Ok(x) => x,
                        Err(e) => {
                            tracing::error!(error = %e, "Got a Device object but failed to convert it to a real Device");
                            return;
                        }
                    };
                    if let Some((node_name, card_profile_device)) = context.borrow_mut().waiting_device.remove(&global.id) {
                        tracing::warn!("waiting_device is been used");
                        let listener = device
                            .add_listener_local()
                            .info({
                                let context = context.clone();
                                let device_id = global.id;
                                move |info| {
                                    device_info_listener(info, device_id, &context.borrow());
                                }
                            })
                            .param({
                                let context = context.clone();
                                move |seq, id, index, next, param| {
                                    device_param_listener(
                                        seq,
                                        id,
                                        index,
                                        next,
                                        param,
                                        &node_name,
                                        card_profile_device,
                                        &mut context.borrow_mut(),
                                    );
                                }
                            })
                            .register();
                        device.subscribe_params(&[ParamType::Route]);
                        context.borrow_mut().device_listeners.insert(global.id, (device, Some((card_profile_device, listener))));
                    } else {
                        context.borrow_mut().device_listeners.insert(global.id, (device, None));
                    }
                    tracing::debug!(devices = ?context.borrow().device_listeners.iter().map(|(id, (device, listener))| (id, device, listener.is_some())).collect::<Vec<_>>());
                }
                _ => (),
            }
        })
        .global_remove({
            let context = context.clone();
            move |id| {
                if let Some((node, _listener)) = context.borrow_mut().node_audio_sink_listeners.remove(&id) {
                    tracing::info!(id, ?node, "Remove one from audio_sink_listeners");
                }
                if let Some((device, _listener)) = context.borrow_mut().device_listeners.remove(&id) {
                    tracing::info!(id, ?device, "Remove one from devices");
                }
            }
        })
        .register();

    let _context = context;

    main_loop.run();

    tracing::warn!("pipewire main loop end");
}

struct Context {
    metadata_default_listener: Option<(Metadata, MetadataListener)>,
    node_audio_sink_listeners: HashMap<u32, (Node, NodeListener)>,
    device_listeners: HashMap<u32, (Device, Option<(i32, DeviceListener)>)>,
    default_sink_name: Option<String>,
    audio_sinks: HashMap<String, AudioSinkInfo>,
    // TODO: Read the pipewire documentation to see if it's save to remove waiting_device
    // (if the device object is always ready before any node needs it)
    waiting_device: HashMap<u32, (String, i32)>,
    tx: UnboundedSender<Update>,
    main_loop: MainLoopRc,
}

#[derive(Default)]
struct AudioSinkInfo {
    use_device: bool,
    device_volume: Option<f32>,
    device_mute: Option<bool>,
    node_volume: Option<f32>,
    node_mute: Option<bool>,
}

impl AudioSinkInfo {
    fn get(&self) -> (Option<f32>, Option<bool>) {
        if self.use_device {
            (self.device_volume, self.device_mute)
        } else {
            (self.node_volume, self.node_mute)
        }
    }
}

fn node_info_listener(info: &NodeInfoRef, node_name: &String, context: &Rc<RefCell<Context>>) {
    if !info.change_mask().contains(NodeChangeMask::PROPS) {
        return;
    }

    let device_id = match info
        .props()
        .and_then(|x| x.get(&DEVICE_ID))
        .map(|x| x.parse::<u32>())
    {
        Some(Ok(x)) => x,
        Some(Err(e)) => {
            tracing::error!(error = %e, "Failed to parse device.id as u32");
            set_not_use_device(node_name, &mut context.borrow_mut());
            return;
        }
        None => {
            tracing::info!(node_name, "There is no {} for this node", *DEVICE_ID);
            set_not_use_device(node_name, &mut context.borrow_mut());
            return;
        }
    };
    let card_profile_device = match info
        .props()
        .and_then(|x| x.get("card.profile.device"))
        .map(|x| x.parse::<i32>())
    {
        Some(Ok(x)) => x,
        Some(Err(e)) => {
            tracing::error!(error = %e, "Failed to parse card.profile.device as i32");
            set_not_use_device(node_name, &mut context.borrow_mut());
            return;
        }
        None => {
            tracing::info!(node_name, "There is no card.profile.device for this node");
            set_not_use_device(node_name, &mut context.borrow_mut());
            return;
        }
    };

    tracing::info!(
        node_name,
        device_id,
        card_profile_device,
        "Creating device_param_listener for this Audio/Sink"
    );
    let mut context_borrowed = context.borrow_mut();
    let (device, listener) = match context_borrowed.device_listeners.get_mut(&device_id) {
        Some(x) => x,
        None => {
            tracing::warn!(
                "Should use device's volume but failed to get device object, put node to waiting_device"
            );
            if let Some((old_node_name, old_card_profile_device)) = context_borrowed
                .waiting_device
                .insert(device_id, (node_name.clone(), card_profile_device))
            {
                tracing::warn!(
                    old_node_name,
                    old_card_profile_device,
                    "Replace wait device"
                );
            }
            set_not_use_device(node_name, &mut context_borrowed);
            return;
        }
    };
    if let Some((old_card_profile_device, _)) = listener
        && card_profile_device == *old_card_profile_device
    {
        return;
    }
    let old_listener = listener.replace((
        card_profile_device,
        device
            .add_listener_local()
            // TODO: this info listener is needed because for reasons that i don't understand,
            // update of route will not automatically send to param listener,
            // need to call enum_params first
            .info({
                let context = context.clone();
                move |info| {
                    device_info_listener(info, device_id, &context.borrow());
                }
            })
            .param({
                let node_name = node_name.clone();
                let context = context.clone();
                move |seq, id, index, next, param| {
                    device_param_listener(
                        seq,
                        id,
                        index,
                        next,
                        param,
                        &node_name,
                        card_profile_device,
                        &mut context.borrow_mut(),
                    );
                }
            })
            .register(),
    ));
    device.subscribe_params(&[ParamType::Route]);
    if let Some(_listener) = old_listener {
        tracing::warn!(
            "Creating a new device_param_listener while there is already an old listener"
        );
    }
    tracing::debug!(devices = ?context_borrowed.device_listeners.iter().map(|(id, (device, listener))| (id, device, listener.is_some())).collect::<Vec<_>>());
}

fn set_not_use_device(node_name: &String, context: &mut Context) {
    if let Some(audio_sink_info) = context.audio_sinks.get_mut(node_name) {
        let was_using_device = audio_sink_info.use_device;
        audio_sink_info.use_device = false;
        if was_using_device && context.default_sink_name.as_ref() == Some(node_name) {
            let (volume, mute) = audio_sink_info.get();
            Update::VolumeAndMute(volume, mute).send(context);
        }
    }
}

fn device_info_listener(info: &DeviceInfoRef, id: u32, context: &Context) {
    if !info.change_mask().contains(DeviceChangeMask::PARAMS) {
        return;
    }
    for param in info.params() {
        if param.id() == ParamType::Route {
            match context.device_listeners.get(&id) {
                Some((device, _)) => {
                    tracing::info!("Should update route, calling enum_params");
                    device.enum_params(0, Some(ParamType::Route), 0, u32::MAX);
                }
                None => {
                    tracing::error!("Error no such device");
                }
            }
            tracing::debug!(?param, type_ = ?param.id(), "device info");
        }
    }
}

fn device_param_listener(
    seq: i32,
    id: ParamType,
    index: u32,
    next: u32,
    param: Option<&Pod>,
    node_name: &String,
    card_profile_device: i32,
    context: &mut Context,
) {
    tracing::info!(
        seq, index, next, param = ?param.map(|x| x.type_()),
        "Device param listener"
    );
    match id {
        ParamType::Route => {
            if let Some(pod) = param {
                let object = match pod.as_object() {
                    Ok(x) => x,
                    Err(e) => {
                        tracing::warn!(error = %e, pod_type = ?pod.type_(), "Device update sends a pod that is not an object");
                        return;
                    }
                };
                if let Some(prop) = object.find_prop(Id(SPA_PARAM_ROUTE_device)) {
                    match prop.value().get_int() {
                        Ok(device) => {
                            if device != card_profile_device {
                                tracing::debug!(
                                    node_name,
                                    device,
                                    "Ignoring this route (not matching card.profile.device)"
                                );
                                return;
                            }
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to parse SPA_PARAM_ROUTE_device as i32");
                            return;
                        }
                    }
                }

                let props = match object
                    .find_prop(Id(SPA_PARAM_ROUTE_props))
                    .map(|x| x.value().as_object())
                {
                    Some(Ok(x)) => x,
                    Some(Err(e)) => {
                        tracing::error!(error = %e, "Route's SPA_PARAM_ROUTE_props is not and object");
                        return;
                    }
                    None => {
                        tracing::error!("Route has no SPA_PARAM_ROUTE_props");
                        return;
                    }
                };

                let volume = if let Some(prop) = props.find_prop(Id(SPA_PROP_channelVolumes)) {
                    match PodDeserializer::deserialize_from::<Vec<f32>>(prop.value().as_bytes()) {
                        Ok(([], channel_volumes)) => {
                            tracing::debug!(node_name, SPA_PROP_channelVolumes = ?channel_volumes);
                            let volume = channel_volumes.into_iter().reduce(f32::max);
                            if context.default_sink_name.as_ref() == Some(node_name) {
                                Update::Volume(volume).send(context);
                            }
                            context
                                .audio_sinks
                                .entry(node_name.clone())
                                .and_modify(
                                    |AudioSinkInfo {
                                         use_device,
                                         device_volume,
                                         ..
                                     }| {
                                        *use_device = true;
                                        *device_volume = volume;
                                    },
                                )
                                .or_insert(AudioSinkInfo {
                                    use_device: true,
                                    device_volume: volume,
                                    ..Default::default()
                                });
                            volume
                        }
                        Ok((remain, _)) => {
                            tracing::error!(
                                "Failed to parse SPA_PROP_channelVolumes as array of f32: {} bytes left",
                                remain.len()
                            );
                            None
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to parse SPA_PROP_channelVolumes as array of f32");
                            None
                        }
                    }
                } else {
                    None
                };
                let mute = if let Some(prop) = props.find_prop(Id(SPA_PROP_mute)) {
                    match prop.value().get_bool() {
                        Ok(mute) => {
                            tracing::debug!(node_name, SPA_PROP_mute = mute);
                            if Some(node_name) == context.default_sink_name.as_ref() {
                                Update::Mute(Some(mute)).send(context);
                            }
                            context
                                .audio_sinks
                                .entry(node_name.clone())
                                .and_modify(
                                    |AudioSinkInfo {
                                         use_device,
                                         device_mute,
                                         ..
                                     }| {
                                        *use_device = true;
                                        *device_mute = Some(mute);
                                    },
                                )
                                .or_insert(AudioSinkInfo {
                                    use_device: true,
                                    device_mute: Some(mute),
                                    ..Default::default()
                                });
                            Some(mute)
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to parse SPA_PROP_mute as bool");
                            None
                        }
                    }
                } else {
                    None
                };
                tracing::info!(node_name, volume, mute);
            }
        }
        _ => (),
    }
}

// see `collect_node_info` function in WirePlumber: <https://gitlab.freedesktop.org/pipewire/wireplumber/-/blob/master/modules/module-mixer-api.c>
// TODO: check `wp_mixer_api_get_volume` function to see if the volume info is handled correctly: <https://gitlab.freedesktop.org/pipewire/wireplumber/-/blob/master/src/tools/wpctl.c>
fn node_param_listener(
    seq: i32,
    id: ParamType,
    index: u32,
    next: u32,
    param: Option<&Pod>,
    node_name: &String,
    context: &mut Context,
) {
    match id {
        ParamType::Props => {
            tracing::debug!(
                seq, index, next, param = ?param.map(|x| x.type_()),
                "Node listener (Props)",
            );
            if let Some(pod) = param {
                let object = match pod.as_object() {
                    Ok(x) => x,
                    Err(e) => {
                        tracing::warn!(error = %e, pod_type = ?pod.type_(), "Node update sends a pod that is not an object");
                        return;
                    }
                };
                if let Some(prop) = object.find_prop(Id(pipewire::spa::sys::SPA_PROP_volume)) {
                    tracing::info!(node_name, SPA_PROP_volume = ?prop.value().get_float());
                }
                let volume = if let Some(prop) = object.find_prop(Id(SPA_PROP_channelVolumes)) {
                    match PodDeserializer::deserialize_from::<Vec<f32>>(prop.value().as_bytes()) {
                        Ok(([], channel_volumes)) => {
                            tracing::debug!(node_name, SPA_PROP_channelVolumes = ?channel_volumes);
                            let volume = channel_volumes.into_iter().reduce(f32::max);
                            if Some(node_name) == context.default_sink_name.as_ref()
                                && !matches!(context.audio_sinks.get(node_name), Some(audio_sink) if audio_sink.use_device)
                            {
                                Update::Volume(volume).send(context);
                            }
                            context
                                .audio_sinks
                                .entry(node_name.clone())
                                .and_modify(|AudioSinkInfo { node_volume, .. }| {
                                    *node_volume = volume;
                                })
                                .or_insert(AudioSinkInfo {
                                    node_volume: volume,
                                    ..Default::default()
                                });
                            volume
                        }
                        Ok((remain, _)) => {
                            tracing::error!(
                                "Failed to parse SPA_PROP_channelVolumes as array of f32: {} bytes left",
                                remain.len()
                            );
                            None
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to parse SPA_PROP_channelVolumes as array of f32");
                            None
                        }
                    }
                } else {
                    None
                };
                let mute = if let Some(prop) = object.find_prop(Id(SPA_PROP_mute)) {
                    match prop.value().get_bool() {
                        Ok(mute) => {
                            tracing::debug!(node_name, SPA_PROP_mute = mute);
                            if Some(node_name) == context.default_sink_name.as_ref()
                                && !matches!(context.audio_sinks.get(node_name), Some(audio_sink) if audio_sink.use_device)
                            {
                                Update::Mute(Some(mute)).send(context);
                            }
                            context
                                .audio_sinks
                                .entry(node_name.clone())
                                .and_modify(|AudioSinkInfo { node_mute, .. }| {
                                    *node_mute = Some(mute);
                                })
                                .or_insert(AudioSinkInfo {
                                    node_mute: Some(mute),
                                    ..Default::default()
                                });
                            Some(mute)
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to parse SPA_PROP_mute as bool");
                            None
                        }
                    }
                } else {
                    None
                };
                tracing::info!(node_name, volume, mute);
            }
        }
        _ => {
            tracing::trace!(
                seq, index, next, param = ?param.map(|x| x.type_()),
                "Node listener"
            );
        }
    }
}

fn metadata_listener(
    subject: u32,
    key: Option<&str>,
    type_: Option<&str>,
    value: Option<&str>,
    context: &mut Context,
) -> i32 {
    tracing::debug!(subject, key, type_, value, "Metadata listener");
    match (key, type_, value) {
        (Some("default.audio.sink"), Some("Spa:String:JSON"), Some(value)) => {
            match serde_json::from_str::<DefaultAudioSink>(value) {
                Ok(value) => {
                    tracing::info!(new = value.name, "Update default sink");
                    let (volume, mute) = match context.audio_sinks.get(&value.name) {
                        Some(x) => x.get(),
                        None => (None, None),
                    };
                    Update::VolumeAndMute(volume, mute).send(context);
                    if let Some(old_default_sink_name) =
                        context.default_sink_name.replace(value.name)
                    {
                        tracing::info!(old_default_sink_name);
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Got an update for default.audio.sink with type json, but failed to parse it");
                }
            }
        }
        (Some("default.audio.sink"), _, None) | (None, _, _) => {
            tracing::info!(key, value, "Remove default.audio.sink property");
            if let Some(old_default_sink_name) = context.default_sink_name.take() {
                tracing::info!(old_default_sink_name);
            }
        }
        (Some("default.audio.sink"), _, _) => {
            tracing::warn!(
                type_,
                value,
                "Got an update for default.audio.sink, but with unexpected type or value"
            );
        }
        _ => (),
    }
    0 // TODO: what is this return value?
}

#[derive(Deserialize)]
struct DefaultAudioSink {
    name: String,
}

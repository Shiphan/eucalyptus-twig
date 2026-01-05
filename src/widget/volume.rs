use std::{cell::RefCell, collections::HashMap, rc::Rc, thread};

use futures::{
    StreamExt,
    channel::mpsc::{self, UnboundedSender},
};
use gpui::{
    AsyncApp, Context, IntoElement, ParentElement, Render, Styled, WeakEntity, Window, div, rems,
};
use pipewire::{
    context::ContextRc,
    main_loop::MainLoopRc,
    metadata::Metadata,
    node::Node,
    proxy::{Listener, ProxyT},
    spa::{
        param::ParamType,
        pod::{Pod, deserialize::PodDeserializer},
        sys::{SPA_PROP_channelVolumes, SPA_PROP_mute},
        utils::Id,
    },
    types::ObjectType,
};
use serde::Deserialize;

use crate::widget::{Widget, widget_wrapper};

pub struct Volume {
    error_message: Option<String>,
    mute: Option<bool>,
    volume: Option<f32>,
}

impl Widget for Volume {
    fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(task).detach();

        Self {
            error_message: None,
            mute: None,
            volume: None,
        }
    }
}

impl Render for Volume {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            widget_wrapper().child(e.clone())
        } else if self.mute == Some(true) {
            widget_wrapper()
                .font_family("Material Symbols Rounded")
                .child("󰖁")
        } else {
            if let Some(volume) = self.volume {
                let volume = volume.cbrt() * 100.0;
                widget_wrapper()
                    .flex()
                    .gap(rems(0.25))
                    .child(
                        div()
                            .font_family("Material Symbols Rounded")
                            .child(if volume <= 0.0 {
                                "󰕿"
                            } else if volume < 50.0 {
                                "󰖀"
                            } else {
                                "󰕾"
                            }),
                    )
                    .child(format!("{:.1}", volume))
            } else {
                widget_wrapper().child("?")
            }
        }
    }
}

async fn task(this: WeakEntity<Volume>, cx: &mut AsyncApp) {
    let (tx, mut rx) = mpsc::unbounded();
    thread::spawn(move || pipewire_thread(tx));
    while let Some(update) = rx.next().await {
        match update {
            Update::Volume(volume) => {
                let _ = this.update(cx, |this, cx| {
                    this.volume = volume;
                    cx.notify()
                });
            }
            Update::Mute(mute) => {
                let _ = this.update(cx, |this, cx| {
                    this.mute = mute;
                    cx.notify()
                });
            }
            Update::ErrorMessage(e) => {
                let _ = this.update(cx, |this, cx| {
                    this.error_message = Some(e);
                    cx.notify()
                });
            }
        }
    }
    tracing::warn!("No more update from pipewire");
}

enum Update {
    Volume(Option<f32>),
    Mute(Option<bool>),
    ErrorMessage(String),
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
    let context = match ContextRc::new(&main_loop, None) {
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
    let core = match context.connect_rc(None) {
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

    let listeners = Rc::new(RefCell::new(
        Vec::<(Box<dyn ProxyT>, Box<dyn Listener>)>::new(),
    ));
    let volumes = Rc::new(RefCell::new(
        HashMap::<String, (Option<bool>, Option<f32>)>::new(),
    ));
    let default_sink_name = Rc::new(RefCell::new(None::<String>));

    let _registry_listener = registry
        .add_listener_local()
        .global({
            let registry = registry.clone();
            let main_loop = main_loop.clone();
            move |global| match global.type_ {
                ObjectType::Node
                    if global.props.and_then(|x| x.get("media.class")) == Some("Audio/Sink") =>
                {
                    let Some(node_name) = global.props.and_then(|x| x.get("node.name")).map(|x| x.to_owned()) else {
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
                        .param({
                            let volumes = volumes.clone();
                            let default_sink_name = default_sink_name.clone();
                            let tx = tx.clone();
                            let main_loop = main_loop.clone();
                            move |seq, id, index, next, param| {
                                node_listener(seq, id, index, next, param, &node_name, &tx, &volumes, &default_sink_name, &main_loop);
                            }
                        })
                        .register();
                    node.subscribe_params(&[ParamType::Props]);

                    listeners.borrow_mut().push((Box::new(node), Box::new(listener)));
                    tracing::info!(listeners_count = listeners.borrow().len());
                }
                ObjectType::Metadata
                    if global.props.and_then(|x| x.get("metadata.name")) == Some("default") =>
                {
                    let metadata = match registry.bind::<Metadata, _>(global) {
                        Ok(x) => x,
                        Err(e) => {
                            tracing::error!(error = %e, "Got a Metadata object but failed to convert it to a real Metadate");
                            return;
                        }
                    };
                    let listener = metadata
                        .add_listener_local()
                        .property({
                            let default_sink_name = default_sink_name.clone();
                            let tx = tx.clone();
                            let volumes = volumes.clone();
                            let main_loop = main_loop.clone();
                            move |subject, key, type_, value| {
                                // TODO: what is this subject parameter
                                metadata_listener(subject, key, type_, value, &tx, &volumes, &default_sink_name, &main_loop)
                            }
                        })
                        .register();

                    listeners.borrow_mut().push((Box::new(metadata), Box::new(listener)));
                    tracing::info!(listeners_count = listeners.borrow().len());
                }
                _ => (),
            }
        })
        .register();

    main_loop.run();

    tracing::warn!("pipewire main loop end");
}

fn node_listener(
    seq: i32,
    id: ParamType,
    index: u32,
    next: u32,
    param: Option<&Pod>,
    node_name: &String,
    tx: &UnboundedSender<Update>,
    volumes: &Rc<RefCell<HashMap<String, (Option<bool>, Option<f32>)>>>,
    default_sink_name: &Rc<RefCell<Option<String>>>,
    main_loop: &MainLoopRc,
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
                if let Some(prop) = object.find_prop(Id(SPA_PROP_channelVolumes)) {
                    match PodDeserializer::deserialize_from::<Vec<f32>>(prop.value().as_bytes()) {
                        Ok(([], channel_volumes)) => {
                            tracing::info!(node_name, SPA_PROP_channelVolumes = ?channel_volumes);
                            let volume = channel_volumes.into_iter().reduce(f32::max);
                            if Some(node_name) == default_sink_name.borrow().as_ref() {
                                if let Err(e) = tx.unbounded_send(Update::Volume(volume)) {
                                    tracing::warn!(error = %e, "Failed to send update to ui thread");
                                    main_loop.quit();
                                }
                            }
                            volumes
                                .borrow_mut()
                                .entry(node_name.clone())
                                .and_modify(|(_, x)| {
                                    *x = volume;
                                })
                                .or_insert((None, volume));
                        }
                        Ok((remain, _)) => {
                            tracing::error!(
                                "Failed to parse SPA_PROP_channelVolumes as array of f32: {} bytes left",
                                remain.len()
                            );
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to parse SPA_PROP_channelVolumes as array of f32");
                        }
                    }
                }
                if let Some(prop) = object.find_prop(Id(SPA_PROP_mute)) {
                    match prop.value().get_bool() {
                        Ok(mute) => {
                            tracing::info!(node_name, SPA_PROP_mute = mute);
                            if Some(node_name) == default_sink_name.borrow().as_ref() {
                                if let Err(e) = tx.unbounded_send(Update::Mute(Some(mute))) {
                                    tracing::warn!(error = %e, "Failed to send update to ui thread");
                                    main_loop.quit();
                                }
                            }
                            volumes
                                .borrow_mut()
                                .entry(node_name.clone())
                                .and_modify(|(x, _)| {
                                    *x = Some(mute);
                                })
                                .or_insert((Some(mute), None));
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "Failed to parse SPA_PROP_mute as bool");
                        }
                    }
                }
            }
        }
        _ => {
            tracing::trace!(
                seq, index, next, param = ?param.map(|x| x.type_()),
                "Node listener"
            )
        }
    }
}
fn metadata_listener(
    subject: u32,
    key: Option<&str>,
    type_: Option<&str>,
    value: Option<&str>,
    tx: &UnboundedSender<Update>,
    volumes: &Rc<RefCell<HashMap<String, (Option<bool>, Option<f32>)>>>,
    default_sink_name: &Rc<RefCell<Option<String>>>,
    main_loop: &MainLoopRc,
) -> i32 {
    tracing::debug!(subject, key, type_, value, "Metadata listener");
    match (key, type_, value) {
        (Some("default.audio.sink"), Some("Spa:String:JSON"), Some(value)) => {
            match serde_json::from_str::<DefaultAudioSink>(value) {
                Ok(value) => {
                    tracing::info!(new = value.name, "Update default sink");
                    let (mute, volume) = volumes
                        .borrow()
                        .get(&value.name)
                        .copied()
                        .unwrap_or((None, None));
                    if let Err(e) = tx.unbounded_send(Update::Mute(mute)) {
                        tracing::warn!(error = %e, "Failed to send update to ui thread");
                        main_loop.quit();
                    }
                    if let Err(e) = tx.unbounded_send(Update::Volume(volume)) {
                        tracing::warn!(error = %e, "Failed to send update to ui thread");
                        main_loop.quit();
                    }
                    *default_sink_name.borrow_mut() = Some(value.name);
                }
                Err(e) => {
                    tracing::error!(error = %e, "Got an update for default.audio.sink with type json, but failed to parse it");
                }
            }
        }
        (Some("default.audio.sink"), _, None) | (None, _, _) => {
            tracing::info!(key, value, "Remove default.audio.sink property");
            *default_sink_name.borrow_mut() = None;
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

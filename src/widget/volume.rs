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
        pod::deserialize::PodDeserializer,
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
                widget_wrapper()
                    .flex()
                    .gap(rems(0.25))
                    .child(div().font_family("Material Symbols Rounded").child("󰖀"))
                    .child(format!("{:.1}", volume.cbrt() * 100.0))
                // "󰕿 "
                // "󰖀 "
                // "󰕾 "
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
    println!("exit...");
}

enum Update {
    Volume(Option<f32>),
    Mute(Option<bool>),
    ErrorMessage(String),
}

fn pipewire_thread(tx: UnboundedSender<Update>) {
    println!("pipewire_thread called");

    let mainloop = match MainLoopRc::new(None) {
        Ok(x) => x,
        Err(e) => {
            tx.unbounded_send(Update::ErrorMessage(format!("{} error: {e}", line!())))
                .unwrap();
            return;
        }
    };
    let context = match ContextRc::new(&mainloop, None) {
        Ok(x) => x,
        Err(e) => {
            tx.unbounded_send(Update::ErrorMessage(format!("{} error: {e}", line!())))
                .unwrap();
            return;
        }
    };
    let core = match context.connect_rc(None) {
        Ok(x) => x,
        Err(e) => {
            tx.unbounded_send(Update::ErrorMessage(format!("{} error: {e}", line!())))
                .unwrap();
            return;
        }
    };
    let registry = match core.get_registry_rc() {
        Ok(x) => x,
        Err(e) => {
            tx.unbounded_send(Update::ErrorMessage(format!("{} error: {e}", line!())))
                .unwrap();
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
            move |global| match global.type_ {
                ObjectType::Node
                    if global.props.and_then(|x| x.get("media.class")) == Some("Audio/Sink") =>
                {
                    let Some(node_name) = global.props.and_then(|x| x.get("node.name")).map(|x| x.to_owned()) else {
                        println!(
                            "we got a node without a name!!: id = {}, props = {:#?}",
                            global.id, global.props
                        );
                        return;
                    };
                    let node = registry.bind::<Node, _>(global).unwrap();
                    let volumes = volumes.clone();
                    let default_sink_name = default_sink_name.clone();
                    let tx = tx.clone();
                    let listener = node
                        .add_listener_local()
                        .param(move |seq, id, index, next, param| {
                            match id {
                                ParamType::Props => {
                                    if let Some(pod) = param {
                                        match pod.as_object() {
                                            Ok(object) => {
                                                if let Some(prop) = object.find_prop(Id(SPA_PROP_channelVolumes)) {
                                                    match PodDeserializer::deserialize_from::<Vec<f32>>(prop.value().as_bytes()) {
                                                        Ok(([], channel_volumes)) => {
                                                            let volume = channel_volumes.into_iter().reduce(f32::max);
                                                            if Some(&node_name) == default_sink_name.borrow().as_ref() {
                                                                tx.unbounded_send(Update::Volume(volume)).unwrap();
                                                            }
                                                            volumes
                                                                .borrow_mut()
                                                                .entry(node_name.clone())
                                                                .and_modify(|(_, x)| {*x = volume;})
                                                                .or_insert((None, volume));

                                                            println!("SPA_PROP_channelVolumes max: {:?}", volume);
                                                        }
                                                        Ok((_remain, _)) => {
                                                                println!("{} error: not all pod used", line!());
                                                        }
                                                        Err(e) => {
                                                            println!("{} error: {e:?}", line!());
                                                        }
                                                    }
                                                }
                                                if let Some(prop) = object.find_prop(Id(SPA_PROP_mute)) {
                                                    match prop.value().get_bool() {
                                                        Ok(mute) => {
                                                            if Some(&node_name) == default_sink_name.borrow().as_ref() {
                                                                tx.unbounded_send(Update::Mute(Some(mute))).unwrap();
                                                            }
                                                            volumes.borrow_mut().entry(node_name.clone()).and_modify(|(x, _)| {*x = Some(mute);}).or_insert((Some(mute), None));

                                                            println!("SPA_PROP_mute: {}", mute);
                                                        }
                                                        Err(e) => {
                                                            println!("{} error: {e:?}", line!());
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                println!("{} error: {e}", line!());
                                            }
                                        }
                                    }
                                    println!(
                                        "node listener (Props!!!): {:#?}",
                                        (seq, index, next, param.is_some())
                                    )
                                }
                                _ => {
                                    println!(
                                        "node listener: {:#?}",
                                        (seq, id, index, next, param.map(|x| x.as_bytes()))
                                    )
                                }
                            }
                        })
                        .register();
                    node.subscribe_params(&[ParamType::Props]);

                    listeners.borrow_mut().push((Box::new(node), Box::new(listener)));
                    println!("listeners count = {}", listeners.borrow().len());
                }
                ObjectType::Metadata
                    if global.props.and_then(|x| x.get("metadata.name")) == Some("default") =>
                {
                    println!("metadata = {global:#?}");
                    let metadata = registry.bind::<Metadata, _>(global).unwrap();
                    let default_sink_name = default_sink_name.clone();
                    let tx = tx.clone();
                    let volumes = volumes.clone();
                    let listener = metadata
                        .add_listener_local()
                        .property(move |subject, key, type_, value| {
                            match (key, type_, value) {
                                (Some("default.audio.sink"), Some("Spa:String:JSON"), Some(value)) => {
                                    match serde_json::from_str::<DefaultAudioSink>(value) {
                                        Ok(value) => {
                                            println!("metadata listener (update): {:#?}", (subject, &value.name));
                                            let (mute, volume) = volumes.borrow().get(&value.name).copied().unwrap_or((None, None));
                                            tx.unbounded_send(Update::Mute(mute)).unwrap();
                                            tx.unbounded_send(Update::Volume(volume)).unwrap();
                                            *default_sink_name.borrow_mut() = Some(value.name);
                                        }
                                        Err(e) => {
                                            println!("metadata listener (default.audio.sink w/ error): {:#?}", (subject, key, type_, value, e));
                                        }
                                    }
                                }
                                (Some("default.audio.sink"), _, None) | (None, _, _) => {
                                    println!("metadata listener (remove): {:#?}", (subject));
                                    *default_sink_name.borrow_mut() = None;
                                }
                                (Some("default.audio.sink"), _, _) => {
                                    println!("metadata listener (default.audio.sink): {:#?}", (subject, key, type_, value));
                                }
                                _ => {
                                    println!("metadata listener: {:#?}", (subject, key, type_, value));
                                }
                            }
                            0
                        })
                        .register();

                    listeners.borrow_mut().push((Box::new(metadata), Box::new(listener)));
                    println!("listeners count = {}", listeners.borrow().len());
                }
                _ => (),
            }
        })
        .register();

    mainloop.run();

    println!("pipewire mainloop end");
}

#[derive(Deserialize)]
struct DefaultAudioSink {
    name: String,
}

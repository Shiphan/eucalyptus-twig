use std::{ffi::OsStr, os::fd::AsFd, path::PathBuf};

use futures::{AsyncReadExt, AsyncSeekExt, SinkExt, StreamExt};
use gpui::{
    AsyncApp,
    Context,
    IntoElement,
    ParentElement,
    Render,
    Styled,
    WeakEntity,
    Window,
    div,
};
use nix::poll::{PollFd, PollFlags, PollTimeout};
use serde::Deserialize;

pub struct Backlight {
    error_message: Option<String>,
    brightness: Option<i32>,
    max_brightness: Option<i32>,
}

impl Backlight {
    pub fn new(cx: &mut Context<Self>, config: &BacklightConfig) -> Self {
        let name = config.name.clone();
        cx.spawn(async |this, cx| task(this, cx, name).await)
            .detach();

        Self {
            error_message: None,
            brightness: None,
            max_brightness: None,
        }
    }
}

impl Render for Backlight {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(e) = &self.error_message {
            div().child(e.clone())
        } else if let Some(brightness) = self.brightness
            && let Some(max_brightness) = self.max_brightness
        {
            div().flex().child(format!(
                "{:.0}",
                brightness as f64 / max_brightness as f64 * 100.0
            ))
        } else {
            div()
                .flex()
                .child(format!("{:?} / {:?}", self.brightness, self.max_brightness))
        }
    }
}

#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BacklightConfig {
    pub name: Option<String>,
}

async fn task(this: WeakEntity<Backlight>, cx: &mut AsyncApp, name: Option<String>) {
    tracing::info!("Start of a backlight task");

    let backlight_dir = OsStr::new("/sys/class/backlight");
    let name = match name {
        Some(x) => x.into(),
        None => {
            let mut dir = match smol::fs::read_dir(backlight_dir).await {
                Ok(x) => x,
                Err(e) => {
                    tracing::error!(%e, "Failed to open {backlight_dir:?}");
                    return;
                }
            };
            match dir.next().await {
                Some(Ok(x)) => x.file_name(),
                Some(Err(e)) => {
                    tracing::error!(%e, "Error");
                    return;
                }
                None => {
                    tracing::error!("Didn't find anything in {backlight_dir:?}");
                    return;
                }
            }
        }
    };
    let backlight_brightness_path = [backlight_dir, &name, "brightness".as_ref()]
        .into_iter()
        .collect::<PathBuf>();
    let backlight_max_brightness_path = [backlight_dir, &name, "max_brightness".as_ref()]
        .into_iter()
        .collect::<PathBuf>();

    let max_brightness = smol::fs::read_to_string(&backlight_max_brightness_path)
        .await
        .unwrap();
    match max_brightness.trim().parse::<i32>() {
        Ok(max_brightness) => {
            tracing::info!(max_brightness, "Backlight update");
            let _ = this.update(cx, |this, cx| {
                this.max_brightness.replace(max_brightness);
                cx.notify();
            });
        }
        Err(e) => {
            tracing::error!(%e, "Error");
        }
    }

    let mut brightness_file = smol::fs::OpenOptions::new()
        .read(true)
        .open(&backlight_brightness_path)
        .await
        .unwrap();
    brightness_file
        .seek(smol::io::SeekFrom::Start(0))
        .await
        .unwrap();
    let mut brightness = String::new();
    brightness_file
        .read_to_string(&mut brightness)
        .await
        .unwrap();

    match brightness.trim().parse::<i32>() {
        Ok(brightness) => {
            tracing::info!(brightness, "Backlight update");
            let _ = this.update(cx, |this, cx| {
                this.brightness.replace(brightness);
                cx.notify();
            });
        }
        Err(e) => {
            tracing::error!(%e, "Error");
        }
    }

    let (mut tx, mut rx) = futures::channel::mpsc::unbounded();
    smol::spawn(smol::unblock(move || {
        loop {
            nix::poll::poll(
                &mut [PollFd::new(brightness_file.as_fd(), PollFlags::POLLPRI)],
                PollTimeout::NONE,
            )
            .unwrap();

            smol::block_on(async {
                brightness_file
                    .seek(smol::io::SeekFrom::Start(0))
                    .await
                    .unwrap();
                brightness_file
                    .read_to_string(&mut brightness)
                    .await
                    .unwrap();
                match brightness.trim().parse::<i32>() {
                    Ok(brightness) => {
                        tracing::info!(brightness, "Backlight update");
                        tx.send(brightness).await.unwrap();
                    }
                    Err(e) => {
                        tracing::error!(%e, "Error");
                    }
                }
            });
        }
    }))
    .detach();

    // let udev_monitor = udev::MonitorBuilder::new()
    //     .unwrap()
    //     .match_subsystem("backlight")
    //     .unwrap()
    //     .listen()
    //     .unwrap();
    // for event in udev_monitor.iter() {
    //     tracing::info!(event_type=%event.event_type(), devpath=?event.devpath(), "udev event");
    //     if event.device().sysname() == name {
    //         brightness_file
    //             .seek(smol::io::SeekFrom::Start(0))
    //             .await
    //             .unwrap();
    //         brightness_file
    //             .read_to_string(&mut brightness)
    //             .await
    //             .unwrap();
    //         match brightness.trim().parse::<i32>() {
    //             Ok(brightness) => {
    //                 tracing::info!(brightness, "Backlight update");
    //                 let _ = this.update(cx, |this, cx| {
    //                     this.brightness.replace(brightness);
    //                     cx.notify();
    //                 });
    //             }
    //             Err(e) => {
    //                 tracing::error!(%e, "Error");
    //             }
    //         }
    //     }
    // }

    while let Some(brightness) = rx.next().await {
        let _ = this.update(cx, |this, cx| {
            this.brightness.replace(brightness);
            cx.notify();
        });
    }

    tracing::warn!("End of backlight task");
}

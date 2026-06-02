use std::{
    convert::identity,
    fs::{self, File},
    io::{BufRead, BufReader},
    time::Duration,
};

use gpui::{
    Context,
    IntoElement,
    ParentElement,
    PathBuilder,
    PathStyle,
    Render,
    StrokeOptions,
    Styled,
    Window,
    canvas,
    div,
    opaque_grey,
    point,
    rems,
};
use heapless::HistoryBuf;
use lyon::path::LineCap;
use serde::Deserialize;

use crate::widget::Widget;

const HISTORY_LEN: usize = 16;

pub struct SystemInformation {
    temperature_hardware_name: String,
    cpu_statistics: Option<CpuStatistics>,
    hwmon_was_here: Option<u64>,
    cpu_usage_history: HistoryBuf<f32, HISTORY_LEN>,
    memory_usage_history: HistoryBuf<f32, HISTORY_LEN>,
    temperature_history: HistoryBuf<f32, HISTORY_LEN>,
}

impl Widget for SystemInformation {
    type Config = SystemInformationConfig;

    fn new(cx: &mut Context<Self>, config: &Self::Config) -> Self {
        let update = Duration::from_secs_f64(config.update);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(update).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            }
        })
        .detach();

        Self {
            temperature_hardware_name: config.temperature_hardware_name.clone(),
            cpu_statistics: None,
            hwmon_was_here: None,
            cpu_usage_history: HistoryBuf::new(),
            memory_usage_history: HistoryBuf::new(),
            temperature_history: HistoryBuf::new(),
        }
    }
}

impl Render for SystemInformation {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        fn item<const N: usize>(
            icon: impl IntoElement,
            value: impl IntoElement,
            history: HistoryBuf<f32, N>,
        ) -> gpui::Div {
            div()
                .flex()
                .child(div().font_family("Material Symbols Rounded").child(icon))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .w(rems(3.0))
                        .relative()
                        .child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let mut path = PathBuilder::default().with_style(PathStyle::Stroke(
                                        StrokeOptions::default()
                                            .with_line_cap(LineCap::Round)
                                            .with_line_width(2.0),
                                    ));
                                    for (index, &record) in history.oldest_ordered().rev().enumerate() {
                                        let point = point(bounds.size.width / (history.capacity() - 1) as f32 * index as f32, bounds.size.height * record) * -1.0;
                                        if index == 0 {
                                            path.move_to(point);
                                        } else {
                                            path.line_to(point);
                                        }
                                    }
                                    path.translate(bounds.bottom_right());
                                    match path.build() {
                                        Ok(path) => window.paint_path(path, opaque_grey(0.5, 1.0)),
                                        Err(e) => tracing::error!(error = %e, "Failed to build path for minute hand"),
                                    }
                                },
                            ).absolute().size_full())
                        .child(div().text_size(rems(0.8)).child(value)),
                )
        }
        div().flex().items_center().gap(rems(0.25)).children(
            [
                CpuStatistics::get().and_then(|cpu_statistics| {
                    self.cpu_statistics
                        .replace(cpu_statistics.clone())
                        .map(|old| {
                            let cpu_usage = 1.0 - cpu_statistics.idle_percentage(&old);
                            self.cpu_usage_history.write(cpu_usage as f32);
                            item(
                                "\u{e322}",
                                format!("{:.0}%", (cpu_usage * 100.0).round()),
                                self.cpu_usage_history.clone(),
                            )
                        })
                }),
                MemoryInfo::get().map(|memory_info| {
                    let memory_usage = 1.0 - memory_info.available_percentage();
                    self.memory_usage_history.write(memory_usage as f32);
                    item(
                        "\u{f7a3}",
                        format!("{:.0}%", (memory_usage * 100.0).round()),
                        self.memory_usage_history.clone(),
                    )
                }),
                // Some(item("\u{e1db}", "100%".to_owned())),
                // Some(item("\u{f7a3}", "100%".to_owned())),
                HardwareMonitoring::get(&self.temperature_hardware_name, self.hwmon_was_here).map(
                    |info| {
                        self.hwmon_was_here = Some(info.id);
                        let temperature = info.average_temperature() as f64 / 1000.0;
                        self.temperature_history.write(temperature as f32 / 100.0);
                        item(
                            "\u{f076}",
                            format!("{:.0}\u{b0}C", temperature.round()),
                            self.temperature_history.clone(),
                        )
                    },
                ),
            ]
            .into_iter()
            .filter_map(identity),
        )
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SystemInformationConfig {
    #[serde(default = "default_update")]
    update: f64,
    #[serde(default = "default_temperature_hardware_name")]
    temperature_hardware_name: String,
}

impl Default for SystemInformationConfig {
    fn default() -> Self {
        Self {
            update: default_update(),
            temperature_hardware_name: default_temperature_hardware_name(),
        }
    }
}

fn default_update() -> f64 {
    1.0
}

fn default_temperature_hardware_name() -> String {
    "k10temp".to_owned()
}

// TODO: improve error message

struct MemoryInfo {
    // in kB
    pub total: u64,
    pub available: u64,
}

impl MemoryInfo {
    fn get() -> Option<Self> {
        let path = "/proc/meminfo";
        let file = BufReader::new(
            File::open(path)
                .map_err(|e| {
                    tracing::error!("Failed to open {path}: {e}");
                })
                .ok()?,
        );

        let mut total = None::<u64>;
        let mut available = None::<u64>;

        for line in file.lines() {
            if let Ok(line) = line
                && let &[name, value, unit] =
                    line.split_ascii_whitespace().collect::<Vec<_>>().as_slice()
                && let Ok(value) = value.parse()
            {
                let name = name.strip_suffix(":").unwrap_or(name);
                match name {
                    "MemTotal" => {
                        if unit != "kB" {
                            tracing::error!(
                                "MemTotal with unit `{unit}` is not supported yet, only support kB"
                            );
                            continue;
                        }

                        total = Some(value);

                        if total.is_some() && available.is_some() {
                            break;
                        }
                    }
                    "MemAvailable" => {
                        if unit != "kB" {
                            tracing::error!(
                                "MemAvailable with unit `{unit}` is not supported yet, only support kB"
                            );
                            continue;
                        }

                        available = Some(value);

                        if total.is_some() && available.is_some() {
                            break;
                        }
                    }
                    _ => (),
                }
            }
        }

        if let Some(total) = total
            && let Some(available) = available
        {
            Some(Self { total, available })
        } else {
            None
        }
    }
    fn available_percentage(&self) -> f64 {
        self.available as f64 / self.total as f64
    }
}

#[derive(Clone)]
struct CpuStatistics {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
    #[allow(unused)]
    guest: u64,
    #[allow(unused)]
    guest_nice: u64,
}

impl CpuStatistics {
    fn get() -> Option<Self> {
        let path = "/proc/stat";
        let file = BufReader::new(
            File::open(path)
                .map_err(|e| {
                    tracing::error!("Failed to open {path}: {e}");
                })
                .ok()?,
        );

        file.lines().find_map(|line| {
            if let Ok(line) = line
                && let mut line = line.split_ascii_whitespace()
                && line.next() == Some("cpu")
                && let &[
                    Ok(user),
                    Ok(nice),
                    Ok(system),
                    Ok(idle),
                    Ok(iowait),
                    Ok(irq),
                    Ok(softirq),
                    Ok(steal),
                    Ok(guest),
                    Ok(guest_nice),
                ] = line.map(str::parse).collect::<Vec<_>>().as_slice()
            {
                Some(Self {
                    user,
                    nice,
                    system,
                    idle,
                    iowait,
                    irq,
                    softirq,
                    steal,
                    guest,
                    guest_nice,
                })
            } else {
                None
            }
        })
    }
    #[allow(dead_code)]
    fn busy_time(&self) -> u64 {
        let Self {
            user,
            nice,
            system,
            idle: _,
            iowait: _,
            irq,
            softirq,
            steal,
            guest: _,
            guest_nice: _,
        } = self;
        user + nice + system + irq + softirq + steal
    }
    fn total_time(&self) -> u64 {
        let Self {
            user,
            nice,
            system,
            idle,
            iowait,
            irq,
            softirq,
            steal,
            guest: _,
            guest_nice: _,
        } = self;
        user + nice + system + idle + iowait + irq + softirq + steal
    }
    #[allow(dead_code)]
    fn busy_percentage(&self, old: &Self) -> f64 {
        (self.busy_time() - old.busy_time()) as f64 / (self.total_time() - old.total_time()) as f64
    }
    fn idle_percentage(&self, old: &Self) -> f64 {
        (self.idle - old.idle) as f64 / (self.total_time() - old.total_time()) as f64
    }
}

struct HardwareMonitoring {
    id: u64,
    temperatures: Vec<i64>,
}

impl HardwareMonitoring {
    fn get(name: &str, was_here: Option<u64>) -> Option<Self> {
        was_here
            .and_then(|was_here| {
                fs::read_to_string(format!("/sys/class/hwmon/hwmon{was_here}/name"))
                    .is_ok_and(|x| x.trim() == name)
                    .then(|| {
                        let path = format!("/sys/class/hwmon/hwmon{was_here}/");
                        match fs::read_dir(&path) {
                            Ok(x) => Some((was_here, x)),
                            Err(e) => {
                                tracing::warn!("Failed to open directory `{path}`: {e}");
                                None
                            }
                        }
                    })
            })
            .flatten()
            .or_else(|| match fs::read_dir("/sys/class/hwmon") {
                Ok(hwmon_dir) => hwmon_dir.filter_map(Result::ok).find_map(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .and_then(|x| x.strip_prefix("hwmon"))
                        .and_then(|x| x.parse::<u64>().ok())
                        .and_then(|id| {
                            fs::read_to_string(entry.path().join("name"))
                                .is_ok_and(|x| x.trim() == name)
                                .then(|| entry.path().read_dir().ok())
                                .flatten()
                                .map(|dir| (id, dir))
                        })
                }),
                Err(e) => {
                    tracing::error!("Failed to open hwmon directory: {e}");
                    None
                }
            })
            .map(|(id, dir)| Self {
                id,
                temperatures: dir
                    .filter_map(|entry| {
                        entry.ok().and_then(|entry| {
                            entry
                                .file_name()
                                .to_str()
                                .is_some_and(|file_name| {
                                    file_name.starts_with("temp") && file_name.ends_with("input")
                                })
                                .then(|| {
                                    fs::read_to_string(entry.path())
                                        .ok()
                                        .and_then(|temp| temp.trim().parse().ok())
                                })
                                .flatten()
                        })
                    })
                    .collect(),
            })
    }
    fn average_temperature(&self) -> i64 {
        self.temperatures.iter().sum::<i64>() / self.temperatures.len() as i64
    }
}

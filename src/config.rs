use std::{env, error::Error, fs, path::PathBuf};

use serde::Deserialize;

use crate::widget::{
    bluetooth::BluetoothConfig,
    clock::ClockConfig,
    network::NetworkConfig,
    power_menu::PowerMenuConfig,
    power_profile::PowerProfileConfig,
    system_information::SystemInformationConfig,
    volume::VolumeConfig,
};

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub left: Box<[WidgetOptionGroup]>,
    pub middle: Box<[WidgetOptionGroup]>,
    pub right: Box<[WidgetOptionGroup]>,
    pub widget: WidgetConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            left: Box::new([
                WidgetOption::PowerMenu.into(),
                WidgetOption::Power.into(),
                WidgetOption::Clock.into(),
                WidgetOption::Display.into(),
            ]),
            middle: Box::new([WidgetOption::Workspaces.into()]),
            right: Box::new([
                WidgetOption::Volume.into(),
                WidgetOption::Bluetooth.into(),
                WidgetOption::PowerProfile.into(),
            ]),
            widget: WidgetConfig::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn Error>> {
        let path = if let Some(config_home) = env::var_os("XDG_CONFIG_HOME")
            && !config_home.is_empty()
        {
            [
                config_home.as_os_str(),
                "eucalyptus-twig/eucalyptus-twig.toml".as_ref(),
            ]
            .iter()
            .collect::<PathBuf>()
        } else if let Some(home_dir) = env::home_dir() {
            tracing::warn!("XDG_CONFIG_HOME is not set or is empty, default to $HOME/.config");
            [
                home_dir.as_os_str(),
                ".config/eucalyptus-twig/eucalyptus-twig.toml".as_ref(),
            ]
            .iter()
            .collect()
        } else {
            return Err("Failed to get home directory".into());
        };
        let config_content = fs::read(path)?;
        Ok(toml::from_slice(&config_content)?)
    }
}

#[derive(Deserialize)]
pub enum WidgetOption {
    Bluetooth,
    Clock,
    Display,
    HyprlandWorkspace,
    Network,
    Power,
    PowerMenu,
    PowerProfile,
    Quit,
    SystemInformation,
    Volume,
    Workspaces,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum WidgetOptionGroup {
    One(WidgetOption),
    Array(Box<[WidgetOption]>),
}

impl From<WidgetOption> for WidgetOptionGroup {
    fn from(value: WidgetOption) -> Self {
        Self::One(value)
    }
}

impl<T> From<T> for WidgetOptionGroup
where
    T: Into<Box<[WidgetOption]>>,
{
    fn from(value: T) -> Self {
        Self::Array(value.into())
    }
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WidgetConfig {
    pub bluetooth: BluetoothConfig,
    pub clock: ClockConfig,
    pub network: NetworkConfig,
    pub power_menu: PowerMenuConfig,
    pub power_profile: PowerProfileConfig,
    pub system_information: SystemInformationConfig,
    pub volume: VolumeConfig,
}

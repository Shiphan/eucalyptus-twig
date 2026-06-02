use std::{env, error::Error, fs, path::PathBuf};

use serde::Deserialize;

use crate::widget::{
    WidgetOption,
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
    pub left: Box<[WidgetOption]>,
    pub middle: Box<[WidgetOption]>,
    pub right: Box<[WidgetOption]>,
    pub widget: WidgetConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            left: Box::new([
                WidgetOption::PowerMenu,
                WidgetOption::Power,
                WidgetOption::Clock,
                WidgetOption::Display,
            ]),
            middle: Box::new([WidgetOption::Workspaces]),
            right: Box::new([
                WidgetOption::Volume,
                WidgetOption::Bluetooth,
                WidgetOption::PowerProfile,
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

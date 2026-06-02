use std::{env, error::Error, fs, path::PathBuf};

use serde::Deserialize;

use crate::widget::{
    WidgetOption, bluetooth::BluetoothConfig, clock::ClockConfig, network::NetworkConfig,
    power_menu::PowerMenuConfig, power_profile::PowerProfileConfig,
    system_information::SystemInformationConfig, volume::VolumeConfig,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub left: Vec<WidgetOption>,
    #[serde(default)]
    pub middle: Vec<WidgetOption>,
    #[serde(default)]
    pub right: Vec<WidgetOption>,
    #[serde(default)]
    pub widget: WidgetConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            left: vec![
                WidgetOption::PowerMenu,
                WidgetOption::Power,
                WidgetOption::Clock,
                WidgetOption::Display,
            ],
            middle: vec![WidgetOption::Workspaces],
            right: vec![
                WidgetOption::Volume,
                WidgetOption::Bluetooth,
                WidgetOption::PowerProfile,
            ],
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
#[serde(deny_unknown_fields)]
pub struct WidgetConfig {
    #[serde(default)]
    pub bluetooth: BluetoothConfig,
    #[serde(default)]
    pub clock: ClockConfig,
    #[serde(default)]
    pub network: NetworkConfig,
    #[serde(default)]
    pub power_menu: PowerMenuConfig,
    #[serde(default)]
    pub power_profile: PowerProfileConfig,
    #[serde(default)]
    pub system_information: SystemInformationConfig,
    #[serde(default)]
    pub volume: VolumeConfig,
}

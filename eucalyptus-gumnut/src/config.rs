use std::{
    borrow::Cow,
    cell::LazyCell,
    error::Error,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::item;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub hide_delay: f64,
    pub item: ItemConfig,
}

const XDG_CONFIG_HOME: LazyCell<Cow<'static, Path>> = LazyCell::new(|| {
    if let Some(xdg_config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        Cow::Owned(PathBuf::from(xdg_config_home))
    } else if let Some(home) = std::env::home_dir() {
        Cow::Owned(home.join(".config"))
    } else {
        Cow::Borrowed(Path::new("~/.config"))
    }
});

impl Config {
    pub const PATH: LazyCell<PathBuf> =
        LazyCell::new(|| XDG_CONFIG_HOME.join("eucalyptus-gumnut/eucalyptus-gumnut.toml"));

    pub fn load() -> Result<Self, Box<dyn Error>> {
        let config_content = std::fs::read(Self::PATH.as_path())?;
        Ok(toml::from_slice(&config_content)?)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hide_delay: 3.0,
            item: ItemConfig::default(),
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemConfig {
    pub power_profile: item::power_profile::Config,
    // pub backlight: BacklightConfig,
}

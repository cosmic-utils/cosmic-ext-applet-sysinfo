use cosmic::cosmic_config::{
    self, Config, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
};

pub const CONFIG_VERSION: u64 = 1;
pub const APP_ID: &str = "io.github.cosmic-utils.cosmic-ext-applet-sysinfo";

#[derive(Default, Debug, CosmicConfigEntry)]
pub struct SysInfoConfig {
    pub include_interfaces: Option<Vec<String>>,
    pub exclude_interfaces: Option<Vec<String>>,
}

impl SysInfoConfig {
    pub fn config_handler() -> Option<Config> {
        Config::new(APP_ID, CONFIG_VERSION).ok()
    }

    pub fn config() -> SysInfoConfig {
        match Self::config_handler() {
            Some(config_handler) => SysInfoConfig::get_entry(&config_handler)
                .map_err(|error| {
                    tracing::info!("error whilst loading config: {:#?}", error);
                })
                .unwrap_or_default(),
            None => SysInfoConfig::default(),
        }
    }
}

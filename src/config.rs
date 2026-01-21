use cosmic::cosmic_config::{
    self, Config, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
};

const CONFIG_VERSION: u64 = 1;

pub const APP_ID: &str = "io.github.cosmic-utils.cosmic-ext-applet-sysinfo";

#[derive(Default, Debug, Clone, CosmicConfigEntry)]
pub struct SysInfoConfig {
    pub include_interfaces: Option<Vec<String>>,
    pub exclude_interfaces: Option<Vec<String>>,
    /// Whether to include Swap usage in the RAM segment
    pub(crate) include_swap_in_ram: bool,
}

impl SysInfoConfig {
    fn config_handler() -> Option<Config> {
        Config::new(APP_ID, CONFIG_VERSION).ok()
    }

    pub fn config() -> SysInfoConfig {
        match Self::config_handler() {
            Some(config_handler) => SysInfoConfig::get_entry(&config_handler)
                .map_err(|error| {
                    tracing::info!("Error whilst loading config: {:#?}", error);
                })
                .unwrap_or_default(),
            None => SysInfoConfig::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Flags {
    pub(crate) config: SysInfoConfig,
    pub(crate) config_handler: Option<cosmic_config::Config>,
}

impl Flags {
    pub(crate) fn new() -> Self {
        Self {
            config: SysInfoConfig::config(),
            config_handler: SysInfoConfig::config_handler(),
        }
    }
}

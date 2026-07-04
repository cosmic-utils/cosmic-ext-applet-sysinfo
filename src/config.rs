use cosmic::cosmic_config::{
    self, Config, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
};

const CONFIG_VERSION: u64 = 1;

pub(crate) const APP_ID: &str = "io.github.cosmic-utils.cosmic-ext-applet-sysinfo";

#[derive(Debug, Clone, CosmicConfigEntry)]
pub(crate) struct SysInfoConfig {
    pub(crate) include_interfaces: Option<Vec<String>>,
    pub(crate) exclude_interfaces: Option<Vec<String>>,
    /// Whether to include Swap usage in the RAM segment
    pub(crate) include_swap_in_ram: bool,
    /// Whether to render the entire template in a monospace font.
    /// When false, the system default font is used for all spans.
    pub(crate) use_mono_font: bool,
    /// Template string controlling the applet display.
    /// Available variables: {cpu_usage}, {ram_usage}, {cpu_temp}, {gpu_temp}, {gpu_usage},
    /// {npu_usage}, {npu_frequency}, {dl_speed}, {ul_speed}, {pub_ipv4}, {pub_ipv6},
    /// {disk_read}, {disk_write}
    pub(crate) template: String,
}

impl Default for SysInfoConfig {
    fn default() -> Self {
        Self {
            include_interfaces: None,
            exclude_interfaces: None,
            include_swap_in_ram: false,
            use_mono_font: false,
            template: "CPU {cpu_usage} RAM {ram_usage} ↓{dl_speed}M/s ↑{ul_speed}M/s".to_string(),
        }
    }
}

impl SysInfoConfig {
    fn config_handler() -> Option<Config> {
        Config::new(APP_ID, CONFIG_VERSION).ok()
    }

    fn config() -> SysInfoConfig {
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

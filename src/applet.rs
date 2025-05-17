use std::time::{Duration, Instant};
use std::fs;
use std::path::Path;
use serde::Deserialize;
use directories::ProjectDirs;

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Networks, RefreshKind, System};

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<SysInfo>(())
}

struct SysInfo {
    core: cosmic::app::Core,
    system: System,
    networks: Networks,
    cpu_usage: f32,
    ram_usage: u64,
    download_speed: f64,
    upload_speed: f64,
    physical_interfaces: Vec<String>,
    config: Config,
    last_scan: Instant,
}

#[derive(Debug, Deserialize, Default)]
struct Config {
    include_interfaces: Option<Vec<String>>,
    exclude_interfaces: Option<Vec<String>>,
}

impl SysInfo {
    fn load_config() -> Config {
        if let Some(proj_dirs) = ProjectDirs::from("io", "github", "cosmic-ext-applet-sysinfo") {
            let config_path = proj_dirs.config_dir().join("config.toml");
            if let Ok(contents) = fs::read_to_string(config_path) {
                if let Ok(cfg) = toml::from_str(&contents) {
                    return cfg;
                }
            }
        }
        Config::default()
    }

    fn get_physical_interfaces(config: &Config) -> Vec<String> {
        let mut interfaces = Vec::new();
        if let Ok(entries) = fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                let iface = entry.file_name().into_string().unwrap_or_default();
                if Path::new(&format!("/sys/class/net/{}/device", iface)).exists() {
                    interfaces.push(iface);
                }
            }
        }
        // Apply config filters
        if let Some(ref include) = config.include_interfaces {
            interfaces.retain(|iface| include.contains(iface));
        }
        if let Some(ref exclude) = config.exclude_interfaces {
            interfaces.retain(|iface| !exclude.contains(iface));
        }
        interfaces
    }

    fn rescan_physical_interfaces(&mut self) {
        self.physical_interfaces = Self::get_physical_interfaces(&self.config);
        self.last_scan = Instant::now();
    }

    fn update_sysinfo_data(&mut self) {
        // Rescan interfaces every 10 seconds
        if self.last_scan.elapsed() > Duration::from_secs(10) {
            self.rescan_physical_interfaces();
        }

        self.system.refresh_specifics(
            RefreshKind::nothing()
                .with_memory(MemoryRefreshKind::nothing().with_ram())
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage()),
        );

        self.cpu_usage = self.system.global_cpu_usage();
        self.ram_usage = (self.system.used_memory() * 100) / self.system.total_memory();

        self.networks.refresh(true);

        let mut upload = 0;
        let mut download = 0;

        for (name, data) in self.networks.iter() {
            if self.physical_interfaces.contains(&name.to_string()) {
                upload += data.transmitted();
                download += data.received();
            }
        }

        self.upload_speed = (upload as f64) / 1_000_000.0;
        self.download_speed = (download as f64) / 1_000_000.0;
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
}

impl cosmic::Application for SysInfo {
    type Flags = ();
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;

    const APP_ID: &'static str = "io.github.cosmic-utils.cosmic-ext-applet-sysinfo";

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (Self, cosmic::app::Task<Self::Message>) {
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(MemoryRefreshKind::nothing().with_ram())
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage()),
        );

        let networks = Networks::new_with_refreshed_list();
        let config = SysInfo::load_config();
        let physical_interfaces = SysInfo::get_physical_interfaces(&config);
        let last_scan = Instant::now();

        (
            Self {
                core,
                system,
                networks,
                cpu_usage: 0.0,
                ram_usage: 0,
                download_speed: 0.00,
                upload_speed: 0.00,
                physical_interfaces,
                config,
                last_scan,
            },
            cosmic::task::none(),
        )
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Message> {
        cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
    }

    fn update(&mut self, message: Message) -> cosmic::app::Task<Self::Message> {
        match message {
            Message::Tick => self.update_sysinfo_data(),
        }

        cosmic::task::none()
    }

    fn view(&self) -> cosmic::Element<Message> {
        let content = {
            cosmic::iced_widget::row![
                cosmic::iced_widget::text(format!("C {:.0}%", self.cpu_usage)),
                cosmic::iced_widget::text(format!("R {}%", self.ram_usage)),
                cosmic::iced_widget::text(format!("↓{:.2}MB/s", self.download_speed)),
                cosmic::iced_widget::text(format!("↑{:.2}MB/s", self.upload_speed)),
            ]
            .spacing(5)
        };

        let button =
            cosmic::widget::button::custom(content).class(cosmic::theme::Button::AppletIcon);

        cosmic::widget::autosize::autosize(button, cosmic::widget::Id::unique()).into()
    }
}

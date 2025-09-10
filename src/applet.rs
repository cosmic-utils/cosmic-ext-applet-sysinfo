use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Networks, RefreshKind, System};

use crate::config::{APP_ID, SysInfoConfig};

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<SysInfo>(())
}

struct SysInfo {
    core: cosmic::app::Core,
    config: SysInfoConfig,
    system: System,
    networks: Networks,
    cpu_usage: f32,
    ram_usage: u64,
    download_speed: f64,
    upload_speed: f64,
    last_scan: Instant,
    physical_interfaces: Vec<String>,
}

impl SysInfo {
    fn get_physical_interfaces(config: &SysInfoConfig) -> Vec<String> {
        let mut interfaces = Vec::new();

        if let Ok(entries) = fs::read_dir("/sys/class/net") {
            for entry in entries.flatten() {
                let interface = entry.file_name().into_string().unwrap_or_default();

                if Path::new(&format!("/sys/class/net/{}/device", interface)).exists() {
                    interfaces.push(interface);
                }
            }
        }

        // Apply config filters
        if let Some(included_interfaces) = &config.include_interfaces {
            interfaces.retain(|interface| included_interfaces.contains(interface));
        }
        if let Some(excluded_interface) = &config.exclude_interfaces {
            interfaces.retain(|interface| !excluded_interface.contains(interface));
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
            if self.physical_interfaces.contains(name) {
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

    const APP_ID: &'static str = APP_ID;

    fn init(
        core: cosmic::app::Core,
        _flags: Self::Flags,
    ) -> (Self, cosmic::app::Task<Self::Message>) {
        let config = SysInfoConfig::config();

        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(MemoryRefreshKind::nothing().with_ram())
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage()),
        );
        let networks = Networks::new_with_refreshed_list();

        let last_scan = Instant::now();
        let physical_interfaces = SysInfo::get_physical_interfaces(&config);

        (
            Self {
                core,
                config,
                system,
                networks,
                cpu_usage: 0.0,
                ram_usage: 0,
                download_speed: 0.00,
                upload_speed: 0.00,
                last_scan,
                physical_interfaces,
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

    fn view(&self) -> cosmic::Element<'_, Message> {
        let data = {
            cosmic::iced_widget::row![
                cosmic::iced_widget::text(format!("CPU {:.0}%", self.cpu_usage)),
                cosmic::iced_widget::text(format!("RAM {}%", self.ram_usage)),
                cosmic::iced_widget::text(format!("↓{:.2}MB/s", self.download_speed)),
                cosmic::iced_widget::text(format!("↑{:.2}MB/s", self.upload_speed)),
            ]
            .spacing(4)
        };

        let button = cosmic::widget::button::custom(data).class(cosmic::theme::Button::AppletIcon);

        cosmic::widget::autosize::autosize(button, cosmic::widget::Id::unique()).into()
    }
}

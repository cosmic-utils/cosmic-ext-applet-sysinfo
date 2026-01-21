use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use sysinfo::{CpuRefreshKind, MemoryRefreshKind, Networks, RefreshKind, System};
use tracing::{debug, trace};

use crate::{
    config::{APP_ID, Flags, SysInfoConfig},
    fl,
};

pub fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<SysInfo>(Flags::new())
}

struct SysInfo {
    core: cosmic::app::Core,
    popup: Option<cosmic::iced::window::Id>,
    config: SysInfoConfig,
    config_handler: Option<cosmic::cosmic_config::Config>,
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
        self.ram_usage = if self.config.include_swap_in_ram {
            ((self.system.used_memory() + self.system.used_swap()) * 100)
                / (self.system.total_memory() + self.system.total_swap())
        } else {
            (self.system.used_memory() * 100) / self.system.total_memory()
        };

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Tick,
    ToggleWindow,
    PopupClosed(cosmic::iced::window::Id),
    ToggleIncludeSwapWithRam(bool),
}

impl cosmic::Application for SysInfo {
    type Flags = Flags;
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;

    const APP_ID: &'static str = APP_ID;

    fn init(
        core: cosmic::app::Core,
        flags: Self::Flags,
    ) -> (Self, cosmic::app::Task<Self::Message>) {
        let config = flags.config;

        let memory_config = if config.include_swap_in_ram {
            MemoryRefreshKind::nothing().with_ram().with_swap()
        } else {
            MemoryRefreshKind::nothing().with_ram()
        };
        let system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(memory_config)
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage()),
        );
        let networks = Networks::new_with_refreshed_list();

        let last_scan = Instant::now();
        let physical_interfaces = SysInfo::get_physical_interfaces(&config);

        (
            Self {
                core,
                popup: None,
                config,
                config_handler: flags.config_handler,
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

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> cosmic::app::Task<Self::Message> {
        match message {
            // don't spam the logs with the tick
            Message::Tick => trace!(?message),
            _ => debug!(?message),
        }

        match message {
            Message::Tick => self.update_sysinfo_data(),
            Message::ToggleWindow => {
                if let Some(id) = self.popup.take() {
                    debug!("have popup with id={id}, destroying");

                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                } else {
                    debug!("do not have a popup, creating");

                    let new_id = cosmic::iced::window::Id::unique();
                    debug_assert_eq!(self.popup.replace(new_id), None);

                    let popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    return cosmic::iced::platform_specific::shell::commands::popup::get_popup(
                        popup_settings,
                    );
                }
            }
            Message::PopupClosed(id) => {
                if let Some(i) = self.popup.take() {
                    debug_assert_eq!(i, id, "got PopupClosed message for an outdated popup id");
                }
            }
            Message::ToggleIncludeSwapWithRam(value) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_include_swap_in_ram(handler, value)
                {
                    tracing::error!("{error}")
                }
            }
        }

        cosmic::task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Message> {
        let data = {
            cosmic::iced_widget::row![
                cosmic::iced_widget::text(format!("CPU {:.0}%", self.cpu_usage)),
                cosmic::iced_widget::text(format!("RAM {}%", self.ram_usage)),
                cosmic::iced_widget::text(format!("↓{:.2}M/s", self.download_speed)),
                cosmic::iced_widget::text(format!("↑{:.2}M/s", self.upload_speed)),
            ]
            .spacing(4)
        };

        let button = cosmic::widget::button::custom(data)
            .class(cosmic::theme::Button::AppletIcon)
            .on_press_down(Message::ToggleWindow);

        cosmic::widget::autosize::autosize(button, cosmic::widget::Id::unique()).into()
    }

    fn view_window(&self, _id: cosmic::iced::window::Id) -> cosmic::Element<'_, Message> {
        let include_swap_in_ram_toggler = cosmic::iced_widget::row![
            cosmic::widget::text(fl!("include-swap-in-ram-toggle")),
            cosmic::widget::Space::with_width(cosmic::iced::Length::Fill),
            cosmic::widget::toggler(self.config.include_swap_in_ram)
                .on_toggle(Message::ToggleIncludeSwapWithRam),
        ];

        let data = cosmic::iced_widget::column![
            // padding comment to make formatting nicer
            cosmic::applet::padded_control(include_swap_in_ram_toggler)
        ]
        .padding([16, 0]);

        self.core
            .applet
            .popup_container(cosmic::widget::container(data))
            .into()
    }
}

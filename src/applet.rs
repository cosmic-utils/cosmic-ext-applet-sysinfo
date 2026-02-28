use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

use crate::{
    config::{APP_ID, Flags, SysInfoConfig},
    fl, template,
};
use cosmic::iced::Color;
use sysinfo::{Components, CpuRefreshKind, MemoryRefreshKind, Networks, RefreshKind, System};

pub(crate) fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<SysInfo>(Flags::new())
}

struct ThemeColors {
    yellow: Color,
    red: Color,
}

impl ThemeColors {
    fn from_active_theme() -> Self {
        let theme = cosmic::theme::active();
        let cosmic = theme.cosmic();
        Self {
            yellow: cosmic.warning_color().into(),
            red: cosmic.destructive_color().into(),
        }
    }

    fn threshold(&self, value: f64, warn: f64, critical: f64) -> Option<Color> {
        if value >= critical {
            Some(self.red)
        } else if value >= warn {
            Some(self.yellow)
        } else {
            None
        }
    }
}

struct SysInfo {
    core: cosmic::app::Core,
    popup: Option<cosmic::iced::window::Id>,
    config: SysInfoConfig,
    config_handler: Option<cosmic::cosmic_config::Config>,
    system: System,
    networks: Networks,
    components: Components,
    cpu_usage: f32,
    ram_usage: u64,
    download_speed: f64,
    upload_speed: f64,
    cpu_temp: Option<f32>,
    gpu_temp: Option<f32>,
    gpu_usage: Option<u64>,
    last_scan: Instant,
    physical_interfaces: Vec<String>,
    template_segments: Vec<template::Segment>,
    template_requires: template::Requires,
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

        if let Some(included) = &config.include_interfaces {
            interfaces.retain(|i| included.contains(i));
        }
        if let Some(excluded) = &config.exclude_interfaces {
            interfaces.retain(|i| !excluded.contains(i));
        }

        interfaces
    }

    fn rescan_physical_interfaces(&mut self) {
        self.physical_interfaces = Self::get_physical_interfaces(&self.config);
        self.last_scan = Instant::now();
    }

    fn update_template_cache(&mut self) {
        self.template_segments = template::parse(&self.config.template);
        self.template_requires = template::Requires::from_segments(&self.template_segments);
    }

    fn update_sysinfo_data(&mut self) {
        if self.last_scan.elapsed() > Duration::from_secs(10) {
            self.rescan_physical_interfaces();
        }

        let memory_refresh = if self.config.include_swap_in_ram {
            MemoryRefreshKind::nothing().with_ram().with_swap()
        } else {
            MemoryRefreshKind::nothing().with_ram()
        };

        self.system.refresh_specifics(
            RefreshKind::nothing()
                .with_memory(memory_refresh)
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

        if self.template_requires.cpu_temp || self.template_requires.gpu_temp {
            self.components.refresh(true);
            if self.template_requires.cpu_temp {
                self.cpu_temp = self.find_cpu_temp();
            }
        }

        if self.template_requires.gpu_temp || self.template_requires.gpu_usage {
            let nvidia = OnceCell::new(|| Self::query_nvidia_smi());
            if self.template_requires.gpu_temp {
                self.gpu_temp = self
                    .find_gpu_temp()
                    .or(nvidia.as_ref().and_then(|(temp, _)| *temp));
            }
            if self.template_requires.gpu_usage {
                self.gpu_usage =
                    Self::find_gpu_usage_sysfs().or(nvidia.as_ref().and_then(|(_, usage)| *usage));
            }
        }
    }

    fn find_cpu_temp(&self) -> Option<f32> {
        const LABELS: &[&str] = &[
            "coretemp",
            "k10temp",
            "zenpower",
            "cpu_thermal",
            "soc_thermal",
            "cpu",
            "package",
            "tctl",
            "tdie",
            "core",
        ];

        self.components
            .iter()
            .find(|c| {
                let label = c.label().to_lowercase();
                LABELS.iter().any(|l| label.contains(l))
            })
            .and_then(|c| c.temperature())
    }

    fn find_gpu_temp(&self) -> Option<f32> {
        const LABELS: &[&str] = &[
            "amdgpu", "radeon", "nouveau", "nvidia", "gpu", "edge", "junction", "mem",
        ];

        self.components
            .iter()
            .find(|c| {
                let label = c.label().to_lowercase();
                LABELS.iter().any(|l| label.contains(l))
            })
            .and_then(|c| c.temperature())
    }

    fn find_gpu_usage_sysfs() -> Option<u64> {
        let entries = fs::read_dir("/sys/class/drm").ok()?;
        for entry in entries.flatten() {
            if let Ok(contents) = fs::read_to_string(entry.path().join("device/gpu_busy_percent"))
                && let Ok(value) = contents.trim().parse()
            {
                return Some(value);
            }
        }
        None
    }

    fn query_nvidia_smi() -> Option<(Option<f32>, Option<u64>)> {
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=temperature.gpu,utilization.gpu",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some((temp, util)) = stdout.trim().split_once(", ") {
            Some((temp.trim().parse().ok(), util.trim().parse().ok()))
        } else {
            Some((None, None))
        }
    }

    fn resolve_variable(
        &self,
        var: template::Variable,
        colors: &ThemeColors,
    ) -> (String, Option<Color>) {
        match var {
            template::Variable::CpuUsage => {
                let val = self.cpu_usage as f64;
                (format!("{:.0}%", val), colors.threshold(val, 50.0, 80.0))
            }
            template::Variable::RamUsage => {
                let val = self.ram_usage as f64;
                (
                    format!("{}%", self.ram_usage),
                    colors.threshold(val, 50.0, 80.0),
                )
            }
            template::Variable::CpuTemp => match self.cpu_temp {
                Some(t) => (
                    format!("{:.0}°C", t),
                    colors.threshold(t as f64, 60.0, 80.0),
                ),
                None => ("--°C".to_string(), None),
            },
            template::Variable::GpuTemp => match self.gpu_temp {
                Some(t) => (
                    format!("{:.0}°C", t),
                    colors.threshold(t as f64, 60.0, 85.0),
                ),
                None => ("--°C".to_string(), None),
            },
            template::Variable::GpuUsage => match self.gpu_usage {
                Some(u) => (format!("{}%", u), colors.threshold(u as f64, 50.0, 80.0)),
                None => ("--%".to_string(), None),
            },
            template::Variable::DlSpeed => (format!("{:.2}", self.download_speed), None),
            template::Variable::UlSpeed => (format!("{:.2}", self.upload_speed), None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Message {
    Tick,
    ToggleWindow,
    PopupClosed(cosmic::iced::window::Id),
    ToggleIncludeSwapWithRam(bool),
    TemplateChanged(String),
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
        let components = Components::new_with_refreshed_list();

        let last_scan = Instant::now();
        let physical_interfaces = SysInfo::get_physical_interfaces(&config);
        let template_segments = template::parse(&config.template);
        let template_requires = template::Requires::from_segments(&template_segments);

        (
            Self {
                core,
                popup: None,
                config,
                config_handler: flags.config_handler,
                system,
                networks,
                components,
                cpu_usage: 0.0,
                ram_usage: 0,
                download_speed: 0.0,
                upload_speed: 0.0,
                cpu_temp: None,
                gpu_temp: None,
                gpu_usage: None,
                last_scan,
                physical_interfaces,
                template_segments,
                template_requires,
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

    fn style(&self) -> Option<cosmic::iced_runtime::Appearance> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> cosmic::app::Task<Self::Message> {
        match message {
            Message::Tick => self.update_sysinfo_data(),
            Message::ToggleWindow => {
                if let Some(id) = self.popup.take() {
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }

                let new_id = cosmic::iced::window::Id::unique();
                self.popup.replace(new_id);

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
            Message::PopupClosed(id) => {
                self.popup.take_if(|stored_id| stored_id == &id);
            }
            Message::ToggleIncludeSwapWithRam(value) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_include_swap_in_ram(handler, value)
                {
                    tracing::error!("{error}")
                }
            }
            Message::TemplateChanged(value) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_template(handler, value)
                {
                    tracing::error!("failed to set template: {error}")
                }
                self.update_template_cache();
            }
        }

        cosmic::task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Message> {
        use cosmic::iced_widget::{rich_text, span};

        let colors = ThemeColors::from_active_theme();

        let spans: Vec<_> = self
            .template_segments
            .iter()
            .map(|segment| match segment {
                template::Segment::Literal(text) => span(text.clone()),
                template::Segment::Variable(var) => {
                    let (text, color) = self.resolve_variable(*var, &colors);
                    span(text).color_maybe(color)
                }
            })
            .collect();

        let content = rich_text(spans);

        let button = cosmic::widget::button::custom(content)
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

        let template_input = cosmic::iced_widget::column![
            cosmic::widget::text::body(fl!("template-label")),
            cosmic::widget::text_input("", &self.config.template)
                .on_input(Message::TemplateChanged),
        ]
        .spacing(4);

        let data = cosmic::iced_widget::column![
            cosmic::applet::padded_control(include_swap_in_ram_toggler),
            cosmic::applet::padded_control(template_input),
        ]
        .padding([16, 0]);

        self.core
            .applet
            .popup_container(cosmic::widget::container(data))
            .into()
    }
}

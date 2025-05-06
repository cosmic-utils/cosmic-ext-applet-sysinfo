use std::time::Duration;

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
}

impl SysInfo {
    fn update_sysinfo_data(&mut self) {
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

        for (_, data) in self.networks.iter() {
            upload += data.transmitted();
            download += data.received();
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

        (
            Self {
                core,
                system,
                networks,
                cpu_usage: 0.0,
                ram_usage: 0,
                download_speed: 0.00,
                upload_speed: 0.00,
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
                cosmic::iced_widget::text(format!("C: {:.0}%", self.cpu_usage)),
                cosmic::iced_widget::text(format!("R: {}%", self.ram_usage)),
                cosmic::iced_widget::text(format!(
                    "N: ↓{:.2}MB/s ↑{:.2}MB/s",
                    self.download_speed, self.upload_speed
                )),
            ]
            .spacing(8)
            .align_y(cosmic::iced::Alignment::Center)
        };

        let button = cosmic::widget::button::custom(content)
            .padding([
                self.core.applet.suggested_padding(false),
                self.core.applet.suggested_padding(false),
            ])
            .class(cosmic::theme::Button::AppletIcon);

        cosmic::widget::autosize::autosize(button, cosmic::widget::Id::unique()).into()
    }
}

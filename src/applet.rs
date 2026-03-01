use std::{str::FromStr, time::Duration};

use cosmic::iced::Color;
use tracing::{debug, trace};

use crate::{
    config::{APP_ID, Flags, SysInfoConfig},
    data, fl, template,
};

pub(crate) fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<SysInfo>(Flags::new())
}

pub(crate) struct ThemeColors {
    pub(crate) yellow: Color,
    pub(crate) red: Color,
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

    pub(crate) fn threshold(&self, value: f64, warn: f64, critical: f64) -> Option<Color> {
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
    data: data::Data,
    template: template::Template,
}

impl SysInfo {
    fn update_template_cache(&mut self) {
        let Ok(template) = template::Template::from_str(&self.config.template);
        self.template = template;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Message {
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
        let data = data::Data::new(&config);
        let Ok(template) = template::Template::from_str(&config.template);

        (
            Self {
                core,
                popup: None,
                config,
                config_handler: flags.config_handler,
                data,
                template,
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
            Message::Tick => trace!(?message),
            _ => debug!(?message),
        }

        match message {
            Message::Tick => self.data.refresh(self.template.requires, &self.config),
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
        let colors = ThemeColors::from_active_theme();

        let content = self.template.render(&self.data, &colors);

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

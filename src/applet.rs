use std::{str::FromStr, time::Duration};

use cosmic::iced::{Rectangle, Size, event::listen_with};
use tracing::{debug, trace};

use crate::{
    color::AppletColor,
    config::{APP_ID, DEFAULT_TEMPLATE, Flags, SysInfoConfig},
    data, fl, template,
};

pub(crate) fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<SysInfo>(Flags::new())
}

struct SysInfo {
    core: cosmic::app::Core,
    popup: Option<cosmic::iced::window::Id>,
    config: SysInfoConfig,
    config_handler: Option<cosmic::cosmic_config::Config>,
    data: data::Data,
    template: template::Template,
    size: Size,
}

impl SysInfo {
    fn update_template_cache(&mut self) {
        let Ok(template) = template::Template::from_str(&self.config.template);
        self.template = template;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Message {
    Size(Size),
    Tick,
    ToggleWindow,
    PopupClosed(cosmic::iced::window::Id),
    ToggleIncludeSwapWithRam(bool),
    ToggleUseMonoFont(bool),
    TemplateChanged(String),
    RestoreTemplate,
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
                size: Size {
                    width: 10.,
                    height: 10.,
                },
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
        cosmic::iced::Subscription::batch([
            cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick),
            listen_with(|event, _status, id| {
                if let cosmic::iced::Event::Window(
                    cosmic::iced::window::Event::Resized(size)
                    | cosmic::iced::window::Event::Opened { position: _, size },
                ) = event
                    && id == cosmic::iced::window::Id::RESERVED
                {
                    Some(Message::Size(size))
                } else {
                    None
                }
            }),
        ])
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
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
            Message::Tick => self.data.refresh(self.template.requires, &self.config),
            Message::ToggleWindow => {
                if let Some(id) = self.popup.take() {
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }

                let new_id = cosmic::iced::window::Id::unique();
                self.popup.replace(new_id);

                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    new_id,
                    None,
                    None,
                    None,
                );
                popup_settings.positioner.anchor_rect = Rectangle::<i32> {
                    x: 0,
                    y: 0,
                    width: self.size.width as i32,
                    height: self.size.height as i32,
                };

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
            Message::ToggleUseMonoFont(value) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_use_mono_font(handler, value)
                {
                    tracing::error!("failed to toggle `use_mono_font`: {error}")
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
            Message::RestoreTemplate => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self
                        .config
                        .set_template(handler, DEFAULT_TEMPLATE.to_string())
                {
                    tracing::error!("failed to restore template: {error}")
                }
                self.update_template_cache();
            }
            Message::Size(size) => {
                self.size = size;
            }
        }

        cosmic::task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Message> {
        let colors = AppletColor::from_active_theme();

        let content = self
            .template
            .render(&self.data, &colors, self.config.use_mono_font);

        let button = cosmic::widget::button::custom(content)
            .class(cosmic::theme::Button::AppletIcon)
            .on_press_down(Message::ToggleWindow);

        cosmic::widget::autosize::autosize(button, cosmic::widget::Id::unique()).into()
    }

    fn view_window(&self, _id: cosmic::iced::window::Id) -> cosmic::Element<'_, Message> {
        let include_swap_in_ram_toggler = cosmic::widget::row::with_capacity(3)
            .push(cosmic::widget::text(fl!("include-swap-in-ram-toggle")))
            .push(cosmic::widget::space::horizontal())
            .push(
                cosmic::widget::toggler(self.config.include_swap_in_ram)
                    .on_toggle(Message::ToggleIncludeSwapWithRam),
            );

        let use_mono_font_toggler = cosmic::widget::column::with_capacity(2)
            .push(
                cosmic::widget::row::with_capacity(3)
                    .push(cosmic::widget::text(fl!("use-mono-font-toggle")))
                    .push(cosmic::widget::space::horizontal())
                    .push(
                        cosmic::widget::toggler(self.config.use_mono_font)
                            .on_toggle(Message::ToggleUseMonoFont),
                    ),
            )
            .push(cosmic::widget::text::caption(fl!("use-mono-font-helper")))
            .spacing(4);

        let template_input = cosmic::widget::column::with_capacity(3)
            .push(cosmic::widget::text::body(fl!("template-label")))
            .push(
                cosmic::widget::text_input("", &self.config.template)
                    .on_input(Message::TemplateChanged),
            )
            .push(
                cosmic::widget::button::custom(cosmic::widget::text::body(fl!(
                    "restore-template-to-default-btn"
                )))
                .on_press(Message::RestoreTemplate)
                .class(cosmic::theme::Button::Standard),
            )
            .spacing(4);

        let data = cosmic::widget::column::with_capacity(3)
            .push(cosmic::applet::padded_control(include_swap_in_ram_toggler))
            .push(cosmic::applet::padded_control(use_mono_font_toggler))
            .push(cosmic::applet::padded_control(template_input))
            .padding([16, 0]);

        self.core
            .applet
            .popup_container(cosmic::widget::container(data))
            .into()
    }
}

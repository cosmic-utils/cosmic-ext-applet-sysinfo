use std::{str::FromStr, sync::Arc, time::Duration};

use cosmic::widget::text_editor::{Action, Content, Edit, Motion};

use tracing::{debug, trace};

use crate::{
    color::AppletColor,
    config::{APP_ID, DEFAULT_TEMPLATE, Flags, SysInfoConfig},
    data::{self, Data},
    fl,
    template::{Category, Requires, Template, Variable},
};

pub(crate) fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<SysInfo>(Flags::new())
}

struct SysInfo {
    core: cosmic::app::Core,
    size: cosmic::iced::Size,
    popup: Option<cosmic::iced::window::Id>,
    config: SysInfoConfig,
    config_handler: Option<cosmic::cosmic_config::Config>,
    data: data::Data,
    template: Template,
    template_content: Content,
    confirm_reset: bool,
    /// Whether the list of variables is expanded in the popup.
    show_variables: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Message {
    Tick,
    Size(cosmic::iced::Size),
    ToggleWindow,
    PopupClosed(cosmic::iced::window::Id),
    ToggleIncludeSwapWithRam(bool),
    ToggleUseMonoFont(bool),
    TemplateEdited(Action),
    ToggleVariables,
    InsertVariable(Variable),
    ResetTemplate,
    ConfirmReset(bool),
    OpenMonitor,
}

impl SysInfo {
    /// The variables to refresh: those in the template, plus every local one while the
    /// variable list is shown, so that their current values can be displayed.
    fn requires(&self) -> Requires {
        if self.popup.is_some() && self.show_variables {
            self.template.requires.union(Requires::local())
        } else {
            self.template.requires
        }
    }

    fn edit_template(&mut self, action: Action) {
        // Only re-sync/re-persist on actual edits; navigation (arrow keys, cursor moves)
        // shouldn't rewrite the config
        let is_edit = action.is_edit();
        self.template_content.perform(action);
        if !is_edit {
            return;
        }

        if let Some(handler) = &self.config_handler {
            let value = self.template_content.text();
            match Template::from_str(&value) {
                Ok(parsed) => {
                    if let Err(error) = self.config.set_template(handler, value) {
                        tracing::error!("failed to set template: {error}");
                    } else {
                        self.template = parsed;
                    }
                }
                Err(error) => {
                    tracing::error!("invalid template: {error}");
                }
            }
        }
    }

    /// The available variables grouped by category, each showing its current value.
    fn variable_list(&self, colors: &AppletColor) -> cosmic::Element<'_, Message> {
        let requires = self.requires();
        let mut list = cosmic::widget::column::with_capacity(Category::ALL.len()).spacing(8);

        for category in Category::ALL {
            let chips: Vec<cosmic::Element<'_, Message>> = Variable::ALL
                .into_iter()
                .filter(|var| var.category() == category)
                .map(|var| {
                    let mut label = cosmic::widget::row::with_capacity(2)
                        .push(cosmic::widget::text::body(var.name()))
                        .spacing(6)
                        .align_y(cosmic::iced::Alignment::Center);
                    // public IPs are only fetched when the template uses them
                    if requires.contains(var) {
                        let (value, color) = var.resolve(&self.data, colors);
                        let value = cosmic::widget::text::caption(value.trim().to_owned());
                        label = label.push(match color {
                            Some(color) => value.class(cosmic::theme::Text::Color(color)),
                            None => value,
                        });
                    }

                    cosmic::widget::button::custom(label)
                        .padding([4, 8])
                        .class(cosmic::theme::Button::Standard)
                        .on_press(Message::InsertVariable(var))
                        .into()
                })
                .collect();

            list = list.push(
                cosmic::widget::column::with_capacity(2)
                    .push(cosmic::widget::text::heading(category_label(category)))
                    .push(cosmic::widget::flex_row(chips).spacing(4))
                    .spacing(4),
            );
        }

        cosmic::widget::scrollable(list).height(260).into()
    }
}

/// An editor holding `text`, with the cursor at the end, so that inserted variables are
/// appended unless the user places the cursor elsewhere.
fn template_content(text: &str) -> Content {
    let mut content = Content::with_text(text);
    content.perform(Action::Move(Motion::DocumentEnd));
    content
}

/// The characters right before and after the cursor, within its line.
fn chars_around_cursor(content: &Content) -> (Option<char>, Option<char>) {
    let position = content.cursor().position;
    let Some(line) = content.line(position.line) else {
        return (None, None);
    };
    let text = line.text.as_ref();
    // the column is a byte offset
    let column = position.column.min(text.len());
    if !text.is_char_boundary(column) {
        return (None, None);
    }
    let (before, after) = text.split_at(column);
    (before.chars().next_back(), after.chars().next())
}

fn category_label(category: Category) -> String {
    match category {
        Category::Cpu => fl!("category-cpu"),
        Category::Memory => fl!("category-memory"),
        Category::Gpu => fl!("category-gpu"),
        Category::Network => fl!("category-network"),
        Category::Disk => fl!("category-disk"),
        Category::Npu => fl!("category-npu"),
    }
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
        let data = Data::new(&config);
        let Ok(template) = Template::from_str(&config.template);
        let template_content = template_content(&config.template);

        (
            Self {
                core,
                popup: None,
                config,
                config_handler: flags.config_handler,
                data,
                template,
                template_content,
                size: cosmic::iced::Size {
                    width: 10.,
                    height: 10.,
                },
                confirm_reset: false,
                show_variables: false,
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
            cosmic::iced::event::listen_with(|event, _status, id| {
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
            Message::Tick => self.data.refresh(self.requires(), &self.config),
            Message::Size(size) => {
                self.size = size;
            }
            Message::ToggleWindow => {
                if let Some(id) = self.popup.take() {
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }

                let new_id = cosmic::iced::window::Id::unique();
                self.popup.replace(new_id);

                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core
                        .main_window_id()
                        .expect("applet should always have a main window"),
                    new_id,
                    None,
                    None,
                    None,
                );
                popup_settings.positioner.anchor_rect = cosmic::iced::Rectangle::<i32> {
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
                if self.popup.take_if(|stored_id| stored_id == &id).is_some() {
                    self.show_variables = false;
                }
            }
            Message::ToggleIncludeSwapWithRam(value) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_include_swap_in_ram(handler, value)
                {
                    tracing::error!("failed to toggle `include_swap_in_ram`: {error}")
                }
            }
            Message::ToggleUseMonoFont(value) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_use_mono_font(handler, value)
                {
                    tracing::error!("failed to toggle `use_mono_font`: {error}")
                }
            }
            Message::TemplateEdited(action) => self.edit_template(action),
            Message::ToggleVariables => {
                self.show_variables = !self.show_variables;
                if self.show_variables {
                    // fill in the values shown next to the variables right away
                    self.data.refresh(self.requires(), &self.config);
                }
            }
            Message::InsertVariable(var) => {
                let (before, after) = chars_around_cursor(&self.template_content);
                let text = var.insertion(before, after);
                self.edit_template(Action::Edit(Edit::Paste(Arc::new(text))));
            }
            Message::ResetTemplate => {
                self.confirm_reset = true;
            }
            Message::ConfirmReset(true) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self
                        .config
                        .set_template(handler, DEFAULT_TEMPLATE.to_string())
                {
                    tracing::error!("failed to restore template: {error}");
                } else {
                    self.template = Template::from_str(DEFAULT_TEMPLATE)
                        .expect("DEFAULT_TEMPLATE should always be a valid template");
                    self.template_content = template_content(DEFAULT_TEMPLATE);
                    self.confirm_reset = false;
                }
            }
            Message::ConfirmReset(false) => {
                self.confirm_reset = false;
            }
            Message::OpenMonitor => match std::process::Command::new("cosmic-monitor").spawn() {
                Ok(_) => tracing::info!("Launched `cosmic-monitor`"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    tracing::warn!("`cosmic-monitor` binary not found on PATH");
                }
                Err(error) => tracing::error!("Failed to launch `cosmic-monitor`: {error}"),
            },
        }

        cosmic::task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Message> {
        let colors = AppletColor::new();

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
                cosmic::widget::text_editor::text_editor(&self.template_content)
                    .on_action(Message::TemplateEdited)
                    .min_height(40.0)
                    .max_height(300.0)
                    .padding([8, 12]),
            );

        let reset_button: cosmic::Element<'_, Message> = if self.confirm_reset {
            cosmic::widget::column::with_capacity(3)
                .push(cosmic::widget::text::body(fl!("reset-template-confirm")))
                .push(
                    cosmic::widget::row::with_capacity(2)
                        .push(
                            cosmic::widget::button::custom(cosmic::widget::text::body(fl!(
                                "reset-confirm-yes"
                            )))
                            .on_press(Message::ConfirmReset(true))
                            .class(cosmic::theme::Button::Destructive),
                        )
                        .push(
                            cosmic::widget::button::custom(cosmic::widget::text::body(fl!(
                                "reset-confirm-no"
                            )))
                            .on_press(Message::ConfirmReset(false))
                            .class(cosmic::theme::Button::Standard),
                        )
                        .spacing(8),
                )
                .spacing(8)
                .into()
        } else {
            cosmic::widget::button::custom(cosmic::widget::text::body(fl!(
                "reset-template-to-default-button"
            )))
            .on_press(Message::ResetTemplate)
            .class(cosmic::theme::Button::Standard)
            .into()
        };

        let colors = AppletColor::new();
        let unknown: Vec<&str> = self.template.unknown_variables().collect();
        let template_input = if unknown.is_empty() {
            template_input
        } else {
            let names = unknown
                .iter()
                .map(|name| format!("{{{name}}}"))
                .collect::<Vec<_>>()
                .join(", ");
            template_input.push(
                cosmic::widget::text::caption(fl!(
                    "unknown-variables",
                    count = unknown.len(),
                    names = names
                ))
                .class(cosmic::theme::Text::Color(colors.critical)),
            )
        };

        let toggle_variables = cosmic::widget::button::text(if self.show_variables {
            fl!("hide-variables-button")
        } else {
            fl!("insert-variable-button")
        })
        .trailing_icon(cosmic::widget::icon::from_name(if self.show_variables {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        }))
        .on_press(Message::ToggleVariables);

        let mut template_input = template_input.push(toggle_variables);
        if self.show_variables {
            template_input = template_input.push(self.variable_list(&colors));
        }

        let template_input = template_input.push(reset_button).spacing(4);

        let mut data = cosmic::widget::column::with_capacity(4)
            .push(cosmic::applet::padded_control(include_swap_in_ram_toggler))
            .push(cosmic::applet::padded_control(use_mono_font_toggler))
            .push(cosmic::applet::padded_control(template_input))
            .padding([16, 0]);

        if std::env::var_os("PATH").is_some_and(|paths| {
            std::env::split_paths(&paths).any(|dir| dir.join("cosmic-monitor").exists())
        }) {
            let open_monitor_button = cosmic::widget::button::text(fl!("open-monitor-menu-item"))
                .on_press(Message::OpenMonitor);
            data = data.push(cosmic::applet::padded_control(open_monitor_button));
        }

        self.core
            .applet
            .popup_container(cosmic::widget::container(data))
            .into()
    }
}

#[cfg(test)]
mod test {
    use cosmic::widget::text_editor::{Cursor, Position};

    use super::*;

    fn at(content: &mut Content, line: usize, column: usize) {
        content.move_to(Cursor {
            position: Position { line, column },
            selection: None,
        });
    }

    #[test]
    fn new_content_has_cursor_at_end() {
        let content = template_content("CPU {cpu_usage}\nRAM");
        assert_eq!(content.cursor().position, Position { line: 1, column: 3 });
        assert_eq!(chars_around_cursor(&content), (Some('M'), None));
    }

    #[test]
    fn chars_around_cursor_handle_multibyte_text() {
        // `↓` is 3 bytes long, and the column is a byte offset
        let mut content = template_content("↓{dl_speed}");
        at(&mut content, 0, '↓'.len_utf8());
        assert_eq!(chars_around_cursor(&content), (Some('↓'), Some('{')));

        at(&mut content, 0, 0);
        assert_eq!(chars_around_cursor(&content), (None, Some('↓')));
    }

    #[test]
    fn inserting_a_variable_at_the_cursor() {
        let mut content = template_content("CPU RAM");
        at(&mut content, 0, 3);
        let (before, after) = chars_around_cursor(&content);
        let text = Variable::CpuUsage.insertion(before, after);
        content.perform(Action::Edit(Edit::Paste(Arc::new(text))));
        assert_eq!(content.text(), "CPU {cpu_usage} RAM");

        let mut content = template_content("CPU");
        let (before, after) = chars_around_cursor(&content);
        let text = Variable::CpuUsage.insertion(before, after);
        content.perform(Action::Edit(Edit::Paste(Arc::new(text))));
        assert_eq!(content.text(), "CPU {cpu_usage}");
    }
}

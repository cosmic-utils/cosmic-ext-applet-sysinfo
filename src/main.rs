mod applet;
mod config;
mod i18n;
mod template;

fn main() -> cosmic::iced::Result {
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    applet::run()
}

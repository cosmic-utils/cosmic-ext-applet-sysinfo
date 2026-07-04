mod applet;
mod config;
mod data;
mod i18n;
mod template;

fn main() -> cosmic::iced::Result {
    // Initialize logging, honouring `RUST_LOG` when set (e.g. RUST_LOG=debug)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    tracing::info!("Starting sysinfo applet");

    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    i18n::init(&requested_languages);

    applet::run()
}

fn main() -> cosmic::iced::Result {
    tracing_subscriber::fmt::init();
    let _ = tracing_log::LogTracer::init();

    tracing::info!("Starting sysinfo applet");

    cosmic_ext_applet_sysinfo::run()
}

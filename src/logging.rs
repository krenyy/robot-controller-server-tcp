pub(crate) fn set_up() {
    #[cfg(debug_assertions)]
    let level = tracing::Level::TRACE;

    #[cfg(not(debug_assertions))]
    let level = tracing::Level::INFO;

    let subscriber = tracing_subscriber::fmt().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    tracing::debug!("logging set-up!")
}

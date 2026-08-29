#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod error;
mod i18n;
mod model;
mod platform;
mod services;
mod storage;
mod updater;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let silent_startup = args
        .iter()
        .any(|argument| argument == platform::startup::SILENT_STARTUP_ARG);

    if args.iter().any(|argument| argument == "--compact-storage") {
        let storage = match storage::StorageHandle::new() {
            Ok(s) => s,
            Err(error) => {
                eprintln!("Failed to open database: {error}");
                std::process::exit(1);
            }
        };
        if let Err(error) = storage::compact_database(&storage) {
            eprintln!("Failed to compact storage: {error}");
            std::process::exit(1);
        }
        return;
    }

    if args.iter().any(|argument| argument == "--quit") {
        #[cfg(windows)]
        platform::single_instance::notify_existing_instance_to_quit();
        return;
    }

    #[cfg(windows)]
    let _single_instance = match platform::single_instance::acquire() {
        platform::single_instance::SingleInstance::Primary(guard) => {
            platform::single_instance::start_activation_listener();
            Some(guard)
        }
        platform::single_instance::SingleInstance::AlreadyRunning => {
            if !silent_startup {
                platform::single_instance::notify_existing_instance();
            }
            return;
        }
        platform::single_instance::SingleInstance::Unavailable => None,
    };

    app::run(!silent_startup);

    // GPUI owns detached platform threads that can outlive the event loop.
    // Ensure an installer never sees a stale UCP process after the UI quits.
    #[cfg(windows)]
    std::process::exit(0);
}

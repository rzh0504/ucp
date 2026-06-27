#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod clipboard_watcher;
mod components;
mod i18n;
mod model;
mod platform;
mod storage;

use app::App;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder, WindowCloseBehaviour};

const APP_ICON_BYTES: &[u8] = include_bytes!("../assets/icons/Ucp.png");
const APP_DIR: &str = "UCP Clipboard";

fn main() {
    if std::env::args().any(|argument| argument == "--compact-storage") {
        if let Err(error) = storage::compact_database() {
            eprintln!("Failed to compact storage: {error}");
            std::process::exit(1);
        }
        return;
    }

    if std::env::args().any(|argument| argument == "--quit") {
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
            platform::single_instance::notify_existing_instance();
            return;
        }
        platform::single_instance::SingleInstance::Unavailable => None,
    };

    let mut config = Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title("UCP Clipboard")
                .with_window_icon(dioxus::desktop::icon_from_memory(APP_ICON_BYTES).ok())
                .with_decorations(false)
                .with_transparent(true)
                .with_inner_size(LogicalSize::new(900.0, 660.0))
                .with_min_inner_size(LogicalSize::new(860.0, 620.0)),
        )
        .with_menu(None)
        .with_close_behaviour(WindowCloseBehaviour::WindowHides)
        .with_disable_context_menu(true)
        .with_custom_head(app::style_head())
        .with_background_color((0, 0, 0, 0));

    #[cfg(windows)]
    {
        config = config.with_data_directory(webview_data_directory());
    }

    dioxus::LaunchBuilder::new().with_cfg(config).launch(App);
}

#[cfg(windows)]
fn webview_data_directory() -> std::path::PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(APP_DIR)
        .join("WebView2")
}

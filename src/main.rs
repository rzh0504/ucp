#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod components;
mod model;
mod platform;

use app::App;
use dioxus::desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    dioxus::LaunchBuilder::new()
        .with_cfg(
            Config::new()
                .with_window(
                    WindowBuilder::new()
                        .with_title("UCP Clipboard")
                        .with_inner_size(LogicalSize::new(1006.0, 754.0))
                        .with_min_inner_size(LogicalSize::new(860.0, 620.0)),
                )
                .with_disable_context_menu(true)
                .with_background_color((246, 247, 251, 255)),
        )
        .launch(App);
}

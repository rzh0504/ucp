mod filter_tabs;
mod history_list;
mod icons;
mod settings_page;
mod tabs;
mod top_bar;

pub use history_list::HistoryList;
pub use icons::{AppIcon, Icon};
pub use settings_page::SettingsPage;
pub use top_bar::TopBar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppPage {
    History,
    Settings,
}

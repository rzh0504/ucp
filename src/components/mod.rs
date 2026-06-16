mod filter_tabs;
mod floating_settings_button;
mod history_list;
mod icons;
mod settings_page;
mod tabs;
mod top_bar;

pub use floating_settings_button::FloatingSettingsButton;
pub use history_list::HistoryList;
pub use settings_page::SettingsPage;
pub use top_bar::TopBar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppPage {
    History,
    Settings,
}

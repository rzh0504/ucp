use crate::components::AppPage;
use crate::model::ClipboardFilter;
use dioxus::events::MountedData;
use dioxus::html::Key;
use dioxus::prelude::*;
use std::rc::Rc;

/// Handle keyboard shortcuts for the main shell
pub fn handle_keyboard_shortcuts(
    key: &Key,
    modifiers_ctrl: bool,
    modifiers_meta: bool,
    keyboard_shortcuts_enabled: bool,
    mut active_page: Signal<AppPage>,
    mut active_filter: Signal<ClipboardFilter>,
    mut query: Signal<String>,
    mut debounced_query: Signal<String>,
    search_input: Signal<Option<Rc<MountedData>>>,
    shell: Signal<Option<Rc<MountedData>>>,
) -> bool {
    if !keyboard_shortcuts_enabled {
        return false;
    }

    let primary = modifiers_ctrl || modifiers_meta;

    // Ctrl/Cmd + F: Focus search
    if primary && matches!(key, Key::Character(k) if k.eq_ignore_ascii_case("f")) {
        active_page.set(AppPage::History);
        if let Some(input) = search_input.read().clone() {
            spawn(async move {
                let _ = input.set_focus(true).await;
            });
        }
        return true;
    }

    // Ctrl/Cmd + ,: Toggle settings
    if primary && matches!(key, Key::Character(k) if k == ",") {
        active_page.set(if active_page() == AppPage::Settings {
            AppPage::History
        } else {
            AppPage::Settings
        });
        return true;
    }

    // Ctrl/Cmd + 1-5: Filter shortcuts
    if primary {
        if let Some(filter) = filter_shortcut(key) {
            active_page.set(AppPage::History);
            active_filter.set(filter);
            return true;
        }
    }

    // Escape: Close settings or clear search
    if *key == Key::Escape {
        if active_page() == AppPage::Settings {
            active_page.set(AppPage::History);
            return true;
        } else if !query.read().is_empty() || !debounced_query.read().is_empty() {
            query.set(String::new());
            debounced_query.set(String::new());
            if let Some(element) = shell.read().clone() {
                spawn(async move {
                    let _ = element.set_focus(true).await;
                });
            }
            return true;
        }
    }

    false
}

fn filter_shortcut(key: &Key) -> Option<ClipboardFilter> {
    match key {
        Key::Character(key) if key == "1" => Some(ClipboardFilter::All),
        Key::Character(key) if key == "2" => Some(ClipboardFilter::Text),
        Key::Character(key) if key == "3" => Some(ClipboardFilter::Image),
        Key::Character(key) if key == "4" => Some(ClipboardFilter::File),
        Key::Character(key) if key == "5" => Some(ClipboardFilter::Favorite),
        _ => None,
    }
}

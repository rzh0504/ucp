use super::{StorageError, with_connection};
use crate::model::{AppLanguage, AppSettings, AppTheme};
use rusqlite::params;

pub fn load_settings() -> Result<AppSettings, StorageError> {
    with_connection(|connection| {
        let mut settings = AppSettings::default();
        let mut statement = connection.prepare("SELECT key, value FROM app_settings")?;

        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (key, value) = row?;
            match key.as_str() {
                "history_limit" => {
                    settings.history_limit = value
                        .parse::<usize>()
                        .unwrap_or(AppSettings::default().history_limit)
                }
                "auto_cleanup_days" => settings.auto_cleanup_days = parse_auto_cleanup_days(&value),
                "language" => settings.language = AppLanguage::from_key(&value),
                "theme" => settings.theme = AppTheme::from_key(&value),
                "launch_at_startup" => settings.launch_at_startup = parse_bool(&value),
                "desktop_widget" => settings.desktop_widget = parse_bool(&value),
                "desktop_widget_topmost" => settings.desktop_widget_topmost = parse_bool(&value),
                "keyboard_shortcuts" => settings.keyboard_shortcuts = parse_bool(&value),
                "global_show_shortcut" => settings.global_show_shortcut = value,
                "auto_focus_history" => settings.auto_focus_history = parse_bool(&value),
                "promote_copied_entries" => settings.promote_copied_entries = parse_bool(&value),
                "quick_paste" => settings.quick_paste = parse_bool(&value),
                "hide_after_copy" => settings.hide_after_copy = parse_bool(&value),
                "show_copy_time" => settings.show_copy_time = parse_bool(&value),
                "show_text_length" => settings.show_text_length = parse_bool(&value),
                "background_opacity" => {
                    settings.background_opacity = value
                        .parse::<u8>()
                        .unwrap_or(AppSettings::default().background_opacity)
                }
                _ => {}
            }
        }

        Ok(settings.normalized())
    })
}

pub fn save_settings(settings: &AppSettings) -> Result<(), StorageError> {
    with_connection(|connection| {
        let transaction = connection.transaction()?;
        let values = [
            ("history_limit", settings.history_limit.to_string()),
            (
                "auto_cleanup_days",
                settings
                    .auto_cleanup_days
                    .map(|days| days.to_string())
                    .unwrap_or_else(|| "none".to_string()),
            ),
            ("language", settings.language.key().to_string()),
            ("theme", settings.theme.key().to_string()),
            ("launch_at_startup", settings.launch_at_startup.to_string()),
            ("desktop_widget", settings.desktop_widget.to_string()),
            (
                "desktop_widget_topmost",
                settings.desktop_widget_topmost.to_string(),
            ),
            (
                "keyboard_shortcuts",
                settings.keyboard_shortcuts.to_string(),
            ),
            (
                "global_show_shortcut",
                settings.global_show_shortcut.clone(),
            ),
            (
                "auto_focus_history",
                settings.auto_focus_history.to_string(),
            ),
            (
                "promote_copied_entries",
                settings.promote_copied_entries.to_string(),
            ),
            ("quick_paste", settings.quick_paste.to_string()),
            ("hide_after_copy", settings.hide_after_copy.to_string()),
            ("show_copy_time", settings.show_copy_time.to_string()),
            ("show_text_length", settings.show_text_length.to_string()),
            (
                "background_opacity",
                settings.background_opacity.to_string(),
            ),
        ];

        for (key, value) in values {
            transaction.execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }

        transaction.commit()?;
        Ok(())
    })
}

fn parse_bool(value: &str) -> bool {
    matches!(value, "true" | "1" | "yes" | "on")
}

fn parse_auto_cleanup_days(value: &str) -> Option<u16> {
    match value {
        "7" => Some(7),
        "30" => Some(30),
        "60" => Some(60),
        _ => None,
    }
}

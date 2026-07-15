mod image;

pub use image::ClipboardImage;

use chrono::{DateTime, Duration as ChronoDuration, Local};

pub const DEFAULT_HISTORY_LIMIT: usize = 200;
pub const DEFAULT_BACKGROUND_OPACITY: u8 = 100;
pub const DEFAULT_GLOBAL_SHOW_SHORTCUT: &str = "Ctrl+Shift+V";
pub const MIN_BACKGROUND_OPACITY: u8 = 45;
pub const HISTORY_LIMIT_OPTIONS: [usize; 5] = [50, 100, 200, 500, 1000];
pub const AUTO_CLEANUP_DAY_OPTIONS: [Option<u16>; 4] = [Some(7), Some(30), Some(60), None];

const TEXT_LIST_PREVIEW_CHAR_LIMIT: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppLanguage {
    Chinese,
    English,
}

impl AppLanguage {
    pub const OPTIONS: [Self; 2] = [Self::Chinese, Self::English];

    pub fn key(self) -> &'static str {
        match self {
            Self::Chinese => "zh-CN",
            Self::English => "en-US",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chinese => "简体中文",
            Self::English => "English",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "en" | "en-US" => Self::English,
            _ => Self::Chinese,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppTheme {
    System,
    Light,
    Dark,
}

impl AppTheme {
    pub const OPTIONS: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn label(self, language: AppLanguage) -> &'static str {
        match (self, language) {
            (Self::System, AppLanguage::Chinese) => "跟随设备",
            (Self::Light, AppLanguage::Chinese) => "浅色",
            (Self::Dark, AppLanguage::Chinese) => "深色",
            (Self::System, AppLanguage::English) => "Use device setting",
            (Self::Light, AppLanguage::English) => "Light",
            (Self::Dark, AppLanguage::English) => "Dark",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "dark" => Self::Dark,
            "light" => Self::Light,
            _ => Self::System,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Image,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardContent {
    Text(String),
    Image(ClipboardImage),
    Files(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppSettings {
    pub history_limit: usize,
    pub auto_cleanup_days: Option<u16>,
    pub preserve_favorites_on_delete: bool,
    pub language: AppLanguage,
    pub theme: AppTheme,
    pub launch_at_startup: bool,
    pub desktop_widget: bool,
    pub desktop_widget_topmost: bool,
    pub keyboard_shortcuts: bool,
    pub global_show_shortcut: String,
    pub auto_focus_history: bool,
    pub promote_copied_entries: bool,
    pub quick_paste: bool,
    pub hide_after_copy: bool,
    pub show_copy_time: bool,
    pub show_text_length: bool,
    pub background_opacity: u8,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            history_limit: DEFAULT_HISTORY_LIMIT,
            auto_cleanup_days: None,
            preserve_favorites_on_delete: false,
            language: AppLanguage::Chinese,
            theme: AppTheme::System,
            launch_at_startup: false,
            desktop_widget: false,
            desktop_widget_topmost: false,
            keyboard_shortcuts: true,
            global_show_shortcut: DEFAULT_GLOBAL_SHOW_SHORTCUT.to_string(),
            auto_focus_history: true,
            promote_copied_entries: true,
            quick_paste: true,
            hide_after_copy: false,
            show_copy_time: true,
            show_text_length: true,
            background_opacity: DEFAULT_BACKGROUND_OPACITY,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        if !HISTORY_LIMIT_OPTIONS.contains(&self.history_limit) {
            self.history_limit = DEFAULT_HISTORY_LIMIT;
        }
        if !AUTO_CLEANUP_DAY_OPTIONS.contains(&self.auto_cleanup_days) {
            self.auto_cleanup_days = None;
        }
        self.background_opacity = self
            .background_opacity
            .clamp(MIN_BACKGROUND_OPACITY, DEFAULT_BACKGROUND_OPACITY);
        self.global_show_shortcut = normalized_global_shortcut(self.global_show_shortcut);
        self
    }
}

fn normalized_global_shortcut(shortcut: String) -> String {
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        DEFAULT_GLOBAL_SHOW_SHORTCUT.to_string()
    } else {
        shortcut.to_string()
    }
}

impl ClipboardContent {
    pub fn kind(&self) -> ClipboardKind {
        match self {
            Self::Text(_) => ClipboardKind::Text,
            Self::Image(_) => ClipboardKind::Image,
            Self::Files(_) => ClipboardKind::File,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(text) => text.trim().is_empty(),
            Self::Image(image) => image.width == 0 || image.height == 0,
            Self::Files(files) => files.is_empty(),
        }
    }

    pub fn normalized(self) -> Self {
        match self {
            Self::Text(text) => Self::Text(text),
            Self::Files(files) => Self::Files(
                files
                    .into_iter()
                    .map(|file| file.trim().to_string())
                    .filter(|file| !file.is_empty())
                    .collect(),
            ),
            other => other,
        }
    }

    pub fn title_with_language(&self, language: AppLanguage) -> String {
        match self {
            Self::Text(text) => text_title(text),
            Self::Image(_) => crate::i18n::tr(language).image.to_string(),
            Self::Files(files) => {
                if files.len() == 1 {
                    files[0].clone()
                } else {
                    crate::i18n::file_count(language, files.len())
                }
            }
        }
    }

    pub fn size_label_with_language(&self, language: AppLanguage) -> String {
        match self {
            Self::Text(text) => crate::i18n::character_count(language, text.chars().count()),
            Self::Image(image) => image
                .bytes
                .as_ref()
                .map(|bytes| format_bytes(bytes.len()))
                .unwrap_or_else(|| format!("{} x {}", image.width, image.height)),
            Self::Files(files) => crate::i18n::file_count(language, files.len()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardFilter {
    All,
    Text,
    Image,
    File,
    Favorite,
}

impl ClipboardFilter {
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Text => "text",
            Self::Image => "image",
            Self::File => "file",
            Self::Favorite => "favorite",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "text" => Self::Text,
            "image" => Self::Image,
            "file" => Self::File,
            "favorite" => Self::Favorite,
            _ => Self::All,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardEntry {
    pub id: u64,
    pub content: ClipboardContent,
    pub captured_at: DateTime<Local>,
    pub pinned: bool,
    pub favorite: bool,
}

impl ClipboardEntry {
    pub fn new(id: u64, content: ClipboardContent) -> Self {
        Self {
            id,
            content,
            captured_at: Local::now(),
            pinned: false,
            favorite: false,
        }
    }

    pub fn kind(&self) -> ClipboardKind {
        self.content.kind()
    }

    pub fn is_text(&self) -> bool {
        self.kind() == ClipboardKind::Text
    }

    pub fn title_with_language(&self, language: AppLanguage) -> String {
        self.content.title_with_language(language)
    }

    pub fn size_label_with_language(&self, language: AppLanguage) -> String {
        self.content.size_label_with_language(language)
    }

    pub fn age_label_with_language(&self, language: AppLanguage) -> String {
        let elapsed = Local::now().signed_duration_since(self.captured_at);
        let seconds = elapsed.num_seconds().max(0);

        if seconds < 60 {
            crate::i18n::tr(language).just_now.to_string()
        } else if seconds < 3_600 {
            crate::i18n::age_minutes(language, seconds / 60)
        } else if seconds < 86_400 {
            crate::i18n::age_hours(language, seconds / 3_600)
        } else {
            crate::i18n::age_days(language, seconds / 86_400)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryCounts {
    pub total: usize,
    pub text: usize,
    pub image: usize,
    pub file: usize,
    pub favorite: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardHistory {
    capacity: usize,
    next_id: u64,
    entries: Vec<ClipboardEntry>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PushResult {
    pub changed: bool,
    pub entry: Option<ClipboardEntry>,
    pub removed_ids: Vec<u64>,
}

impl ClipboardHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            next_id: 1,
            entries: Vec::new(),
        }
    }

    pub fn from_entries(capacity: usize, entries: Vec<ClipboardEntry>) -> Self {
        let next_id = entries
            .iter()
            .map(|entry| entry.id)
            .max()
            .unwrap_or_default()
            + 1;

        let mut history = Self {
            capacity,
            next_id,
            entries,
        };
        history.sort_entries();
        history.truncate();
        history
    }

    pub fn push(&mut self, content: ClipboardContent) -> PushResult {
        let content = content.normalized();
        if content.is_empty() {
            return PushResult::default();
        }

        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.content == content)
        {
            if !self.should_promote_position(position) {
                return PushResult::default();
            }

            let mut entry = self.entries.remove(position);
            entry.captured_at = Local::now();
            let updated_entry = entry.clone();
            self.entries.insert(0, entry);
            return PushResult {
                changed: true,
                entry: Some(updated_entry),
                removed_ids: Vec::new(),
            };
        }

        let entry = ClipboardEntry::new(self.next_id, content);
        let inserted_entry = entry.clone();
        self.next_id += 1;
        self.entries.insert(0, entry);
        let removed_ids = self.truncate();

        PushResult {
            changed: true,
            entry: Some(inserted_entry),
            removed_ids,
        }
    }

    pub fn would_push_change(&self, content: &ClipboardContent) -> bool {
        let content = content.clone().normalized();
        if content.is_empty() {
            return false;
        }

        self.entries
            .iter()
            .position(|entry| entry.content == content)
            .is_none_or(|position| self.should_promote_position(position))
    }

    pub fn filtered(&self, query: &str, filter: ClipboardFilter) -> Vec<ClipboardEntry> {
        let normalized_query = query.trim().to_lowercase();
        let mut entries = self
            .entries
            .iter()
            .filter(|entry| matches_filter(*entry, filter))
            .filter(|entry| {
                if normalized_query.is_empty() {
                    return true;
                }

                if entry.kind() == ClipboardKind::Image {
                    return false;
                }

                content_matches_query(&entry.content, normalized_query.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        sort_entries(&mut entries);
        entries
    }

    pub fn deletable_ids(&self, ids: &[u64], preserve_favorites: bool) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|entry| ids.contains(&entry.id) && (!preserve_favorites || !entry.favorite))
            .map(|entry| entry.id)
            .collect()
    }

    pub fn deletable_ids_for_filter(
        &self,
        filter: ClipboardFilter,
        preserve_favorites: bool,
    ) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|entry| {
                matches_filter(entry, filter) && (!preserve_favorites || !entry.favorite)
            })
            .map(|entry| entry.id)
            .collect()
    }

    pub fn counts(&self) -> HistoryCounts {
        let mut counts = HistoryCounts {
            total: self.entries.len(),
            ..HistoryCounts::default()
        };

        for entry in &self.entries {
            match entry.kind() {
                ClipboardKind::Text => counts.text += 1,
                ClipboardKind::Image => counts.image += 1,
                ClipboardKind::File => counts.file += 1,
            }

            if entry.favorite {
                counts.favorite += 1;
            }
        }

        counts
    }

    pub fn entry(&self, id: u64) -> Option<&ClipboardEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn should_promote(&self, id: u64) -> bool {
        self.entries
            .iter()
            .position(|entry| entry.id == id)
            .is_some_and(|position| self.should_promote_position(position))
    }

    pub fn promote(&mut self, id: u64) -> Option<ClipboardEntry> {
        if let Some(position) = self.entries.iter().position(|entry| entry.id == id) {
            if !self.should_promote_position(position) {
                return None;
            }

            let mut entry = self.entries.remove(position);
            entry.captured_at = Local::now();
            let updated_entry = entry.clone();
            self.entries.insert(0, entry);
            Some(updated_entry)
        } else {
            None
        }
    }

    pub fn toggle_favorite(&mut self, id: u64) -> Option<ClipboardEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .map(|entry| {
                entry.favorite = !entry.favorite;
                entry.clone()
            })
    }

    pub fn toggle_pin(&mut self, id: u64) -> Option<ClipboardEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .map(|entry| {
                entry.pinned = !entry.pinned;
                entry.clone()
            })
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn remove_older_than_days(&mut self, days: u16, preserve_favorites: bool) -> usize {
        let cutoff = Local::now() - ChronoDuration::days(i64::from(days));
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.captured_at >= cutoff || (preserve_favorites && entry.favorite));
        before - self.entries.len()
    }

    pub fn set_capacity(&mut self, capacity: usize) -> Vec<u64> {
        self.capacity = capacity.max(1);
        self.truncate()
    }

    fn sort_entries(&mut self) {
        sort_entries(&mut self.entries);
    }

    fn should_promote_position(&self, position: usize) -> bool {
        position != 0 && !self.entries[position].pinned
    }

    fn truncate(&mut self) -> Vec<u64> {
        let mut removed_ids = Vec::new();

        while self.entries.len() > self.capacity {
            let Some(position) = self
                .entries
                .iter()
                .rposition(|entry| !entry.pinned && !entry.favorite)
            else {
                break;
            };

            removed_ids.push(self.entries.remove(position).id);
        }

        removed_ids
    }
}

fn matches_filter(entry: &ClipboardEntry, filter: ClipboardFilter) -> bool {
    match filter {
        ClipboardFilter::All => true,
        ClipboardFilter::Text => entry.kind() == ClipboardKind::Text,
        ClipboardFilter::Image => entry.kind() == ClipboardKind::Image,
        ClipboardFilter::File => entry.kind() == ClipboardKind::File,
        ClipboardFilter::Favorite => entry.favorite,
    }
}

fn content_matches_query(content: &ClipboardContent, normalized_query: &str) -> bool {
    match content {
        ClipboardContent::Text(text) => text.to_lowercase().contains(normalized_query),
        ClipboardContent::Files(files) => files
            .iter()
            .any(|file| file.to_lowercase().contains(normalized_query)),
        ClipboardContent::Image(_) => false,
    }
}

fn sort_entries(entries: &mut [ClipboardEntry]) {
    entries.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.captured_at.cmp(&left.captured_at))
            .then_with(|| right.id.cmp(&left.id))
    });
}

fn text_title(text: &str) -> String {
    let (preview, is_oversized) = text_prefix(text, TEXT_LIST_PREVIEW_CHAR_LIMIT);
    if !is_oversized {
        return text.to_string();
    }

    format!("{preview}...")
}

fn text_prefix(text: &str, char_limit: usize) -> (&str, bool) {
    let Some((cutoff, _)) = text.char_indices().nth(char_limit) else {
        return (text, false);
    };

    (&text[..cutoff], true)
}

fn format_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;

    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / KIB)
    } else {
        format!("{:.1} MiB", bytes as f64 / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const TEXT_CONTENT_CHAR_LIMIT: usize = 200_000;

    #[test]
    fn duplicate_text_is_not_duplicated() {
        let mut history = ClipboardHistory::new(10);

        let first = history.push(ClipboardContent::Text("hello".to_string()));
        let duplicate = history.push(ClipboardContent::Text("hello".to_string()));

        assert!(first.changed);
        assert!(!duplicate.changed);
        assert!(duplicate.entry.is_none());
        assert_eq!(history.counts().total, 1);
        assert_eq!(history.counts().text, 1);
    }

    #[test]
    fn text_content_is_preserved_before_saving_to_history() {
        let mut history = ClipboardHistory::new(10);
        let text = format!("\n  {}tail  \n", "a".repeat(TEXT_CONTENT_CHAR_LIMIT));

        let entry = history
            .push(ClipboardContent::Text(text.clone()))
            .entry
            .unwrap();

        assert!(matches!(
            entry.content,
            ClipboardContent::Text(saved_text) if saved_text == text
        ));
    }

    #[test]
    fn oversized_text_title_is_shortened_for_list_rendering() {
        let text = format!("{}tail", "a".repeat(TEXT_CONTENT_CHAR_LIMIT));
        let title = ClipboardContent::Text(text).title_with_language(AppLanguage::English);

        assert_eq!(
            title.chars().count(),
            TEXT_LIST_PREVIEW_CHAR_LIMIT + "...".chars().count()
        );
        assert!(title.ends_with("..."));
        assert!(
            title
                .trim_end_matches("...")
                .chars()
                .all(|character| character == 'a')
        );
    }

    #[test]
    fn oversized_text_title_preserves_character_boundaries() {
        let text = format!("{}界外", "好".repeat(TEXT_CONTENT_CHAR_LIMIT));
        let title = ClipboardContent::Text(text).title_with_language(AppLanguage::Chinese);

        assert_eq!(
            title.chars().count(),
            TEXT_LIST_PREVIEW_CHAR_LIMIT + "...".chars().count()
        );
        assert!(title.ends_with("..."));
        assert!(
            title
                .trim_end_matches("...")
                .chars()
                .all(|character| character == '好')
        );
    }

    #[test]
    fn capacity_keeps_pinned_and_favorite_entries() {
        let mut history = ClipboardHistory::new(2);
        let pinned_id = history
            .push(ClipboardContent::Text("pinned".to_string()))
            .entry
            .unwrap()
            .id;
        let favorite_id = history
            .push(ClipboardContent::Text("favorite".to_string()))
            .entry
            .unwrap()
            .id;

        history.toggle_pin(pinned_id);
        history.toggle_favorite(favorite_id);

        let overflow = history.push(ClipboardContent::Text("overflow".to_string()));

        assert_eq!(overflow.removed_ids, vec![overflow.entry.unwrap().id]);
        assert_eq!(history.counts().total, 2);
        assert!(history.entry(pinned_id).is_some());
        assert!(history.entry(favorite_id).is_some());
    }

    #[test]
    fn reducing_capacity_removes_old_unprotected_entries() {
        let mut history = ClipboardHistory::new(5);
        let old_id = history
            .push(ClipboardContent::Text("old".to_string()))
            .entry
            .unwrap()
            .id;
        let pinned_id = history
            .push(ClipboardContent::Text("pinned".to_string()))
            .entry
            .unwrap()
            .id;
        let latest_id = history
            .push(ClipboardContent::Text("latest".to_string()))
            .entry
            .unwrap()
            .id;

        history.toggle_pin(pinned_id);
        let removed = history.set_capacity(2);

        assert_eq!(removed, vec![old_id]);
        assert!(history.entry(old_id).is_none());
        assert!(history.entry(pinned_id).is_some());
        assert!(history.entry(latest_id).is_some());
    }

    #[test]
    fn age_cleanup_can_preserve_favorite_entries() {
        let mut history = ClipboardHistory::new(10);
        let regular_id = history
            .push(ClipboardContent::Text("regular".to_string()))
            .entry
            .unwrap()
            .id;
        let favorite_id = history
            .push(ClipboardContent::Text("favorite".to_string()))
            .entry
            .unwrap()
            .id;

        history.toggle_favorite(favorite_id);
        assert_eq!(
            history.deletable_ids(&[regular_id, favorite_id], true),
            vec![regular_id]
        );
        for entry in &mut history.entries {
            entry.captured_at = Local::now() - ChronoDuration::days(31);
        }

        let removed = history.remove_older_than_days(30, true);

        assert_eq!(removed, 1);
        assert!(history.entry(regular_id).is_none());
        assert!(history.entry(favorite_id).is_some());
    }

    #[test]
    fn pinned_entries_are_not_promoted_by_copy_or_capture() {
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Text("old".to_string()));
        let pinned_id = history
            .push(ClipboardContent::Text("pinned".to_string()))
            .entry
            .unwrap()
            .id;
        history.push(ClipboardContent::Text("latest".to_string()));
        history.toggle_pin(pinned_id);
        let captured_at = history.entry(pinned_id).unwrap().captured_at;

        assert!(!history.should_promote(pinned_id));
        assert!(!history.would_push_change(&ClipboardContent::Text("pinned".to_string())));
        assert!(history.promote(pinned_id).is_none());
        assert_eq!(history.entry(pinned_id).unwrap().captured_at, captured_at);
    }

    #[test]
    fn text_query_does_not_match_image_metadata() {
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Image(ClipboardImage {
            width: 5,
            height: 10,
            bytes: Some(Arc::new(vec![0, 0, 0, 0])),
            preview_url: None,
        }));
        history.push(ClipboardContent::Text("5".to_string()));

        let empty_results = history.filtered("", ClipboardFilter::All);
        assert!(
            empty_results
                .iter()
                .any(|entry| entry.kind() == ClipboardKind::Image)
        );

        let search_results = history.filtered("5", ClipboardFilter::All);
        assert_eq!(search_results.len(), 1);
        assert_eq!(search_results[0].kind(), ClipboardKind::Text);
    }
}

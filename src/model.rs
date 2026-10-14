mod image;

pub use image::ClipboardImage;

#[cfg(test)]
use chrono::Duration as ChronoDuration;
use chrono::{DateTime, Local};

pub const DEFAULT_HISTORY_LIMIT: usize = 200;
pub const DEFAULT_BACKGROUND_OPACITY: u8 = 100;
pub const DEFAULT_GLOBAL_SHOW_SHORTCUT: &str = "Alt+C";
pub const MIN_BACKGROUND_OPACITY: u8 = 45;
pub const HISTORY_LIMIT_OPTIONS: [usize; 5] = [50, 100, 200, 500, 1000];
pub const AUTO_CLEANUP_DAY_OPTIONS: [Option<u16>; 4] = [Some(7), Some(30), Some(60), None];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppLanguage {
    Chinese,
    English,
}

impl AppLanguage {
    pub fn key(self) -> &'static str {
        match self {
            Self::Chinese => "zh-CN",
            Self::English => "en-US",
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
    pub fn key(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
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
    pub double_click_copy: bool,
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
            quick_paste: false,
            double_click_copy: true,
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

    /// Returns the normalized form for comparison without cloning content.
    ///
    /// Normalization only ever rewrites file lists (see [`Self::normalized`]),
    /// so text and image content borrow. Duplicate detection runs this on
    /// every clipboard event; cloning multi-megabyte text there is wasted work.
    pub fn normalized_ref(&self) -> std::borrow::Cow<'_, Self> {
        match self {
            Self::Files(_) => std::borrow::Cow::Owned(self.clone().normalized()),
            Self::Text(_) | Self::Image(_) => std::borrow::Cow::Borrowed(self),
        }
    }

    pub fn title_with_language(&self, language: AppLanguage) -> String {
        match self {
            Self::Text(text) => text.clone(),
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

use std::sync::Arc;

// Eq lets Arc<ClipboardEntry> comparisons take the pointer-equality fast path,
// which keeps component memoization cheap for large text/image entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardEntry {
    pub id: u64,
    pub content: ClipboardContent,
    pub captured_at: DateTime<Local>,
    pub pinned: bool,
    pub favorite: bool,
}

// Shared entries support UI lists and background filtering without text copies.
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

    pub fn title_with_language(&self, language: AppLanguage) -> String {
        self.content.title_with_language(language)
    }

    pub fn size_label_with_language(&self, language: AppLanguage) -> String {
        self.content.size_label_with_language(language)
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
    entries: Vec<Arc<ClipboardEntry>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PushResult {
    pub changed: bool,
    pub entry: Option<ClipboardEntry>,
    pub removed_ids: Vec<u64>,
}

impl ClipboardHistory {
    #[cfg(test)]
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
            entries: entries.into_iter().map(Arc::new).collect(),
        };
        history.sort_entries();
        history.truncate();
        history
    }

    #[cfg(test)]
    pub fn push(&mut self, content: ClipboardContent) -> PushResult {
        self.push_with_promotion(content, true)
    }

    #[cfg(test)]
    pub fn push_with_promotion(
        &mut self,
        content: ClipboardContent,
        promote_existing: bool,
    ) -> PushResult {
        self.push_excluding(content, promote_existing, &Default::default())
    }

    pub fn push_excluding(
        &mut self,
        content: ClipboardContent,
        promote_existing: bool,
        excluded: &std::collections::HashSet<u64>,
    ) -> PushResult {
        let content = content.normalized();
        if content.is_empty() {
            return PushResult::default();
        }

        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| !excluded.contains(&entry.id) && entry.content == content)
        {
            if !promote_existing || !self.should_promote_position(position) {
                return PushResult::default();
            }

            let rc_entry = self.entries.remove(position);
            let mut entry = Arc::unwrap_or_clone(rc_entry);
            entry.captured_at = Local::now();
            let updated_entry = entry.clone();
            self.entries.insert(0, Arc::new(entry));
            return PushResult {
                changed: true,
                entry: Some(updated_entry),
                removed_ids: Vec::new(),
            };
        }

        let entry = ClipboardEntry::new(self.next_id, content);
        let inserted_entry = entry.clone();
        self.next_id += 1;
        self.entries.insert(0, Arc::new(entry));
        let removed_ids = self.truncate_excluding(excluded);

        PushResult {
            changed: true,
            entry: Some(inserted_entry),
            removed_ids,
        }
    }

    #[cfg(test)]
    pub fn would_push_change(&self, content: &ClipboardContent) -> bool {
        self.would_push_change_with_promotion(content, true)
    }

    #[cfg(test)]
    pub fn would_push_change_with_promotion(
        &self,
        content: &ClipboardContent,
        promote_existing: bool,
    ) -> bool {
        self.would_push_change_excluding(content, promote_existing, &Default::default())
    }

    pub fn would_push_change_excluding(
        &self,
        content: &ClipboardContent,
        promote_existing: bool,
        excluded: &std::collections::HashSet<u64>,
    ) -> bool {
        let content = content.normalized_ref();
        if content.is_empty() {
            return false;
        }

        self.entries
            .iter()
            .position(|entry| !excluded.contains(&entry.id) && entry.content == *content)
            .is_none_or(|position| promote_existing && self.should_promote_position(position))
    }

    pub fn filtered(&self, query: &str, filter: ClipboardFilter) -> Vec<Arc<ClipboardEntry>> {
        Self::filter_snapshot(self.snapshot(), query, filter)
    }

    /// Only shared pointers are copied on GPUI; text matching can run elsewhere.
    pub fn snapshot(&self) -> Vec<Arc<ClipboardEntry>> {
        self.entries.clone()
    }

    pub fn filter_snapshot(
        entries: Vec<Arc<ClipboardEntry>>,
        query: &str,
        filter: ClipboardFilter,
    ) -> Vec<Arc<ClipboardEntry>> {
        Self::filter_snapshot_while(entries, query, filter, || true).unwrap()
    }

    pub fn filter_snapshot_while(
        entries: Vec<Arc<ClipboardEntry>>,
        query: &str,
        filter: ClipboardFilter,
        is_current: impl Fn() -> bool,
    ) -> Option<Vec<Arc<ClipboardEntry>>> {
        let normalized_query = crate::search::SearchQuery::new(query);
        let mut entries = entries
            .into_iter()
            .take_while(|_| is_current())
            .filter(|entry| matches_filter(entry, filter))
            .filter(|entry| {
                if normalized_query.is_empty() {
                    return true;
                }

                if entry.kind() == ClipboardKind::Image {
                    return false;
                }

                match &entry.content {
                    ClipboardContent::Text(text) => {
                        normalized_query.matches_while(text, &is_current)
                    }
                    ClipboardContent::Files(files) => files.iter().any(|file| {
                        is_current() && normalized_query.matches_while(file, &is_current)
                    }),
                    ClipboardContent::Image(_) => false,
                }
            })
            .collect::<Vec<_>>();

        if !is_current() {
            return None;
        }
        sort_entries(&mut entries);
        Some(entries)
    }

    pub fn entry_matches(entry: &ClipboardEntry, query: &str, filter: ClipboardFilter) -> bool {
        if !matches_filter(entry, filter) {
            return false;
        }

        let normalized_query = crate::search::SearchQuery::new(query);
        normalized_query.is_empty()
            || (entry.kind() != ClipboardKind::Image
                && content_matches_query(&entry.content, &normalized_query))
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

    pub fn entry(&self, id: u64) -> Option<Arc<ClipboardEntry>> {
        self.entries.iter().find(|entry| entry.id == id).cloned()
    }

    pub fn position(&self, id: u64) -> Option<usize> {
        self.entries.iter().position(|entry| entry.id == id)
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

            let rc_entry = self.entries.remove(position);
            let mut entry = Arc::unwrap_or_clone(rc_entry);
            entry.captured_at = Local::now();
            let updated_entry = entry.clone();
            self.entries.insert(0, Arc::new(entry));
            Some(updated_entry)
        } else {
            None
        }
    }

    /// Installs the persisted preview URL and releases the captured pixels.
    /// Copying or expanding the image loads the original from storage on demand.
    pub fn set_image_preview_url(&mut self, id: u64, preview_url: String) -> bool {
        let Some(position) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let rc_entry = self.entries.remove(position);
        let mut entry = Arc::unwrap_or_clone(rc_entry);
        let ClipboardContent::Image(image) = &mut entry.content else {
            self.entries.insert(position, Arc::new(entry));
            return false;
        };
        image.preview_url = Some(preview_url);
        image.bytes = None;
        self.entries.insert(position, Arc::new(entry));
        true
    }

    pub fn toggle_favorite(&mut self, id: u64) -> Option<ClipboardEntry> {
        if let Some(position) = self.entries.iter().position(|entry| entry.id == id) {
            let rc_entry = self.entries.remove(position);
            let mut entry = Arc::unwrap_or_clone(rc_entry);
            entry.favorite = !entry.favorite;
            let updated_entry = entry.clone();
            self.entries.insert(position, Arc::new(entry));
            Some(updated_entry)
        } else {
            None
        }
    }

    #[cfg(test)]
    pub fn toggle_pin(&mut self, id: u64) -> Option<ClipboardEntry> {
        if let Some(position) = self.entries.iter().position(|entry| entry.id == id) {
            let rc_entry = self.entries.remove(position);
            let mut entry = Arc::unwrap_or_clone(rc_entry);
            entry.pinned = !entry.pinned;
            let updated_entry = entry.clone();
            self.entries.insert(position, Arc::new(entry));
            Some(updated_entry)
        } else {
            None
        }
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.id != id);
        self.entries.len() != before
    }

    /// Applies an immutable deletion snapshot, preserving captures added later.
    pub fn remove_entries(&mut self, ids: &[u64]) {
        let ids = ids
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        self.entries.retain(|entry| !ids.contains(&entry.id));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[cfg(test)]
    pub fn remove_older_than_days(&mut self, days: u16, preserve_favorites: bool) -> usize {
        let cutoff = Local::now() - ChronoDuration::days(i64::from(days));
        let before = self.entries.len();
        self.entries
            .retain(|entry| entry.captured_at >= cutoff || (preserve_favorites && entry.favorite));
        before - self.entries.len()
    }

    #[cfg(test)]
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
        self.truncate_excluding(&Default::default())
    }

    fn truncate_excluding(&mut self, excluded: &std::collections::HashSet<u64>) -> Vec<u64> {
        let mut removed_ids = Vec::new();
        let mut active_count = self
            .entries
            .iter()
            .filter(|entry| !excluded.contains(&entry.id))
            .count();
        while active_count > self.capacity {
            let Some(position) = self.entries.iter().rposition(|entry| {
                !excluded.contains(&entry.id) && !entry.pinned && !entry.favorite
            }) else {
                break;
            };

            removed_ids.push(self.entries.remove(position).id);
            active_count -= 1;
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

fn content_matches_query(content: &ClipboardContent, query: &crate::search::SearchQuery) -> bool {
    match content {
        ClipboardContent::Text(text) => query.matches(text),
        ClipboardContent::Files(files) => files.iter().any(|file| query.matches(file)),
        ClipboardContent::Image(_) => false,
    }
}

fn sort_entries(entries: &mut [Arc<ClipboardEntry>]) {
    entries.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| right.captured_at.cmp(&left.captured_at))
            .then_with(|| right.id.cmp(&left.id))
    });
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

    const TEXT_CONTENT_CHAR_COUNT: usize = 200_000;

    #[test]
    fn background_search_shares_content_and_discards_cancelled_snapshots() {
        fn assert_send<T: Send>() {}
        assert_send::<Vec<Arc<ClipboardEntry>>>();
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Text("a".repeat(100_000)));
        let snapshot = history.snapshot();
        assert!(Arc::ptr_eq(&snapshot[0], &history.entries[0]));
        let checks = std::cell::Cell::new(0);
        let result = ClipboardHistory::filter_snapshot_while(
            snapshot,
            "missing",
            ClipboardFilter::All,
            || {
                checks.set(checks.get() + 1);
                checks.get() < 3
            },
        );
        assert!(
            result.is_none(),
            "partial cancelled results must not be applied"
        );
    }

    #[test]
    fn recapture_during_snapshot_deletion_allocates_a_new_identity() {
        for promote in [false, true] {
            let mut history = ClipboardHistory::new(10);
            let old = history
                .push(ClipboardContent::Text("same content".into()))
                .entry
                .unwrap();
            let pending = std::collections::HashSet::from([old.id]);
            let content = ClipboardContent::Text("same content".into());
            assert!(history.would_push_change_excluding(&content, promote, &pending));
            let new = history
                .push_excluding(content, promote, &pending)
                .entry
                .unwrap();
            assert_ne!(new.id, old.id);
            history.remove_entries(&[old.id]);
            assert_eq!(history.entries.len(), 1);
            assert_eq!(history.entries[0].id, new.id);
        }
    }

    #[test]
    fn capacity_pruning_preserves_the_allocator_high_water_mark() {
        let mut protected = ClipboardEntry::new(1, ClipboardContent::Text("protected".into()));
        protected.favorite = true;
        let pruned = ClipboardEntry::new(100, ClipboardContent::Text("pruned".into()));
        let mut history = ClipboardHistory::from_entries(1, vec![protected, pruned]);
        let entry = history
            .push(ClipboardContent::Text("new".into()))
            .entry
            .unwrap();
        assert_eq!(entry.id, 101);
    }

    #[test]
    fn pending_protected_deletions_do_not_evict_new_captures_at_capacity() {
        let mut protected = ClipboardEntry::new(1, ClipboardContent::Text("same".into()));
        protected.favorite = true;
        let mut history = ClipboardHistory::from_entries(1, vec![protected]);
        let pending = std::collections::HashSet::from([1]);
        let result = history.push_excluding(ClipboardContent::Text("same".into()), true, &pending);
        let new_id = result.entry.unwrap().id;
        assert!(result.removed_ids.is_empty());
        history.remove_entries(&[1]);
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].id, new_id);
    }

    #[test]
    fn history_search_uses_the_same_unicode_fold_for_query_and_content() {
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Text("ΟΣ Straße".into()));
        for query in ["ΟΣ", "ος", "οσ", "STRASSE"] {
            assert_eq!(history.filtered(query, ClipboardFilter::All).len(), 1);
            assert!(ClipboardHistory::entry_matches(
                &history.entries[0],
                query,
                ClipboardFilter::All
            ));
        }
    }

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
        let text = format!("\n  {}tail  \n", "a".repeat(TEXT_CONTENT_CHAR_COUNT));

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
    fn text_title_preserves_full_text_for_list_rendering() {
        let text = format!("{}tail", "a".repeat(TEXT_CONTENT_CHAR_COUNT));
        let title = ClipboardContent::Text(text).title_with_language(AppLanguage::English);

        assert_eq!(title.chars().count(), TEXT_CONTENT_CHAR_COUNT + 4);
        assert!(title.ends_with("tail"));
    }

    #[test]
    fn text_title_preserves_full_unicode_text() {
        let text = format!("{}界外", "好".repeat(TEXT_CONTENT_CHAR_COUNT));
        let title = ClipboardContent::Text(text).title_with_language(AppLanguage::Chinese);

        assert_eq!(title.chars().count(), TEXT_CONTENT_CHAR_COUNT + 2);
        assert!(title.ends_with("界外"));
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
            Arc::make_mut(entry).captured_at = Local::now() - ChronoDuration::days(31);
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
    fn duplicate_capture_can_leave_entry_in_place() {
        let mut history = ClipboardHistory::new(10);
        let first_id = history
            .push(ClipboardContent::Text("first".to_string()))
            .entry
            .unwrap()
            .id;
        let second_id = history
            .push(ClipboardContent::Text("second".to_string()))
            .entry
            .unwrap()
            .id;

        assert!(
            !history.would_push_change_with_promotion(
                &ClipboardContent::Text("first".to_string()),
                false
            )
        );
        assert!(
            history
                .push_with_promotion(ClipboardContent::Text("first".to_string()), false)
                .entry
                .is_none()
        );
        assert_eq!(history.position(second_id), Some(0));
        assert_eq!(history.position(first_id), Some(1));
    }

    #[test]
    fn text_query_does_not_match_image_metadata() {
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Image(ClipboardImage {
            width: 5,
            height: 10,
            bytes: Some(Arc::new(vec![0, 0, 0, 0])),
            preview_url: None,
            content_hash: None,
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

    #[test]
    fn search_matches_case_insensitively_without_lowercasing_entries() {
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Text("Hello World 世界".into()));
        history.push(ClipboardContent::Files(vec!["C:/Docs/Report.PDF".into()]));

        assert_eq!(history.filtered("hello", ClipboardFilter::All).len(), 1);
        assert_eq!(history.filtered("WORLD", ClipboardFilter::All).len(), 1);
        assert_eq!(history.filtered("世界", ClipboardFilter::All).len(), 1);
        assert_eq!(
            history.filtered("report.pdf", ClipboardFilter::All).len(),
            1
        );
        assert_eq!(history.filtered("planet", ClipboardFilter::All).len(), 0);
    }

    #[test]
    fn file_content_is_normalized_before_duplicate_detection() {
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Files(vec!["C:/a.txt".into()]));

        assert!(!history.would_push_change(&ClipboardContent::Files(vec![
            " C:/a.txt ".into(),
            "  ".into(),
        ])));
    }

    #[test]
    fn image_preview_url_can_be_replaced_after_persistence() {
        let mut history = ClipboardHistory::new(10);
        let id = history
            .push(ClipboardContent::Image(ClipboardImage {
                width: 2,
                height: 1,
                bytes: Some(Arc::new(vec![0; 8])),
                preview_url: None,
                content_hash: None,
            }))
            .entry
            .unwrap()
            .id;

        assert!(history.set_image_preview_url(id, "file:///preview.png".to_string()));
        assert!(matches!(
            &history.entry(id).unwrap().content,
            ClipboardContent::Image(image)
                if image.preview_url.as_deref() == Some("file:///preview.png")
                    && image.bytes.is_none()
        ));
    }

    #[test]
    fn duplicate_images_are_not_added_and_can_be_promoted() {
        let mut history = ClipboardHistory::new(10);
        let image = ClipboardImage::from_rgba(1, 1, vec![1, 2, 3, 255]);
        let first_id = history
            .push_with_promotion(ClipboardContent::Image(image.clone()), false)
            .entry
            .unwrap()
            .id;
        history.push(ClipboardContent::Text("other".to_string()));

        let duplicate = history.push_with_promotion(ClipboardContent::Image(image), true);

        assert!(duplicate.changed);
        assert_eq!(duplicate.entry.unwrap().id, first_id);
        assert_eq!(history.position(first_id), Some(0));
        assert_eq!(history.counts().image, 1);
    }
}

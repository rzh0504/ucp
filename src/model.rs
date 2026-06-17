use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Duration as ChronoDuration, Local};
use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
use std::sync::Arc;

pub const DEFAULT_HISTORY_LIMIT: usize = 200;
pub const HISTORY_LIMIT_OPTIONS: [usize; 5] = [50, 100, 200, 500, 1000];
pub const AUTO_CLEANUP_DAY_OPTIONS: [Option<u16>; 4] = [Some(7), Some(30), Some(60), None];
const MAX_INLINE_IMAGE_PREVIEW_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Image,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    pub width: usize,
    pub height: usize,
    pub bytes: Arc<Vec<u8>>,
    pub preview_url: Option<String>,
}

impl ClipboardImage {
    pub fn from_rgba(width: usize, height: usize, bytes: Vec<u8>) -> Self {
        let preview_url = if bytes.len() <= MAX_INLINE_IMAGE_PREVIEW_BYTES {
            encode_png(&bytes, width, height).map(|png| {
                format!(
                    "data:image/png;base64,{}",
                    general_purpose::STANDARD.encode(png)
                )
            })
        } else {
            None
        };

        Self {
            width,
            height,
            bytes: Arc::new(bytes),
            preview_url,
        }
    }

    pub fn to_png_bytes(&self) -> Option<Vec<u8>> {
        encode_png(self.bytes.as_slice(), self.width, self.height)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardContent {
    Text(String),
    Image(ClipboardImage),
    Files(Vec<String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppSettings {
    pub history_limit: usize,
    pub auto_cleanup_days: Option<u16>,
    pub launch_at_startup: bool,
    pub keyboard_shortcuts: bool,
    pub auto_focus_history: bool,
    pub promote_copied_entries: bool,
    pub quick_paste: bool,
    pub show_copy_time: bool,
    pub show_text_length: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            history_limit: DEFAULT_HISTORY_LIMIT,
            auto_cleanup_days: None,
            launch_at_startup: false,
            keyboard_shortcuts: true,
            auto_focus_history: true,
            promote_copied_entries: true,
            quick_paste: false,
            show_copy_time: true,
            show_text_length: true,
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
        self
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
            Self::Image(image) => image.width == 0 || image.height == 0 || image.bytes.is_empty(),
            Self::Files(files) => files.is_empty(),
        }
    }

    pub fn normalized(self) -> Self {
        match self {
            Self::Text(text) => Self::Text(text.trim().to_string()),
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

    pub fn searchable_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Image(image) => format!("图像 {} x {}", image.width, image.height),
            Self::Files(files) => files.join("\n"),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Image(_) => "图像".to_string(),
            Self::Files(files) => {
                if files.len() == 1 {
                    files[0].clone()
                } else {
                    format!("{} 个文件", files.len())
                }
            }
        }
    }

    pub fn size_label(&self) -> String {
        match self {
            Self::Text(text) => format!("{} 字符", text.chars().count()),
            Self::Image(image) => format_bytes(image.bytes.len()),
            Self::Files(files) => format!("{} 个文件", files.len()),
        }
    }
}

impl ClipboardKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "文本",
            Self::Image => "图像",
            Self::File => "文件",
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

    pub fn title(&self) -> String {
        self.content.title()
    }

    pub fn size_label(&self) -> String {
        self.content.size_label()
    }

    pub fn age_label(&self) -> String {
        let elapsed = Local::now().signed_duration_since(self.captured_at);
        let seconds = elapsed.num_seconds().max(0);

        if seconds < 60 {
            "刚刚".to_string()
        } else if seconds < 3_600 {
            format!("{} 分钟前", seconds / 60)
        } else if seconds < 86_400 {
            format!("{} 小时前", seconds / 3_600)
        } else {
            format!("{} 天前", seconds / 86_400)
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
            let mut entry = self.entries.remove(position);
            let changed_top = position != 0;
            entry.captured_at = Local::now();
            let updated_entry = entry.clone();
            self.entries.insert(0, entry);
            return PushResult {
                changed: changed_top,
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

    pub fn filtered(&self, query: &str, filter: ClipboardFilter) -> Vec<ClipboardEntry> {
        let normalized_query = query.trim().to_lowercase();
        let mut entries = self
            .entries
            .iter()
            .filter(|entry| match filter {
                ClipboardFilter::All => true,
                ClipboardFilter::Text => entry.kind() == ClipboardKind::Text,
                ClipboardFilter::Image => entry.kind() == ClipboardKind::Image,
                ClipboardFilter::File => entry.kind() == ClipboardKind::File,
                ClipboardFilter::Favorite => entry.favorite,
            })
            .filter(|entry| {
                if normalized_query.is_empty() {
                    return true;
                }

                if entry.kind() == ClipboardKind::Image {
                    return false;
                }

                entry
                    .content
                    .searchable_text()
                    .to_lowercase()
                    .contains(normalized_query.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        sort_entries(&mut entries);
        entries
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

    pub fn promote(&mut self, id: u64) -> Option<ClipboardEntry> {
        if let Some(position) = self.entries.iter().position(|entry| entry.id == id) {
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

    pub fn remove_older_than_days(&mut self, days: u16) -> usize {
        let cutoff = Local::now() - ChronoDuration::days(i64::from(days));
        let before = self.entries.len();
        self.entries.retain(|entry| entry.captured_at >= cutoff);
        before - self.entries.len()
    }

    pub fn set_capacity(&mut self, capacity: usize) -> Vec<u64> {
        self.capacity = capacity.max(1);
        self.truncate()
    }

    fn sort_entries(&mut self) {
        sort_entries(&mut self.entries);
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

fn sort_entries(entries: &mut [ClipboardEntry]) {
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

fn encode_png(bytes: &[u8], width: usize, height: usize) -> Option<Vec<u8>> {
    if bytes.len() != width.checked_mul(height)?.checked_mul(4)? {
        return None;
    }

    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(bytes, width as u32, height as u32, ColorType::Rgba8.into())
        .ok()?;

    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn duplicate_text_is_normalized_and_not_duplicated() {
        let mut history = ClipboardHistory::new(10);

        let first = history.push(ClipboardContent::Text("hello".to_string()));
        let duplicate = history.push(ClipboardContent::Text("  hello  ".to_string()));

        assert!(first.changed);
        assert!(!duplicate.changed);
        assert_eq!(history.counts().total, 1);
        assert_eq!(history.counts().text, 1);
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
    fn text_query_does_not_match_image_metadata() {
        let mut history = ClipboardHistory::new(10);
        history.push(ClipboardContent::Image(ClipboardImage {
            width: 5,
            height: 10,
            bytes: Arc::new(vec![0, 0, 0, 0]),
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

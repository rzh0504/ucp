use chrono::{DateTime, Local};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Image,
    File,
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClipboardEntry {
    pub id: u64,
    pub kind: ClipboardKind,
    pub content: String,
    pub captured_at: DateTime<Local>,
    pub pinned: bool,
    pub favorite: bool,
}

impl ClipboardEntry {
    pub fn text(id: u64, content: String) -> Self {
        Self {
            id,
            kind: ClipboardKind::Text,
            content,
            captured_at: Local::now(),
            pinned: false,
            favorite: false,
        }
    }

    pub fn is_text(&self) -> bool {
        self.kind == ClipboardKind::Text
    }

    pub fn size_label(&self) -> String {
        match self.kind {
            ClipboardKind::Text => format!("{} 字符", self.content.chars().count()),
            ClipboardKind::Image => "1 张图像".to_string(),
            ClipboardKind::File => "1 个文件".to_string(),
        }
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

impl ClipboardHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            next_id: 1,
            entries: Vec::new(),
        }
    }

    pub fn push_text(&mut self, content: String) -> bool {
        let content = content.trim().to_string();
        if content.is_empty() {
            return false;
        }

        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.kind == ClipboardKind::Text && entry.content == content)
        {
            let mut entry = self.entries.remove(position);
            let changed_top = position != 0;
            entry.captured_at = Local::now();
            self.entries.insert(0, entry);
            return changed_top;
        }

        let entry = ClipboardEntry::text(self.next_id, content);
        self.next_id += 1;
        self.entries.insert(0, entry);
        self.truncate();
        true
    }

    pub fn filtered(&self, query: &str, filter: ClipboardFilter) -> Vec<ClipboardEntry> {
        let normalized_query = query.trim().to_lowercase();
        let mut entries = self
            .entries
            .iter()
            .filter(|entry| match filter {
                ClipboardFilter::All => true,
                ClipboardFilter::Text => entry.kind == ClipboardKind::Text,
                ClipboardFilter::Image => entry.kind == ClipboardKind::Image,
                ClipboardFilter::File => entry.kind == ClipboardKind::File,
                ClipboardFilter::Favorite => entry.favorite,
            })
            .filter(|entry| {
                normalized_query.is_empty()
                    || entry
                        .content
                        .to_lowercase()
                        .contains(normalized_query.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        entries.sort_by_key(|entry| (!entry.pinned, std::cmp::Reverse(entry.id)));
        entries
    }

    pub fn counts(&self) -> HistoryCounts {
        let mut counts = HistoryCounts {
            total: self.entries.len(),
            ..HistoryCounts::default()
        };

        for entry in &self.entries {
            match entry.kind {
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

    pub fn promote(&mut self, id: u64) {
        if let Some(position) = self.entries.iter().position(|entry| entry.id == id) {
            let mut entry = self.entries.remove(position);
            entry.captured_at = Local::now();
            self.entries.insert(0, entry);
        }
    }

    pub fn toggle_favorite(&mut self, id: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.favorite = !entry.favorite;
        }
    }

    pub fn toggle_pin(&mut self, id: u64) {
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.pinned = !entry.pinned;
        }
    }

    pub fn remove(&mut self, id: u64) {
        self.entries.retain(|entry| entry.id != id);
    }

    fn truncate(&mut self) {
        if self.entries.len() > self.capacity {
            self.entries.truncate(self.capacity);
        }
    }
}

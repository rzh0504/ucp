use chrono::{DateTime, Local};

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
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardContent {
    Text(String),
    Image(ClipboardImage),
    Files(Vec<String>),
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
            Self::Image(image) => format!("图像 {} x {}", image.width, image.height),
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
            Self::Image(image) => format!(
                "{} x {} · {}",
                image.width,
                image.height,
                format_bytes(image.bytes.len())
            ),
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

impl ClipboardHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            next_id: 1,
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, content: ClipboardContent) -> bool {
        let content = content.normalized();
        if content.is_empty() {
            return false;
        }

        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.content == content)
        {
            let mut entry = self.entries.remove(position);
            let changed_top = position != 0;
            entry.captured_at = Local::now();
            self.entries.insert(0, entry);
            return changed_top;
        }

        let entry = ClipboardEntry::new(self.next_id, content);
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
                ClipboardFilter::Text => entry.kind() == ClipboardKind::Text,
                ClipboardFilter::Image => entry.kind() == ClipboardKind::Image,
                ClipboardFilter::File => entry.kind() == ClipboardKind::File,
                ClipboardFilter::Favorite => entry.favorite,
            })
            .filter(|entry| {
                normalized_query.is_empty()
                    || entry
                        .content
                        .searchable_text()
                        .to_lowercase()
                        .contains(normalized_query.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();

        entries.sort_by(|left, right| {
            right
                .pinned
                .cmp(&left.pinned)
                .then_with(|| right.captured_at.cmp(&left.captured_at))
                .then_with(|| right.id.cmp(&left.id))
        });
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
        while self.entries.len() > self.capacity {
            let Some(position) = self
                .entries
                .iter()
                .rposition(|entry| !entry.pinned && !entry.favorite)
            else {
                break;
            };

            self.entries.remove(position);
        }
    }
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

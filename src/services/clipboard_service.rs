use crate::model::{AppLanguage, ClipboardContent, ClipboardEntry, ClipboardFilter, ClipboardHistory};
use crate::platform;
use crate::storage;
use chrono::Local;
use std::path::Path;

/// 纯业务逻辑服务，不依赖 Dioxus Signal
pub struct ClipboardService;

#[derive(Debug)]
pub enum ClipboardError {
    EntryNotFound,
    ImageLoadFailed(String),
    FilesNotFound(Vec<String>),
    FileAccessError(String),
    WriteError(String),
    StorageError(String),
}

pub struct CopyResult {
    pub should_promote: bool,
    pub content: Option<ClipboardContent>,
}

impl ClipboardService {
    /// 复制条目到剪贴板
    pub fn copy_entry(
        history: &ClipboardHistory,
        id: u64,
        promote_on_copy: bool,
    ) -> Result<CopyResult, ClipboardError> {
        let Some(entry) = history.entry(id) else {
            return Err(ClipboardError::EntryNotFound);
        };

        let mut content = entry.content.clone();

        // 如果是图片且未加载，先加载图片
        if let ClipboardContent::Image(image) = &content {
            if !image.has_bytes() {
                if let Some(loaded_image) = storage::load_image(entry.id)
                    .map_err(|e| ClipboardError::ImageLoadFailed(e.to_string()))? {
                    content = ClipboardContent::Image(loaded_image);
                } else {
                    return Err(ClipboardError::ImageLoadFailed("Image not found in storage".to_string()));
                }
            }
        }

        // 如果是文件，验证文件存在
        if let ClipboardContent::Files(files) = &content {
            Self::validate_files_exist(files)?;
        }

        // 写入剪贴板
        platform::clipboard::write_content(&content)
            .map_err(|e| ClipboardError::WriteError(e.to_string()))?;

        let should_promote = promote_on_copy && history.should_promote(id);

        Ok(CopyResult {
            should_promote,
            content: Some(content),
        })
    }

    /// 提升条目（需要可变 history）
    pub fn promote_entry(
        history: &mut ClipboardHistory,
        id: u64,
    ) -> Result<ClipboardEntry, ClipboardError> {
        history
            .promote(id)
            .ok_or(ClipboardError::EntryNotFound)
    }

    /// 保存条目到存储
    #[allow(dead_code)]
    pub fn save_entry(entry: &ClipboardEntry) -> Result<(), ClipboardError> {
        storage::save_entry(entry)
            .map(|_| ())
            .map_err(|e| ClipboardError::StorageError(e.to_string()))
    }

    /// 验证文件是否存在
    pub fn validate_files_exist(files: &[String]) -> Result<(), ClipboardError> {
        if files.is_empty() {
            return Err(ClipboardError::FilesNotFound(vec![]));
        }

        let mut missing_files = Vec::new();

        for file in files {
            let file = file.trim();
            if file.is_empty() {
                continue;
            }

            match Path::new(file).try_exists() {
                Ok(true) => {}
                Ok(false) => missing_files.push(file.to_string()),
                Err(e) => {
                    return Err(ClipboardError::FileAccessError(format!(
                        "Cannot access {}: {}",
                        file, e
                    )))
                }
            }
        }

        if !missing_files.is_empty() {
            return Err(ClipboardError::FilesNotFound(missing_files));
        }

        Ok(())
    }

    /// 打开文件位置
    #[cfg(windows)]
    #[allow(dead_code)]
    pub fn open_file_location(path: &Path) -> Result<(), ClipboardError> {
        // 验证路径不包含危险字符
        let path_str = path.to_string_lossy();
        if path_str.contains('\0') {
            return Err(ClipboardError::FileAccessError(
                "File path contains null bytes".to_string(),
            ));
        }

        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path.as_os_str())
            .spawn()
            .map(|_| ())
            .map_err(|e| ClipboardError::WriteError(format!("Failed to open explorer: {}", e)))
    }

    #[cfg(not(windows))]
    pub fn open_file_location(_path: &Path) -> Result<(), ClipboardError> {
        Err(ClipboardError::WriteError(
            "Opening file location not supported on this platform".to_string(),
        ))
    }

    /// 删除条目（返回实际删除的 ID）
    pub fn delete_entries(
        history: &mut ClipboardHistory,
        ids: &[u64],
        preserve_favorites: bool,
    ) -> Result<Vec<u64>, ClipboardError> {
        let deletable_ids = history.deletable_ids(ids, preserve_favorites);
        
        if !deletable_ids.is_empty() {
            storage::delete_entries(&deletable_ids)
                .map_err(|e| ClipboardError::StorageError(e.to_string()))?;
        }

        for id in &deletable_ids {
            history.remove(*id);
        }

        Ok(deletable_ids)
    }

    /// 切换收藏状态
    #[allow(dead_code)]
    pub fn toggle_favorite(
        history: &mut ClipboardHistory,
        id: u64,
    ) -> Result<ClipboardEntry, ClipboardError> {
        let entry = history
            .toggle_favorite(id)
            .ok_or(ClipboardError::EntryNotFound)?;

        Self::save_entry(&entry)?;
        Ok(entry)
    }

    /// 清空历史记录
    #[allow(dead_code)]
    pub fn clear_history(
        history: &mut ClipboardHistory,
        preserve_favorites: bool,
    ) -> Result<usize, ClipboardError> {
        let count = if preserve_favorites {
            let ids = history.deletable_ids_for_filter(ClipboardFilter::All, preserve_favorites);
            let len = ids.len();
            Self::delete_entries(history, &ids, preserve_favorites)?;
            len
        } else {
            let count = history.counts().total;
            storage::delete_entries_older_than(Local::now(), false)
                .map_err(|e| ClipboardError::StorageError(e.to_string()))?;
            history.clear();
            count
        };

        Ok(count)
    }
}

impl ClipboardError {
    pub fn to_localized_string(&self, language: AppLanguage) -> String {
        match self {
            ClipboardError::EntryNotFound => match language {
                AppLanguage::Chinese => "条目未找到".to_string(),
                AppLanguage::English => "Entry not found".to_string(),
            },
            ClipboardError::ImageLoadFailed(e) => match language {
                AppLanguage::Chinese => format!("图片加载失败：{}", e),
                AppLanguage::English => format!("Failed to load image: {}", e),
            },
            ClipboardError::FilesNotFound(files) if files.is_empty() => match language {
                AppLanguage::Chinese => "文件列表为空".to_string(),
                AppLanguage::English => "File list is empty".to_string(),
            },
            ClipboardError::FilesNotFound(files) if files.len() == 1 => match language {
                AppLanguage::Chinese => format!("文件已不存在：{}", files[0]),
                AppLanguage::English => format!("File no longer exists: {}", files[0]),
            },
            ClipboardError::FilesNotFound(files) => match language {
                AppLanguage::Chinese => format!("{} 个文件已不存在", files.len()),
                AppLanguage::English => format!("{} files no longer exist", files.len()),
            },
            ClipboardError::FileAccessError(e) => match language {
                AppLanguage::Chinese => format!("文件访问错误：{}", e),
                AppLanguage::English => format!("File access error: {}", e),
            },
            ClipboardError::WriteError(e) => match language {
                AppLanguage::Chinese => format!("写入失败：{}", e),
                AppLanguage::English => format!("Write failed: {}", e),
            },
            ClipboardError::StorageError(e) => match language {
                AppLanguage::Chinese => format!("存储错误：{}", e),
                AppLanguage::English => format!("Storage error: {}", e),
            },
        }
    }
}

use crate::i18n;
use crate::model::AppLanguage;
use crate::platform;
use std::path::Path;

const COLLAPSED_FILE_LIMIT: usize = 3;

#[derive(Clone, Debug)]
pub(super) struct FileListDisplay {
    files: Vec<FileDisplay>,
    pub(super) stats: String,
    pub(super) missing_count: usize,
}

impl FileListDisplay {
    pub(super) fn new(files: &[String], language: AppLanguage) -> Self {
        let files = files
            .iter()
            .map(|file| FileDisplay::new(file, language))
            .collect::<Vec<_>>();
        let total_count = files.len();
        let missing_count = files.iter().filter(|file| !file.exists).count();
        let stats = match files.as_slice() {
            [] => i18n::file_count(language, 0),
            [file] => format!("{} · {}", file.kind_label, file.directory),
            _ if missing_count > 0 => match language {
                AppLanguage::Chinese => format!("{total_count} 个项目 · {missing_count} 项不存在"),
                AppLanguage::English => format!("{total_count} items · {missing_count} missing"),
            },
            _ => match language {
                AppLanguage::Chinese => format!("{total_count} 个项目"),
                AppLanguage::English => format!("{total_count} items"),
            },
        };

        Self {
            files,
            stats,
            missing_count,
        }
    }

    pub(super) fn visible_files(&self, expanded: bool) -> &[FileDisplay] {
        if expanded {
            &self.files
        } else {
            &self.files[..self.files.len().min(COLLAPSED_FILE_LIMIT)]
        }
    }

    pub(super) fn hidden_count(&self, expanded: bool) -> usize {
        if expanded {
            0
        } else {
            self.files.len().saturating_sub(COLLAPSED_FILE_LIMIT)
        }
    }

    pub(super) fn can_collapse(&self, expanded: bool) -> bool {
        expanded && self.files.len() > COLLAPSED_FILE_LIMIT
    }
}

#[derive(Clone, Debug)]
pub(super) struct FileDisplay {
    pub(super) name: String,
    directory: String,
    kind_label: String,
    pub(super) icon_url: Option<String>,
    pub(super) exists: bool,
}

impl FileDisplay {
    fn new(path: &str, language: AppLanguage) -> Self {
        let copy = i18n::tr(language);
        let path = path.trim();
        let path_ref = Path::new(path);
        let metadata = if path.is_empty() {
            None
        } else {
            std::fs::metadata(path_ref).ok()
        };
        let exists = metadata.is_some();
        let is_dir = metadata.as_ref().is_some_and(|metadata| metadata.is_dir());
        let name = path_ref
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(if path.is_empty() {
                copy.empty_path
            } else {
                path
            })
            .to_string();
        let directory = if path.is_empty() {
            copy.path_empty.to_string()
        } else {
            path_ref
                .parent()
                .map(|parent| parent.display().to_string())
                .filter(|parent| !parent.is_empty())
                .unwrap_or_else(|| copy.current_directory.to_string())
        };
        let kind_label = if path.is_empty() {
            copy.invalid_path.to_string()
        } else if !exists {
            copy.missing.to_string()
        } else if is_dir {
            copy.folder.to_string()
        } else {
            path_ref
                .extension()
                .and_then(|extension| extension.to_str())
                .filter(|extension| !extension.is_empty())
                .map(|extension| extension.to_ascii_uppercase())
                .unwrap_or_else(|| copy.file.to_string())
        };

        Self {
            name,
            directory,
            kind_label,
            icon_url: platform::file_icon::data_url(path),
            exists,
        }
    }
}

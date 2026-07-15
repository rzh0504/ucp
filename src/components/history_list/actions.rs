use super::selection::focused_entry_id;
use crate::i18n;
use crate::model::{
    AppLanguage, ClipboardContent, ClipboardEntry, ClipboardHistory, ClipboardImage,
};
use crate::platform;
use crate::storage;
use dioxus::desktop::DesktopContext;
use dioxus::prelude::*;
use futures_timer::Delay;
use std::path::{Path, PathBuf};
use std::time::Duration;

const QUICK_PASTE_DELAY: Duration = Duration::from_millis(260);
const DELETE_EXIT_DELAY: Duration = Duration::from_millis(240);

pub(super) fn copy_entry(
    id: u64,
    mut history: Signal<ClipboardHistory>,
    mut ignored_clipboard_write: Signal<Option<ClipboardContent>>,
    promote_on_copy: bool,
    mut status: Signal<String>,
    language: AppLanguage,
) -> bool {
    let Some(mut content) = history.read().entry(id).map(|entry| entry.content.clone()) else {
        return false;
    };

    if let ClipboardContent::Image(image) = &content
        && !image.has_bytes()
    {
        let Some(image) = load_image_for_action(id, status, language) else {
            return false;
        };
        content = ClipboardContent::Image(image);
    }

    if let ClipboardContent::Files(files) = &content
        && let Err(error) = validate_files_for_copy(files, language)
    {
        status.set(copy_failed(language, &error));
        return false;
    }

    if let Err(error) = platform::clipboard::write_content(&content) {
        status.set(copy_failed(language, &error.to_string()));
        return false;
    }

    if !promote_on_copy {
        ignored_clipboard_write.set(Some(content.clone()));
    }

    let copied_to_clipboard = i18n::tr(language).copied_to_clipboard;
    if promote_on_copy && history.peek().should_promote(id) {
        if let Some(entry) = history.write().promote(id) {
            save_entry_with_status(&entry, status, copied_to_clipboard, language);
        } else {
            status.set(copied_to_clipboard.to_string());
        }
    } else {
        status.set(copied_to_clipboard.to_string());
    }

    true
}

pub(super) fn run_quick_paste_shortcut(mut status: Signal<String>, language: AppLanguage) {
    status.set(i18n::tr(language).switching_window_and_pasting.to_string());
    spawn(async move {
        Delay::new(QUICK_PASTE_DELAY).await;
        match platform::clipboard::paste_shortcut() {
            Ok(()) => status.set(i18n::tr(language).quick_pasted.to_string()),
            Err(error) => status.set(match language {
                AppLanguage::Chinese => format!("快捷粘贴失败：{error}"),
                AppLanguage::English => format!("Quick paste failed: {error}"),
            }),
        }
    });
}

#[cfg(windows)]
pub(super) fn hide_window_after_copy(window: &DesktopContext) {
    window.close();
}

#[cfg(not(windows))]
pub(super) fn hide_window_after_copy(window: &DesktopContext) {
    window.set_minimized(true);
}

pub(super) fn open_file_location(
    files: &[String],
    mut status: Signal<String>,
    language: AppLanguage,
) {
    let mut missing_count = 0usize;

    for file in files
        .iter()
        .map(|file| file.trim())
        .filter(|file| !file.is_empty())
    {
        let path = Path::new(file);
        match path.try_exists() {
            Ok(true) => match open_path_location(path, language) {
                Ok(()) => status.set(match language {
                    AppLanguage::Chinese => format!("已打开文件位置：{file}"),
                    AppLanguage::English => format!("Opened file location: {file}"),
                }),
                Err(error) => status.set(match language {
                    AppLanguage::Chinese => format!("打开文件位置失败：{error}"),
                    AppLanguage::English => format!("Failed to open file location: {error}"),
                }),
            },
            Ok(false) => {
                missing_count += 1;
                continue;
            }
            Err(error) => status.set(match language {
                AppLanguage::Chinese => format!("无法访问文件：{file}（{error}）"),
                AppLanguage::English => format!("Cannot access file: {file} ({error})"),
            }),
        }
        return;
    }

    if missing_count == 0 {
        status.set(i18n::tr(language).empty_file_path.to_string());
    } else if missing_count == 1 {
        status.set(i18n::tr(language).file_missing.to_string());
    } else {
        status.set(match language {
            AppLanguage::Chinese => format!("{missing_count} 个文件已不存在"),
            AppLanguage::English => format!("{missing_count} files no longer exist"),
        });
    }
}

#[cfg(windows)]
fn open_path_location(path: &Path, language: AppLanguage) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|error| match language {
            AppLanguage::Chinese => format!("无法打开资源管理器：{error}"),
            AppLanguage::English => format!("Failed to open File Explorer: {error}"),
        })
}

#[cfg(not(windows))]
fn open_path_location(_path: &Path, language: AppLanguage) -> Result<(), String> {
    Err(match language {
        AppLanguage::Chinese => "当前平台暂不支持打开文件位置".to_string(),
        AppLanguage::English => {
            "Opening file locations is not supported on this platform".to_string()
        }
    })
}

fn validate_files_for_copy(files: &[String], language: AppLanguage) -> Result<(), String> {
    if files.is_empty() {
        return Err(match language {
            AppLanguage::Chinese => "文件列表为空".to_string(),
            AppLanguage::English => "File list is empty".to_string(),
        });
    }

    let mut missing_files = Vec::new();
    for file in files {
        let file = file.trim();
        if file.is_empty() {
            return Err(i18n::tr(language).empty_file_path.to_string());
        }

        match Path::new(file).try_exists() {
            Ok(true) => {}
            Ok(false) => missing_files.push(file.to_string()),
            Err(error) => {
                return Err(match language {
                    AppLanguage::Chinese => format!("无法访问文件：{file}（{error}）"),
                    AppLanguage::English => format!("Cannot access file: {file} ({error})"),
                });
            }
        }
    }

    match missing_files.as_slice() {
        [] => Ok(()),
        [file] => Err(match language {
            AppLanguage::Chinese => format!("文件已不存在：{file}"),
            AppLanguage::English => format!("File no longer exists: {file}"),
        }),
        files => Err(match language {
            AppLanguage::Chinese => format!("{} 个文件已不存在", files.len()),
            AppLanguage::English => format!("{} files no longer exist", files.len()),
        }),
    }
}

pub(super) fn save_entry_with_status(
    entry: &ClipboardEntry,
    mut status: Signal<String>,
    success: &str,
    language: AppLanguage,
) {
    match storage::save_entry(entry) {
        Ok(()) => status.set(success.to_string()),
        Err(error) => status.set(match language {
            AppLanguage::Chinese => format!("历史保存失败：{error}"),
            AppLanguage::English => format!("Failed to save history: {error}"),
        }),
    }
}

pub(super) fn delete_entries_with_animation(
    mut ids: Vec<u64>,
    mut deleting_ids: Signal<Vec<u64>>,
    mut history: Signal<ClipboardHistory>,
    mut status: Signal<String>,
    language: AppLanguage,
    success_message: &'static str,
    preserve_favorites_on_delete: bool,
) {
    let requested_count = ids.len();
    ids = history
        .read()
        .deletable_ids(&ids, preserve_favorites_on_delete);
    if requested_count > 0 && ids.is_empty() && preserve_favorites_on_delete {
        status.set(i18n::tr(language).favorite_preserved.to_string());
        return;
    }
    let current_deleting_ids = deleting_ids.read().clone();
    ids.retain(|id| !current_deleting_ids.contains(id));
    if ids.is_empty() {
        return;
    }

    let mut next_deleting_ids = current_deleting_ids;
    next_deleting_ids.extend(ids.iter().copied());
    deleting_ids.set(next_deleting_ids);

    spawn(async move {
        Delay::new(DELETE_EXIT_DELAY).await;

        let removed_ids = {
            let mut history = history.write();
            ids.iter()
                .copied()
                .filter(|id| history.remove(*id))
                .collect::<Vec<_>>()
        };

        deleting_ids
            .write()
            .retain(|id| !ids.iter().any(|removed_id| removed_id == id));

        if removed_ids.is_empty() {
            return;
        }

        match storage::delete_entries(&removed_ids) {
            Ok(()) => status.set(success_message.to_string()),
            Err(error) => status.set(match language {
                AppLanguage::Chinese => format!("历史删除失败：{error}"),
                AppLanguage::English => format!("Failed to delete history: {error}"),
            }),
        }
    });
}

pub(super) fn save_image_file(
    id: u64,
    mut image: ClipboardImage,
    default_file_name: String,
    mut status: Signal<String>,
    language: AppLanguage,
) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter(i18n::tr(language).png_image_filter, &["png"])
        .set_file_name(default_file_name)
        .save_file()
    else {
        return;
    };

    if !image.has_bytes() {
        let Some(loaded_image) = load_image_for_action(id, status, language) else {
            return;
        };
        image = loaded_image;
    }

    let Some(png) = image.to_png_bytes() else {
        status.set(i18n::tr(language).invalid_image_data.to_string());
        return;
    };

    let path = path_with_png_extension(path);
    match std::fs::write(&path, png) {
        Ok(()) => status.set(match language {
            AppLanguage::Chinese => format!("已保存图片：{}", path.display()),
            AppLanguage::English => format!("Saved image: {}", path.display()),
        }),
        Err(error) => status.set(match language {
            AppLanguage::Chinese => format!("保存图片失败：{error}"),
            AppLanguage::English => format!("Failed to save image: {error}"),
        }),
    }
}

fn load_image_for_action(
    id: u64,
    mut status: Signal<String>,
    language: AppLanguage,
) -> Option<ClipboardImage> {
    match storage::load_image(id) {
        Ok(Some(image)) if image.has_bytes() => Some(image),
        Ok(Some(_)) | Ok(None) => {
            status.set(i18n::tr(language).image_original_missing.to_string());
            None
        }
        Err(error) => {
            status.set(match language {
                AppLanguage::Chinese => format!("图片读取失败：{error}"),
                AppLanguage::English => format!("Failed to read image: {error}"),
            });
            None
        }
    }
}

fn path_with_png_extension(mut path: PathBuf) -> PathBuf {
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
    {
        path.set_extension("png");
    }

    path
}

pub(super) fn delete_focused_or_selected(
    entry_ids: &[u64],
    focused_id: Option<u64>,
    selected_ids: &mut Signal<Vec<u64>>,
    selection_anchor_id: &mut Signal<Option<u64>>,
    deleting_ids: Signal<Vec<u64>>,
    history: Signal<ClipboardHistory>,
    status: Signal<String>,
    language: AppLanguage,
    preserve_favorites_on_delete: bool,
) {
    let mut ids = selected_ids
        .read()
        .iter()
        .copied()
        .filter(|id| entry_ids.contains(id))
        .collect::<Vec<_>>();

    if ids.is_empty()
        && let Some(id) = focused_entry_id(entry_ids, focused_id)
    {
        ids.push(id);
    }

    let success_message = if ids.len() > 1 {
        i18n::tr(language).selected_history_deleted
    } else {
        i18n::tr(language).history_deleted
    };
    delete_entries_with_animation(
        ids,
        deleting_ids,
        history,
        status,
        language,
        success_message,
        preserve_favorites_on_delete,
    );

    selected_ids.set(Vec::new());
    selection_anchor_id.set(None);
}

fn copy_failed(language: AppLanguage, error: &str) -> String {
    match language {
        AppLanguage::Chinese => format!("复制失败：{error}"),
        AppLanguage::English => format!("Copy failed: {error}"),
    }
}

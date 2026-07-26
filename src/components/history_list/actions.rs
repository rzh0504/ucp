use super::selection::focused_entry_id;
use crate::i18n;
use crate::model::{AppLanguage, ClipboardContent, ClipboardEntry, ClipboardHistory, ClipboardImage};
use crate::platform;
use crate::services::ClipboardService;
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
    // Use service layer for business logic
    let result = ClipboardService::copy_entry(&history.read(), id, promote_on_copy);
    
    match result {
        Ok(copy_result) => {
            if !promote_on_copy {
                if let Some(content) = copy_result.content {
                    ignored_clipboard_write.set(Some(content));
                }
            }
            
            let copied_to_clipboard = i18n::tr(language).copied_to_clipboard;
            
            if copy_result.should_promote {
                if let Some(entry) = ClipboardService::promote_entry(&mut history.write(), id).ok() {
                    save_entry_with_status(&entry, status, copied_to_clipboard, language);
                } else {
                    status.set(copied_to_clipboard.to_string());
                }
            } else {
                status.set(copied_to_clipboard.to_string());
            }
            
            true
        }
        Err(error) => {
            status.set(error.to_localized_string(language));
            false
        }
    }
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
    // Validate path doesn't contain dangerous characters
    let path_str = path.to_string_lossy();
    if path_str.contains('\0') {
        return Err(match language {
            AppLanguage::Chinese => "文件路径包含非法字符".to_string(),
            AppLanguage::English => "File path contains invalid characters".to_string(),
        });
    }
    
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(path.as_os_str())
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

pub(super) fn save_entry_with_status(
    entry: &ClipboardEntry,
    mut status: Signal<String>,
    success: &str,
    language: AppLanguage,
) {
    match storage::save_entry(entry) {
        Ok(_) => status.set(success.to_string()),
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

        // Use service layer for deletion
        match ClipboardService::delete_entries(&mut history.write(), &ids, preserve_favorites_on_delete) {
            Ok(removed_ids) if !removed_ids.is_empty() => {
                deleting_ids
                    .write()
                    .retain(|id| !removed_ids.contains(id));
                status.set(success_message.to_string());
            }
            Ok(_) => {
                // No entries were deleted
                deleting_ids
                    .write()
                    .retain(|id| !ids.contains(id));
            }
            Err(error) => {
                deleting_ids
                    .write()
                    .retain(|id| !ids.contains(id));
                status.set(error.to_localized_string(language));
            }
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

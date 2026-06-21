use super::selection::focused_entry_id;
use crate::model::{ClipboardContent, ClipboardEntry, ClipboardHistory, ClipboardImage};
use crate::platform;
use crate::storage;
use dioxus::prelude::*;
use futures_timer::Delay;
use std::path::{Path, PathBuf};
use std::time::Duration;

const QUICK_PASTE_DELAY: Duration = Duration::from_millis(260);

pub(super) fn copy_entry(
    id: u64,
    mut history: Signal<ClipboardHistory>,
    promote_on_copy: bool,
    mut status: Signal<String>,
) -> bool {
    let Some(mut content) = history.read().entry(id).map(|entry| entry.content.clone()) else {
        return false;
    };

    if let ClipboardContent::Image(image) = &content
        && !image.has_bytes()
    {
        let Some(image) = load_image_for_action(id, status) else {
            return false;
        };
        content = ClipboardContent::Image(image);
    }

    if let ClipboardContent::Files(files) = &content
        && let Err(error) = validate_files_for_copy(files)
    {
        status.set(format!("复制失败：{error}"));
        return false;
    }

    if let Err(error) = platform::clipboard::write_content(&content) {
        status.set(format!("复制失败：{error}"));
        return false;
    }

    if promote_on_copy {
        if let Some(entry) = history.write().promote(id) {
            save_entry_with_status(&entry, status, "已复制到剪贴板");
        } else {
            status.set("已复制到剪贴板".to_string());
        }
    } else {
        status.set("已复制到剪贴板".to_string());
    }

    true
}

pub(super) fn run_quick_paste_shortcut(mut status: Signal<String>) {
    status.set("正在切换窗口并粘贴...".to_string());
    spawn(async move {
        Delay::new(QUICK_PASTE_DELAY).await;
        match platform::clipboard::paste_shortcut() {
            Ok(()) => status.set("已快捷粘贴".to_string()),
            Err(error) => status.set(format!("快捷粘贴失败：{error}")),
        }
    });
}

pub(super) fn open_file_location(files: &[String], mut status: Signal<String>) {
    let mut missing_count = 0usize;

    for file in files
        .iter()
        .map(|file| file.trim())
        .filter(|file| !file.is_empty())
    {
        let path = Path::new(file);
        match path.try_exists() {
            Ok(true) => match open_path_location(path) {
                Ok(()) => status.set(format!("已打开文件位置：{file}")),
                Err(error) => status.set(format!("打开文件位置失败：{error}")),
            },
            Ok(false) => {
                missing_count += 1;
                continue;
            }
            Err(error) => status.set(format!("无法访问文件：{file}（{error}）")),
        }
        return;
    }

    if missing_count == 0 {
        status.set("文件路径为空".to_string());
    } else if missing_count == 1 {
        status.set("文件已不存在".to_string());
    } else {
        status.set(format!("{missing_count} 个文件已不存在"));
    }
}

#[cfg(windows)]
fn open_path_location(path: &Path) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开资源管理器：{error}"))
}

#[cfg(not(windows))]
fn open_path_location(_path: &Path) -> Result<(), String> {
    Err("当前平台暂不支持打开文件位置".to_string())
}

fn validate_files_for_copy(files: &[String]) -> Result<(), String> {
    if files.is_empty() {
        return Err("文件列表为空".to_string());
    }

    let mut missing_files = Vec::new();
    for file in files {
        let file = file.trim();
        if file.is_empty() {
            return Err("文件路径为空".to_string());
        }

        match Path::new(file).try_exists() {
            Ok(true) => {}
            Ok(false) => missing_files.push(file.to_string()),
            Err(error) => return Err(format!("无法访问文件：{file}（{error}）")),
        }
    }

    match missing_files.as_slice() {
        [] => Ok(()),
        [file] => Err(format!("文件已不存在：{file}")),
        files => Err(format!("{} 个文件已不存在", files.len())),
    }
}

pub(super) fn save_entry_with_status(
    entry: &ClipboardEntry,
    mut status: Signal<String>,
    success: &str,
) {
    match storage::save_entry(entry) {
        Ok(()) => status.set(success.to_string()),
        Err(error) => status.set(format!("历史保存失败：{error}")),
    }
}

pub(super) fn delete_entry_with_status(id: u64, mut status: Signal<String>) {
    match storage::delete_entry(id) {
        Ok(()) => status.set("历史已删除".to_string()),
        Err(error) => status.set(format!("历史删除失败：{error}")),
    }
}

pub(super) fn save_image_file(
    id: u64,
    mut image: ClipboardImage,
    default_file_name: String,
    mut status: Signal<String>,
) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("PNG 图像", &["png"])
        .set_file_name(default_file_name)
        .save_file()
    else {
        return;
    };

    if !image.has_bytes() {
        let Some(loaded_image) = load_image_for_action(id, status) else {
            return;
        };
        image = loaded_image;
    }

    let Some(png) = image.to_png_bytes() else {
        status.set("保存图片失败：图像数据无效".to_string());
        return;
    };

    let path = path_with_png_extension(path);
    match std::fs::write(&path, png) {
        Ok(()) => status.set(format!("已保存图片：{}", path.display())),
        Err(error) => status.set(format!("保存图片失败：{error}")),
    }
}

fn load_image_for_action(id: u64, mut status: Signal<String>) -> Option<ClipboardImage> {
    match storage::load_image(id) {
        Ok(Some(image)) if image.has_bytes() => Some(image),
        Ok(Some(_)) | Ok(None) => {
            status.set("图片原始数据不存在，无法操作".to_string());
            None
        }
        Err(error) => {
            status.set(format!("图片读取失败：{error}"));
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
    mut history: Signal<ClipboardHistory>,
    status: Signal<String>,
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

    for id in ids {
        if history.write().remove(id) {
            delete_entry_with_status(id, status);
        }
    }

    selected_ids.set(Vec::new());
    selection_anchor_id.set(None);
}

use std::path::{Path, PathBuf};

#[cfg(windows)]
pub fn icon_path(file: &Path) -> Option<PathBuf> {
    windows_icon_path(file)
}

#[cfg(not(windows))]
pub fn icon_path(_file: &Path) -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn windows_icon_path(file: &Path) -> Option<PathBuf> {
    use image::{ImageBuffer, Rgba, imageops::FilterType};
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

    let cache_dir = std::env::temp_dir().join("ucp-file-icons");
    fs::create_dir_all(&cache_dir).ok()?;
    let key = format!("{:x}", Sha256::digest(file.to_string_lossy().as_bytes()));
    let output = cache_dir.join(format!("v7-{key}.png"));
    if output.is_file() {
        return Some(output);
    }

    let wide = file
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut shell_info = SHFILEINFOW::default();
    let flags = SHGFI_ICON | SHGFI_LARGEICON;
    let result = unsafe {
        SHGetFileInfoW(
            wide.as_ptr(),
            0,
            &mut shell_info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            flags,
        )
    };
    if result == 0 || shell_info.hIcon.is_null() {
        return None;
    }

    let icon = shell_info.hIcon;
    let result = unsafe { icon_to_rgba(icon) };
    unsafe { DestroyIcon(icon) };
    let (width, height, pixels) = result?;
    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, pixels)?;
    let image = image::imageops::resize(&image, 32, 32, FilterType::Lanczos3);
    image.save(&output).ok()?;
    Some(output)
}

#[cfg(windows)]
#[allow(unsafe_op_in_unsafe_fn)]
unsafe fn icon_to_rgba(
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
) -> Option<(u32, u32, Vec<u8>)> {
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut icon_info = ICONINFO::default();
    if GetIconInfo(icon, &mut icon_info) == 0 {
        return None;
    }
    let color_bitmap = icon_info.hbmColor;
    let mask_bitmap = icon_info.hbmMask;
    let mut bitmap = BITMAP::default();
    if GetObjectW(
        color_bitmap as _,
        std::mem::size_of::<BITMAP>() as i32,
        &mut bitmap as *mut _ as _,
    ) == 0
    {
        DeleteObject(color_bitmap as _);
        DeleteObject(mask_bitmap as _);
        return None;
    }
    let width = bitmap.bmWidth.max(1) as u32;
    let height = bitmap.bmHeight.max(1) as u32;
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let dc = CreateCompatibleDC(std::ptr::null_mut());
    let copied = GetDIBits(
        dc,
        color_bitmap,
        0,
        height,
        pixels.as_mut_ptr() as _,
        &mut info,
        DIB_RGB_COLORS,
    );
    DeleteDC(dc);
    DeleteObject(color_bitmap as _);
    DeleteObject(mask_bitmap as _);
    if copied == 0 {
        return None;
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some((width, height, pixels))
}

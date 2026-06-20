#[cfg(windows)]
mod imp {
    use base64::{Engine as _, engine::general_purpose};
    use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, RGBQUAD, SelectObject,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    };
    use windows_sys::Win32::UI::Shell::{
        SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx};

    const ICON_SIZE: i32 = 32;
    static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

    pub fn data_url(path: &str) -> Option<String> {
        let key = cache_key(path)?;
        let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(icon) = cache.lock().ok()?.get(&key).cloned() {
            return icon;
        }

        let icon = icon_png(path).map(|png| {
            format!(
                "data:image/png;base64,{}",
                general_purpose::STANDARD.encode(png)
            )
        });

        cache.lock().ok()?.insert(key, icon.clone());
        icon
    }

    fn cache_key(path: &str) -> Option<String> {
        let path = path.trim();
        if path.is_empty() {
            return None;
        }

        let path_ref = Path::new(path);
        if path_ref.is_dir() {
            return Some("dir".to_string());
        }

        path_ref
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!("ext:{}", extension.to_ascii_lowercase()))
            .or_else(|| Some(format!("path:{}", path.to_ascii_lowercase())))
    }

    fn icon_png(path: &str) -> Option<Vec<u8>> {
        let path = path.trim();
        if path.is_empty() {
            return None;
        }

        let hicon = shell_icon(path)?;
        let rgba = unsafe { icon_rgba(hicon, ICON_SIZE, ICON_SIZE) };

        unsafe {
            DestroyIcon(hicon);
        }

        encode_png(&rgba?, ICON_SIZE as u32, ICON_SIZE as u32)
    }

    fn shell_icon(path: &str) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
        let path_ref = Path::new(path);
        let attributes = if path_ref.is_dir() {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        };
        let wide_path = path_ref
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut info = unsafe { zeroed::<SHFILEINFOW>() };
        let result = unsafe {
            SHGetFileInfoW(
                wide_path.as_ptr(),
                attributes,
                &mut info,
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES,
            )
        };

        if result == 0 || info.hIcon.is_null() {
            None
        } else {
            Some(info.hIcon)
        }
    }

    unsafe fn icon_rgba(
        hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
        width: i32,
        height: i32,
    ) -> Option<Vec<u8>> {
        let hdc = unsafe { CreateCompatibleDC(std::ptr::null_mut()) };
        if hdc.is_null() {
            return None;
        }

        let mut bits = std::ptr::null_mut::<c_void>();
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..unsafe { zeroed() }
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        let bitmap = unsafe {
            CreateDIBSection(
                hdc,
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            )
        };

        if bitmap.is_null() || bits.is_null() {
            unsafe {
                DeleteDC(hdc);
            }
            return None;
        }

        let previous = unsafe { SelectObject(hdc, bitmap) };
        let drawn = unsafe {
            DrawIconEx(
                hdc,
                0,
                0,
                hicon,
                width,
                height,
                0,
                std::ptr::null_mut(),
                DI_NORMAL,
            )
        };
        let byte_len = width as usize * height as usize * 4;
        let bgra = unsafe { std::slice::from_raw_parts(bits as *const u8, byte_len) };
        let rgba = if drawn != 0 {
            Some(bgra_to_rgba(bgra))
        } else {
            None
        };

        unsafe {
            SelectObject(hdc, previous);
            DeleteObject(bitmap);
            DeleteDC(hdc);
        }

        rgba
    }

    fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
        let has_alpha = bgra.chunks_exact(4).any(|pixel| pixel[3] != 0);
        let mut rgba = Vec::with_capacity(bgra.len());

        for pixel in bgra.chunks_exact(4) {
            rgba.push(pixel[2]);
            rgba.push(pixel[1]);
            rgba.push(pixel[0]);
            rgba.push(if has_alpha { pixel[3] } else { 255 });
        }

        rgba
    }

    fn encode_png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(rgba, width, height, ColorType::Rgba8.into())
            .ok()?;

        Some(png)
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn data_url(_path: &str) -> Option<String> {
        None
    }
}

pub fn data_url(path: &str) -> Option<String> {
    imp::data_url(path)
}

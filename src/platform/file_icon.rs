#[cfg(windows)]
mod imp {
    use base64::{Engine as _, engine::general_purpose};
    use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
    use std::collections::HashMap;
    use std::ffi::c_void;
    use std::fs;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, RGBQUAD, SelectObject,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL,
    };
    use windows_sys::Win32::UI::Shell::{
        SHFILEINFOW, SHGFI_ICON, SHGFI_SYSICONINDEX, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
        SHGetImageList, SHIL_EXTRALARGE, SHIL_LARGE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx, HICON};
    use windows_sys::core::{GUID, HRESULT, IUnknown_Vtbl};

    const EXTRA_LARGE_ICON_SIZE: i32 = 48;
    const LARGE_ICON_SIZE: i32 = 32;
    const APP_DIR: &str = "UCP";
    const CACHE_DIR: &str = "cache";
    const FILE_ICON_CACHE_DIR: &str = "file-icons";
    const IID_IIMAGELIST: GUID = GUID::from_u128(0x46eb5926_582e_4017_9fdf_e8998daa0950);
    const ILD_TRANSPARENT: u32 = 1;
    static ICON_CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

    #[repr(C)]
    struct IImageList {
        vtbl: *const IImageListVtbl,
    }

    #[repr(C)]
    struct IImageListVtbl {
        base: IUnknown_Vtbl,
        add: usize,
        replace_icon: usize,
        set_overlay_image: usize,
        replace: usize,
        add_masked: usize,
        draw: usize,
        remove: usize,
        get_icon: unsafe extern "system" fn(*mut c_void, i32, u32, *mut HICON) -> HRESULT,
    }

    pub fn data_url(path: &str) -> Option<String> {
        let key = cache_key(path)?;
        let cache = ICON_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(icon) = cache.lock().ok()?.get(&key).cloned() {
            return icon;
        }

        if let Some(icon) = cached_file_icon_url(&key) {
            cache.lock().ok()?.insert(key, Some(icon.clone()));
            return Some(icon);
        }

        let icon = icon_png(path).map(|png| {
            write_cached_file_icon(&key, &png).unwrap_or_else(|| {
                format!(
                    "data:image/png;base64,{}",
                    general_purpose::STANDARD.encode(png)
                )
            })
        });

        cache.lock().ok()?.insert(key, icon.clone());
        icon
    }

    fn cache_key(path: &str) -> Option<String> {
        let path = path.trim();
        if path.is_empty() {
            return None;
        }

        if is_directory_like_path(path) {
            return Some("dir".to_string());
        }

        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!("ext:{}", extension.to_ascii_lowercase()))
            .or_else(|| Some(format!("path:{}", path.to_ascii_lowercase())))
    }

    fn is_directory_like_path(path: &str) -> bool {
        path.ends_with(['\\', '/'])
    }

    fn cached_file_icon_url(key: &str) -> Option<String> {
        let path = file_icon_cache_path(key)?;
        fs::read(path).ok().map(data_url_from_png)
    }

    fn write_cached_file_icon(key: &str, png: &[u8]) -> Option<String> {
        let path = file_icon_cache_path(key)?;
        fs::create_dir_all(path.parent()?).ok()?;
        fs::write(&path, png).ok()?;
        Some(data_url_from_png(png.to_vec()))
    }

    fn file_icon_cache_path(key: &str) -> Option<PathBuf> {
        Some(cache_directory()?.join(format!("{}.png", cache_file_name(key))))
    }

    fn cache_directory() -> Option<PathBuf> {
        Some(
            std::env::var_os("LOCALAPPDATA")
                .or_else(|| std::env::var_os("APPDATA"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(APP_DIR)
                .join(CACHE_DIR)
                .join(FILE_ICON_CACHE_DIR),
        )
    }

    fn cache_file_name(key: &str) -> String {
        use sha2::{Digest, Sha256};

        format!("{:x}", Sha256::digest(key.as_bytes()))
    }

    fn data_url_from_png(png: Vec<u8>) -> String {
        format!(
            "data:image/png;base64,{}",
            general_purpose::STANDARD.encode(png)
        )
    }

    fn icon_png(path: &str) -> Option<Vec<u8>> {
        let path = path.trim();
        if path.is_empty() {
            return None;
        }

        if let Some(png) = system_icon_png(path) {
            return Some(png);
        }

        let hicon = shell_icon(path)?;
        let rgba = unsafe { icon_rgba(hicon, LARGE_ICON_SIZE, LARGE_ICON_SIZE) };

        unsafe {
            DestroyIcon(hicon);
        }

        encode_png(&rgba?, LARGE_ICON_SIZE as u32, LARGE_ICON_SIZE as u32)
    }

    fn system_icon_png(path: &str) -> Option<Vec<u8>> {
        let index = system_icon_index(path)?;
        let icon = unsafe { image_list_icon(index) }?;
        let rgba = unsafe { icon_rgba(icon.hicon, icon.size, icon.size) };

        unsafe {
            DestroyIcon(icon.hicon);
        }

        encode_png(&rgba?, icon.size as u32, icon.size as u32)
    }

    fn system_icon_index(path: &str) -> Option<i32> {
        let path_ref = Path::new(path);
        let wide_path = wide_path(path_ref);
        let mut info = unsafe { zeroed::<SHFILEINFOW>() };
        let result = unsafe {
            SHGetFileInfoW(
                wide_path.as_ptr(),
                file_attributes(path_ref),
                &mut info,
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES,
            )
        };

        if result == 0 { None } else { Some(info.iIcon) }
    }

    struct IconHandle {
        hicon: HICON,
        size: i32,
    }

    unsafe fn image_list_icon(index: i32) -> Option<IconHandle> {
        for (list, size) in [
            (SHIL_EXTRALARGE, EXTRA_LARGE_ICON_SIZE),
            (SHIL_LARGE, LARGE_ICON_SIZE),
        ] {
            let mut image_list = std::ptr::null_mut::<c_void>();
            let result = unsafe { SHGetImageList(list as i32, &IID_IIMAGELIST, &mut image_list) };
            if result < 0 || image_list.is_null() {
                continue;
            }

            let image_list = image_list as *mut IImageList;
            let vtbl = unsafe { (*image_list).vtbl };
            let mut hicon = std::ptr::null_mut::<c_void>();
            let result = unsafe {
                ((*vtbl).get_icon)(
                    image_list as *mut c_void,
                    index,
                    ILD_TRANSPARENT,
                    &mut hicon,
                )
            };
            unsafe {
                ((*vtbl).base.Release)(image_list as *mut c_void);
            }

            if result >= 0 && !hicon.is_null() {
                return Some(IconHandle { hicon, size });
            }
        }

        None
    }

    fn shell_icon(path: &str) -> Option<windows_sys::Win32::UI::WindowsAndMessaging::HICON> {
        let path_ref = Path::new(path);
        let wide_path = wide_path(path_ref);
        let mut info = unsafe { zeroed::<SHFILEINFOW>() };
        let result = unsafe {
            SHGetFileInfoW(
                wide_path.as_ptr(),
                file_attributes(path_ref),
                &mut info,
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_USEFILEATTRIBUTES,
            )
        };

        if result == 0 || info.hIcon.is_null() {
            None
        } else {
            Some(info.hIcon)
        }
    }

    fn file_attributes(path: &Path) -> u32 {
        if path.is_dir() {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        }
    }

    fn wide_path(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
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

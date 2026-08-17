#[cfg(windows)]
use std::sync::atomic::{AtomicU8, Ordering};

#[cfg(windows)]
const SHOW: u8 = 1;
#[cfg(windows)]
const QUIT: u8 = 2;
#[cfg(windows)]
static PENDING_ACTIONS: AtomicU8 = AtomicU8::new(0);

#[cfg(windows)]
pub struct TrayIcon {
    _icon: tray_icon::TrayIcon,
}

#[cfg(windows)]
pub fn create() -> Result<TrayIcon, String> {
    use tray_icon::{
        TrayIconBuilder, TrayIconEvent,
        menu::{Menu, MenuEvent, MenuItem},
    };

    let show = MenuItem::with_id("show", "显示主窗口", true, None);
    let quit = MenuItem::with_id("quit", "退出", true, None);
    let menu = Menu::with_items(&[&show, &quit]).map_err(|error| error.to_string())?;
    let icon = load_icon()?;

    MenuEvent::set_event_handler(Some(|event: MenuEvent| match event.id.0.as_str() {
        "show" => request(SHOW),
        "quit" => request(QUIT),
        _ => {}
    }));
    TrayIconEvent::set_event_handler(Some(|event| {
        if matches!(
            event,
            TrayIconEvent::Click {
                button: tray_icon::MouseButton::Left,
                button_state: tray_icon::MouseButtonState::Up,
                ..
            }
        ) {
            request(SHOW);
        }
    }));

    let icon = TrayIconBuilder::new()
        .with_icon(icon)
        .with_tooltip("UCP Clipboard")
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| error.to_string())?;

    Ok(TrayIcon { _icon: icon })
}

#[cfg(windows)]
pub fn take_show_request() -> bool {
    take(SHOW)
}

#[cfg(windows)]
pub fn take_quit_request() -> bool {
    take(QUIT)
}

#[cfg(windows)]
fn request(action: u8) {
    PENDING_ACTIONS.fetch_or(action, Ordering::Release);
}

#[cfg(windows)]
fn take(action: u8) -> bool {
    PENDING_ACTIONS.fetch_and(!action, Ordering::AcqRel) & action != 0
}

#[cfg(windows)]
fn load_icon() -> Result<tray_icon::Icon, String> {
    let image = image::load_from_memory(include_bytes!("../../assets/icons/Ucp.png"))
        .map_err(|error| error.to_string())?
        .into_rgba8();
    let (width, height) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), width, height).map_err(|error| error.to_string())
}

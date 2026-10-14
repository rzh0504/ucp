use crate::model::{ClipboardContent, ClipboardEntry, ClipboardFilter, ClipboardHistory};
use gpui_kit::component::{
    ActiveTheme as _, WindowExt as _,
    input::{Input, InputState},
    notification::Notification,
};
use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
use gpui_kit::{
    AppContext as _, AssetSource as _, Bounds, Context, Entity, IntoElement, ParentElement as _,
    Render, Styled as _, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

struct HistoryView {
    history: ClipboardHistory,
    query: String,
    filter: ClipboardFilter,
    visible_entries: Vec<std::sync::Arc<ClipboardEntry>>,
}

impl HistoryView {
    fn new() -> Self {
        Self {
            history: ClipboardHistory::from_entries(
                10,
                vec![
                    ClipboardEntry::new(1, ClipboardContent::Text("alpha".into())),
                    ClipboardEntry::new(2, ClipboardContent::Text("beta".into())),
                ],
            ),
            query: String::new(),
            filter: ClipboardFilter::All,
            visible_entries: Vec::new(),
        }
    }

    fn refresh(&mut self) {
        self.visible_entries = self.history.filtered(&self.query, self.filter);
    }
}

#[gpui_kit::test]
async fn history_view_refreshes_visible_entries_from_search(cx: &mut TestAppContext) {
    let view = cx.new(|_| HistoryView::new());
    view.update(cx, |view, _| view.refresh());
    assert_eq!(cx.read(|cx| view.read(cx).visible_entries.len()), 2);

    view.update(cx, |view, _| {
        view.query = "alp".into();
        view.refresh();
    });
    cx.read(|cx| {
        let view = view.read(cx);
        assert_eq!(view.visible_entries.len(), 1);
        assert_eq!(view.visible_entries[0].id, 1);
    });
}

#[gpui_kit::test]
async fn history_view_applies_filter_and_preserves_order(cx: &mut TestAppContext) {
    let view = cx.new(|_| HistoryView::new());
    view.update(cx, |view, _| {
        view.filter = ClipboardFilter::Text;
        view.refresh();
    });
    cx.read(|cx| {
        let view = view.read(cx);
        assert_eq!(
            view.visible_entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            vec![2, 1]
        );
    });
}

// Exercise the production UI initialization and Kit window entry point without
// ClipboardApp::new, which opens the user's database and starts native listeners.
struct UiProbe {
    search: Entity<InputState>,
}

impl Render for UiProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_4()
            .child(Input::new(&self.search).id("search"))
    }
}

fn open_ui_probe(cx: &mut TestAppContext) -> (gpui_kit::AnyWindowHandle, Entity<UiProbe>) {
    cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: Default::default(),
                    size: size(px(900.), px(660.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| UiProbe {
                    search: cx.new(|cx| InputState::new(window, cx)),
                })
            },
        )
        .expect("open headless UI probe")
    })
}

#[gpui_kit::test]
fn kit_window_hosts_overlays_once_and_only_in_the_target_window(cx: &mut TestAppContext) {
    cx.update(|cx| {
        super::init_ui(cx);
        cx.set_reduce_motion(true);
    });
    let (first, content) = open_ui_probe(cx);
    let (second, _) = open_ui_probe(cx);
    let root = first
        .downcast::<gpui_kit::base::Root>()
        .expect("Kit must wrap content in a Base Root");
    root.read_with(cx, |root, _| {
        assert_eq!(root.view().entity_id(), content.entity_id());
    })
    .unwrap();

    cx.update_window(first, |_, window, cx| {
        window.open_alert_dialog(cx, |alert, _, _| alert.title("清空历史？"));
        window.push_notification(Notification::error("回归测试通知").autohide(false), cx);
        window.render_frame(cx);
        // find() also rejects duplicate identities, catching double mounting.
        assert!(window.find("dialog").visible());
        assert!(window.find("notification").visible());
    })
    .unwrap();
    cx.update_window(second, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("dialog").is_none());
        assert!(window.try_find("notification").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
async fn search_input_survives_alert_confirmation_and_focus_restore(cx: &mut TestAppContext) {
    cx.update(|cx| {
        super::init_ui(cx);
        cx.set_reduce_motion(true);
    });
    let (handle, _) = open_ui_probe(cx);
    let confirmed = Rc::new(Cell::new(0));
    cx.update_window(handle, |_, window, cx| {
        window.click("search", cx);
        window.input("alpha 中文", cx);
        assert_eq!(window.find("search").value(), Some("alpha 中文"));
        assert_eq!(window.find("search").focused(), Some(true));
        let confirmed = confirmed.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let confirmed = confirmed.clone();
            alert
                .title("清空历史？")
                .button_props(
                    super::DialogButtonProps::default()
                        .ok_text("清空")
                        .cancel_text("取消")
                        .show_cancel(true),
                )
                .on_ok(move |_, _, _| {
                    confirmed.set(confirmed.get() + 1);
                    true
                })
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("ok", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_none() && window.find("search").focused() == Some(true)
    })
    .await;
    assert_eq!(confirmed.get(), 1, "confirmation must run exactly once");
    cx.update_window(handle, |_, window, cx| {
        window.input(" beta", cx);
        assert_eq!(window.find("search").value(), Some("alpha 中文 beta"));
    })
    .unwrap();
}

#[gpui_kit::test]
async fn cancelling_an_alert_keeps_search_and_does_not_confirm(cx: &mut TestAppContext) {
    cx.update(|cx| {
        super::init_ui(cx);
        cx.set_reduce_motion(true);
    });
    let (handle, _) = open_ui_probe(cx);
    let confirmed = Rc::new(Cell::new(false));
    cx.update_window(handle, |_, window, cx| {
        window.click("search", cx);
        window.input("保留搜索", cx);
        let confirmed = confirmed.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let confirmed = confirmed.clone();
            alert
                .title("清空历史？")
                .button_props(super::DialogButtonProps::default().show_cancel(true))
                .on_ok(move |_, _, _| {
                    confirmed.set(true);
                    true
                })
        });
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_some()
    })
    .await;
    cx.update_window(handle, |_, window, cx| {
        window.within("dialog").click("cancel", cx);
    })
    .unwrap();
    cx.wait_for(handle, Duration::from_secs(1), |window, _| {
        window.try_find("dialog").is_none() && window.find("search").focused() == Some(true)
    })
    .await;
    assert!(
        !confirmed.get(),
        "cancel must not execute the destructive action"
    );
    cx.update_window(handle, |_, window, _| {
        assert_eq!(window.find("search").value(), Some("保留搜索"));
    })
    .unwrap();
}

#[gpui_kit::test]
fn bundled_themes_keep_the_application_palette_after_mode_changes(cx: &mut TestAppContext) {
    cx.update(super::init_ui);
    let (handle, _) = open_ui_probe(cx);
    cx.update_window(handle, |_, window, cx| {
        for (mode, expected) in [
            (crate::model::AppTheme::Dark, gpui_kit::rgb(0x181a1d)),
            (crate::model::AppTheme::Light, gpui_kit::rgb(0xf7f8fa)),
        ] {
            super::ClipboardApp::apply_theme(mode, cx);
            window.render_frame(cx);
            assert_eq!(cx.theme().background, expected.into());
            assert!(window.find("search").visible());
        }
    })
    .unwrap();
}

#[test]
fn bundled_assets_include_custom_icons_and_kit_fallbacks() {
    let assets = super::AppAssets(gpui_kit::assets::Assets);
    let listed = assets.list("icons").expect("list bundled icons");
    for path in [
        "icons/pin.svg",
        "icons/file-missing.svg",
        "icons/search.svg",
    ] {
        let content = assets.load(path).expect("load icon").expect("bundled icon");
        assert!(!content.is_empty(), "{path}");
        assert!(listed.iter().any(|listed| listed == path), "{path}");
    }
}

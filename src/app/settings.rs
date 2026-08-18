use super::{ClipboardApp, UpdateCheckState};
use crate::model::AppTheme;
use crate::platform;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    kbd::Kbd,
    label::Label,
    link::Link,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    v_flex,
};
use std::sync::Arc;

impl ClipboardApp {
    fn shortcut_item(label: &'static str, shortcut: &'static str) -> SettingItem {
        let keystroke = Keystroke::parse(shortcut).expect("shortcut must be valid");
        SettingItem::render(move |_, _, cx| {
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap_4()
                .child(Label::new(label).text_sm())
                .child(
                    Kbd::new(keystroke.clone())
                        .px_2()
                        .py_1()
                        .text_sm()
                        .text_color(cx.theme().foreground),
                )
        })
    }

    pub(super) fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let app = cx.entity().clone();
        let theme = SettingItem::new(
            "外观主题",
            SettingField::dropdown(
                vec![
                    ("light".into(), "浅色".into()),
                    ("dark".into(), "深色".into()),
                ],
                {
                    let app = app.clone();
                    move |cx| match app.read(cx).settings.theme {
                        AppTheme::Dark => "dark".into(),
                        AppTheme::System | AppTheme::Light => "light".into(),
                    }
                },
                {
                    let app = app.clone();
                    move |value, cx| {
                        app.update(cx, |this, cx| {
                            this.settings.theme = AppTheme::from_key(value.as_ref());
                            Self::apply_theme(this.settings.theme, cx);
                            this.save_settings();
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("切换应用的浅色或深色外观。");
        let monitor = SettingItem::new(
            "监听剪贴板",
            SettingField::switch(
                {
                    let app = app.clone();
                    move |cx| !app.read(cx).monitor_paused
                },
                {
                    let app = app.clone();
                    move |checked, cx| {
                        app.update(cx, |this, cx| {
                            this.monitor_paused = !checked;
                            this.status = if checked {
                                "剪贴板监听已开启".into()
                            } else {
                                "剪贴板监听已暂停".into()
                            };
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("在后台监听系统剪贴板并保存新的内容。");
        let startup = SettingItem::new(
            "开机启动",
            SettingField::switch(
                {
                    let app = app.clone();
                    move |cx| app.read(cx).settings.launch_at_startup
                },
                {
                    let app = app.clone();
                    move |checked, cx| {
                        app.update(cx, |this, cx| {
                            this.settings.launch_at_startup = checked;
                            if let Err(error) = platform::startup::set_enabled(checked) {
                                this.status = error;
                            } else {
                                this.save_settings();
                            }
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("启动系统时自动运行 UCP。");
        let promote = SettingItem::new(
            "复制后提升记录",
            SettingField::switch(
                {
                    let app = app.clone();
                    move |cx| app.read(cx).settings.promote_copied_entries
                },
                {
                    let app = app.clone();
                    move |checked, cx| {
                        app.update(cx, |this, cx| {
                            this.settings.promote_copied_entries = checked;
                            this.save_settings();
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("复制历史记录时，将其移动到列表顶部。");
        let show_copy_time = SettingItem::new(
            "显示复制时间",
            SettingField::switch(
                {
                    let app = app.clone();
                    move |cx| app.read(cx).settings.show_copy_time
                },
                {
                    let app = app.clone();
                    move |checked, cx| {
                        app.update(cx, |this, cx| {
                            this.settings.show_copy_time = checked;
                            this.save_settings();
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("在历史记录中显示内容的复制时间。");
        let show_text_length = SettingItem::new(
            "显示文本长度",
            SettingField::switch(
                {
                    let app = app.clone();
                    move |cx| app.read(cx).settings.show_text_length
                },
                {
                    let app = app.clone();
                    move |checked, cx| {
                        app.update(cx, |this, cx| {
                            this.settings.show_text_length = checked;
                            this.save_settings();
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("在文本记录中显示字符数量。");
        let quick_paste = SettingItem::new(
            "启用快捷粘贴",
            SettingField::switch(
                {
                    let app = app.clone();
                    move |cx| app.read(cx).settings.quick_paste
                },
                {
                    let app = app.clone();
                    move |checked, cx| {
                        app.update(cx, |this, cx| {
                            this.settings.quick_paste = checked;
                            this.save_settings();
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("复制文本记录后，自动粘贴到当前光标所在的输入框。");
        let double_click_copy = SettingItem::new(
            "双击快速复制",
            SettingField::switch(
                {
                    let app = app.clone();
                    move |cx| app.read(cx).settings.double_click_copy
                },
                {
                    let app = app.clone();
                    move |checked, cx| {
                        app.update(cx, |this, cx| {
                            this.settings.double_click_copy = checked;
                            this.save_settings();
                            cx.notify();
                        });
                    }
                },
            ),
        )
        .description("双击历史记录时立即将其复制到剪贴板。");

        let move_up = Self::shortcut_item("向上导航", "up");
        let move_down = Self::shortcut_item("向下导航", "down");
        let move_first = Self::shortcut_item("跳转到第一条", "home");
        let move_last = Self::shortcut_item("跳转到最后一条", "end");
        let clear_navigation = Self::shortcut_item("清除导航", "escape");

        let update_app = cx.entity().clone();
        let about = SettingItem::render(move |_, _, cx| {
            let update_state = update_app.read(cx).update_check.clone();
            let checking = matches!(update_state, UpdateCheckState::Checking);
            let status = match &update_state {
                UpdateCheckState::Idle => {
                    format!("当前是最新版本 {}。", env!("CARGO_PKG_VERSION"))
                }
                UpdateCheckState::Checking => "正在检查更新...".to_string(),
                UpdateCheckState::UpToDate(version) => {
                    format!("当前是最新版本 {version}。")
                }
                UpdateCheckState::Available(info) => {
                    format!("发现新版本 {}。", info.version)
                }
                UpdateCheckState::Failed(error) => format!("检查更新失败：{error}"),
            };
            let download = match &update_state {
                UpdateCheckState::Available(info) => Some((
                    info.download_url.clone(),
                    if info.asset_name.is_some() {
                        "下载更新"
                    } else {
                        "查看发布页"
                    },
                )),
                _ => None,
            };
            let check_label = if matches!(update_state, UpdateCheckState::Available(_)) {
                "重新检查"
            } else {
                "检查更新"
            };
            let check_app = update_app.clone();

            v_flex()
                .w_full()
                .items_center()
                .gap_3()
                .py_2()
                .text_center()
                .child(
                    img(Arc::new(Image::from_bytes(
                        ImageFormat::Png,
                        include_bytes!("../../assets/icons/Ucp.png").to_vec(),
                    )))
                    .size(px(64.))
                    .object_fit(ObjectFit::Contain),
                )
                .child(Label::new("UCP Clipboard").text_lg())
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(status),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("check-for-updates")
                                .icon(IconName::Redo)
                                .label(check_label)
                                .outline()
                                .loading(checking)
                                .on_click(move |_, _, cx| {
                                    check_app.update(cx, |this, cx| {
                                        this.start_update_check(cx);
                                    });
                                }),
                        )
                        .when_some(download, |this, (url, label)| {
                            this.child(
                                Button::new("download-update")
                                    .icon(IconName::ExternalLink)
                                    .label(label)
                                    .primary()
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                            )
                        }),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("跨平台桌面剪贴板历史应用"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("基于 Rust、GPUI 和 GPUI Component 构建"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("MIT License"),
                )
                .child(
                    Link::new("ucp-repository")
                        .href("https://github.com/rzh0504/ucp")
                        .text_sm()
                        .child("https://github.com/rzh0504/ucp"),
                )
        });

        div().size_full().bg(cx.theme().colors.list).child(
            Settings::new("clipboard-settings")
                .large()
                .sidebar_width(px(180.))
                .pages(vec![
                    SettingPage::new("常规")
                        .icon(IconName::Settings2)
                        .default_open(true)
                        .groups(vec![
                            SettingGroup::new().title("剪贴板").items(vec![
                                monitor,
                                promote,
                                double_click_copy,
                                show_copy_time,
                                show_text_length,
                            ]),
                            SettingGroup::new().title("外观").items(vec![theme]),
                            SettingGroup::new()
                                .title("系统")
                                .items(vec![startup, quick_paste]),
                        ]),
                    SettingPage::new("快捷键")
                        .icon(IconName::Settings2)
                        .resettable(false)
                        .groups(vec![SettingGroup::new().title("导航").items(vec![
                            move_up,
                            move_down,
                            move_first,
                            move_last,
                            clear_navigation,
                        ])]),
                    SettingPage::new("关于")
                        .icon(IconName::Info)
                        .resettable(false)
                        .groups(vec![SettingGroup::new().items(vec![about])]),
                ]),
        )
    }
}

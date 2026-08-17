use super::ClipboardApp;
use crate::model::AppTheme;
use crate::platform;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
};

impl ClipboardApp {
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
                move |checked, cx| {
                    app.update(cx, |this, cx| {
                        this.settings.quick_paste = checked;
                        this.save_settings();
                        cx.notify();
                    });
                },
            ),
        )
        .description("允许使用快捷键快速打开剪贴板历史。");

        div().size_full().bg(cx.theme().colors.list).child(
            Settings::new("clipboard-settings").large().pages(vec![
                SettingPage::new("常规")
                    .icon(IconName::Settings2)
                    .default_open(true)
                    .groups(vec![
                        SettingGroup::new().title("剪贴板").items(vec![
                            monitor,
                            promote,
                            show_copy_time,
                            show_text_length,
                        ]),
                        SettingGroup::new().title("外观").items(vec![theme]),
                        SettingGroup::new()
                            .title("系统")
                            .items(vec![startup, quick_paste]),
                    ]),
            ]),
        )
    }
}

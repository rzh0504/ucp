use super::ClipboardApp;
use crate::platform;
use gpui::*;
use gpui_component::{
    IconName, Sizable as _,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
};

impl ClipboardApp {
    pub(super) fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let app = cx.entity().clone();
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

        div()
            .size_full()
            .child(Settings::new("clipboard-settings").large().pages(vec![
                SettingPage::new("常规")
                    .icon(IconName::Settings2)
                    .default_open(true)
                    .groups(vec![
                        SettingGroup::new()
                            .title("剪贴板")
                            .items(vec![monitor, promote]),
                        SettingGroup::new()
                            .title("系统")
                            .items(vec![startup, quick_paste]),
                    ]),
            ]))
    }
}

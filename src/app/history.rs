use super::ClipboardApp;
use crate::model::{AppLanguage, ClipboardContent, ClipboardEntry, ClipboardFilter, HistoryCounts};
use crate::services::ClipboardService;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _,
    button::Button,
    h_flex,
    input::Input,
    menu::{ContextMenuExt as _, PopupMenuItem},
    scroll::{ScrollableMask, Scrollbar, ScrollbarMode},
    spinner::Spinner,
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex, v_virtual_list,
};
use gpui_kit::{prelude::FluentBuilder as _, *};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const COLLAPSED_TEXT_LINES: usize = 4;
const TEXT_LINE_HEIGHT: f32 = 24.;
const TEXT_VERTICAL_PADDING: f32 = 16.;
const TEXT_FOOTER_HEIGHT: f32 = 24.;
const TEXT_ROW_CHROME_HEIGHT: f32 = 8.;
const SINGLE_LINE_TEXT_ROW_HEIGHT: f32 = 64.;
const TEXT_WIDTH_UNITS_PER_PREVIEW_LINE: usize = 130;
const MAX_EXPANDED_TEXT_ROW_HEIGHT: f32 = 448.;
const COLLAPSED_IMAGE_ROW_HEIGHT: f32 = 180.;
const COLLAPSED_IMAGE_MAX_WIDTH: f32 = 220.;
const COLLAPSED_IMAGE_MAX_HEIGHT: f32 = 140.;
const EXPANDED_IMAGE_MAX_WIDTH: f32 = 800.;
const EXPANDED_IMAGE_MAX_HEIGHT: f32 = 600.;
const MIN_EXPANDED_IMAGE_AREA_HEIGHT: f32 = 180.;
const IMAGE_ROW_CHROME_HEIGHT: f32 = 40.;
const IMAGE_ROW_HORIZONTAL_SPACE: f32 = 100.;

/// Line estimates above this cap cannot change any row height: the expanded
/// row height saturates at [`MAX_EXPANDED_TEXT_ROW_HEIGHT`], so the estimator
/// stops scanning once the cap is reached.
const TEXT_LINE_ESTIMATE_CAP: usize = ((MAX_EXPANDED_TEXT_ROW_HEIGHT
    - TEXT_VERTICAL_PADDING
    - TEXT_FOOTER_HEIGHT
    - TEXT_ROW_CHROME_HEIGHT)
    / TEXT_LINE_HEIGHT) as usize
    + 1;

/// Hard byte budget for collapsed rendering and fallback line estimation.
/// Truncated prefixes conservatively keep expansion available; neither shaping
/// nor trimming a collapsed row can inspect the unbounded clipboard suffix.
const TEXT_ESTIMATE_SCAN_LIMIT: usize = 8 * 1024;
// Row padding/borders, number, spacer, and the two 8px gaps. Expanded text
// reserves another 12px for its scrollbar.
const TEXT_ROW_HORIZONTAL_SPACE: f32 = 116.;
const TEXT_SCROLLBAR_SPACE: f32 = 12.;

#[derive(Clone, PartialEq)]
struct TextLayoutKey {
    width: Pixels,
    font: Font,
}

#[derive(Clone, PartialEq)]
struct RowLayoutKey {
    text: TextLayoutKey,
    image_width: f32,
    expanded_image: Option<u64>,
    expanded_text: Option<u64>,
}

struct CachedText {
    preview: SharedString,
    truncated: bool,
    layout: Option<(TextLayoutKey, usize)>,
    character_count: Option<usize>,
}

#[derive(Clone)]
struct TextRowPresentation {
    title: SharedString,
    lines: usize,
    truncated: bool,
    character_count: Option<usize>,
}

/// Entry content is immutable by id. Metadata-only entry replacements therefore
/// keep these bounded previews; changing the visible sequence invalidates sizes.
#[derive(Default)]
pub(super) struct HistoryRenderCache {
    available_width: Option<Pixels>,
    texts: HashMap<u64, CachedText>,
    sizes: Option<(RowLayoutKey, Rc<Vec<Size<Pixels>>>)>,
    // Only the explicitly expanded row may retain a full clipboard string.
    expanded: Option<(u64, SharedString)>,
}

impl HistoryRenderCache {
    /// Invalidate visible row sizes, but retain text metadata across searches.
    /// `entries` is the active history, not just the current filter's results.
    pub(super) fn invalidate_rows(&mut self, entries: &[Arc<ClipboardEntry>]) {
        self.sizes = None;
        let ids: HashSet<_> = entries.iter().map(|entry| entry.id).collect();
        self.texts.retain(|id, _| ids.contains(id));
        if self
            .expanded
            .as_ref()
            .is_some_and(|(id, _)| !ids.contains(id))
        {
            self.expanded = None;
        }
    }

    fn text(&mut self, id: u64, text: &str) -> &mut CachedText {
        self.texts.entry(id).or_insert_with(|| {
            let prefix = estimate_scan_prefix(text);
            CachedText {
                // Trim only the bounded prefix, never a potentially huge suffix.
                preview: prefix.trim_end_matches(['\r', '\n']).to_owned().into(),
                truncated: prefix.len() < text.len(),
                layout: None,
                character_count: None,
            }
        })
    }

    fn text_lines(&mut self, id: u64, text: &str, key: &TextLayoutKey, window: &Window) -> usize {
        let cached = self.text(id, text);
        if let Some((old_key, lines)) = &cached.layout
            && old_key == key
        {
            return *lines;
        }
        let run = TextRun {
            len: cached.preview.len(),
            font: key.font.clone(),
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let lines = window
            .text_system()
            .shape_text(
                cached.preview.clone(),
                px(14.),
                &[run],
                Some(key.width),
                Some(TEXT_LINE_ESTIMATE_CAP),
            )
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| line.wrap_boundaries.len() + 1)
                    .sum::<usize>()
                    .clamp(1, TEXT_LINE_ESTIMATE_CAP)
            })
            .unwrap_or_else(|_| ClipboardApp::text_line_estimate(&cached.preview));
        // An omitted suffix may contain more lines even if the prefix ends in
        // whitespace. Keep expansion/scrolling available without scanning it.
        let lines = if cached.truncated {
            TEXT_LINE_ESTIMATE_CAP
        } else {
            lines
        };
        cached.layout = Some((key.clone(), lines));
        lines
    }

    fn presentation(
        &mut self,
        entry: &ClipboardEntry,
        expanded: bool,
        show_length: bool,
    ) -> Option<TextRowPresentation> {
        let ClipboardContent::Text(text) = &entry.content else {
            return None;
        };
        let cached = self.text(entry.id, text);
        let character_count = if show_length {
            // This exact count is computed at most once per cached entry, and
            // never during the full-list row sizing pass.
            Some(
                *cached
                    .character_count
                    .get_or_insert_with(|| text.chars().count()),
            )
        } else {
            None
        };
        let mut presentation = TextRowPresentation {
            title: cached.preview.clone(),
            lines: cached.layout.as_ref().map_or(1, |(_, lines)| *lines),
            truncated: cached.truncated,
            character_count,
        };
        if expanded {
            if self.expanded.as_ref().is_none_or(|(id, _)| *id != entry.id) {
                self.expanded = Some((entry.id, text.clone().into()));
            }
            presentation.title = self.expanded.as_ref().unwrap().1.clone();
        }
        Some(presentation)
    }
}

fn estimate_scan_prefix(text: &str) -> &str {
    if text.len() <= TEXT_ESTIMATE_SCAN_LIMIT {
        return text;
    }
    let mut end = TEXT_ESTIMATE_SCAN_LIMIT;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Fades expanded entry content in. The row height itself must never animate:
/// the virtual list derives it per frame, so only opacity moves.
fn expand_reveal<E>(name: &'static str, id: u64, element: E) -> AnyElement
where
    E: IntoElement + Styled + 'static,
{
    element
        .with_animation(
            ElementId::NamedInteger(name.into(), id),
            Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()),
            |element, delta| element.opacity(delta),
        )
        .into_any_element()
}

impl ClipboardApp {
    pub(super) fn render_history(
        &mut self,
        counts: HistoryCounts,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.settings.language;
        let show_copy_time = self.settings.show_copy_time;
        let show_text_length = self.settings.show_text_length;
        let double_click_copy = self.settings.double_click_copy;
        let quick_paste = self.settings.quick_paste;
        let available_width = self
            .history_render_cache
            .available_width
            .unwrap_or(window.viewport_size().width)
            .as_f32();
        let image_max_width =
            (available_width - IMAGE_ROW_HORIZONTAL_SPACE).clamp(1., EXPANDED_IMAGE_MAX_WIDTH);
        let filters = TabBar::new("filters")
            .segmented()
            .large()
            .selected_index(match self.filter {
                ClipboardFilter::All => 0,
                ClipboardFilter::Text => 1,
                ClipboardFilter::Image => 2,
                ClipboardFilter::File => 3,
                ClipboardFilter::Favorite => 4,
            })
            .on_click(cx.listener(|this, index, _, cx| {
                this.filter = match *index {
                    1 => ClipboardFilter::Text,
                    2 => ClipboardFilter::Image,
                    3 => ClipboardFilter::File,
                    4 => ClipboardFilter::Favorite,
                    _ => ClipboardFilter::All,
                };
                this.refresh_visible_entries(cx);
                this.preload_file_icons(cx);
                cx.notify();
            }))
            .child(Self::filter_tab(IconName::Inbox, "全部", counts.total))
            .child(Self::filter_tab(IconName::ALargeSmall, "文本", counts.text))
            .child(Self::filter_tab(IconName::Frame, "图片", counts.image))
            .child(Self::filter_tab(IconName::File, "文件", counts.file))
            .child(Self::filter_tab(IconName::Heart, "收藏", counts.favorite));
        let layout_key = RowLayoutKey {
            text: TextLayoutKey {
                width: px(
                    (available_width - TEXT_ROW_HORIZONTAL_SPACE - TEXT_SCROLLBAR_SPACE).max(1.),
                ),
                font: window.text_style().font(),
            },
            image_width: image_max_width,
            expanded_image: self.expanded_image_id,
            expanded_text: self.expanded_text_id,
        };
        if !self
            .history_render_cache
            .sizes
            .as_ref()
            .is_some_and(|(key, _)| key == &layout_key)
        {
            let sizes = self
                .visible_entries
                .iter()
                .map(|entry| {
                    let height = match &entry.content {
                        ClipboardContent::Text(text) => {
                            let lines = self.history_render_cache.text_lines(
                                entry.id,
                                text,
                                &layout_key.text,
                                window,
                            );
                            Self::measured_text_row_height(
                                lines,
                                self.expanded_text_id == Some(entry.id),
                            )
                        }
                        _ => Self::entry_height(
                            entry,
                            self.expanded_image_id == Some(entry.id),
                            false,
                            image_max_width,
                        ),
                    };
                    size(px(0.), px(height))
                })
                .collect();
            self.history_render_cache.sizes = Some((layout_key, Rc::new(sizes)));
        }
        if self.expanded_text_id.is_none() {
            self.history_render_cache.expanded = None;
        }
        let item_sizes = self.history_render_cache.sizes.as_ref().unwrap().1.clone();
        let rendered_sizes = item_sizes.clone();
        let list = v_virtual_list(
            cx.entity().clone(),
            "history-list",
            item_sizes,
            move |this, range, _window, cx| {
                range
                    .filter_map(|index| {
                        this.visible_entries.get(index).cloned().map(|entry| {
                            let selected = this.selected_entry_ids.contains(&entry.id);
                            let multiple_selected = this.selected_entry_ids.len() > 1;
                            let selected_ids = this.selected_entry_ids.iter().copied().collect();
                            let all_selected_favorite = multiple_selected
                                && this.selected_entry_ids.iter().all(|selected_id| {
                                    this.history
                                        .entry(*selected_id)
                                        .is_some_and(|selected_entry| selected_entry.favorite)
                                });
                            let image_expanded = this.expanded_image_id == Some(entry.id);
                            let text_expanded = this.expanded_text_id == Some(entry.id);
                            let text = this.history_render_cache.presentation(
                                &entry,
                                text_expanded,
                                show_text_length,
                            );
                            let navigated = this.navigation_entry_id == Some(entry.id)
                                || image_expanded
                                || text_expanded;
                            Self::render_entry(
                                entry.clone(),
                                index + 1,
                                language,
                                selected,
                                multiple_selected,
                                selected_ids,
                                all_selected_favorite,
                                navigated,
                                image_expanded,
                                text_expanded,
                                image_max_width,
                                rendered_sizes[index].height.as_f32(),
                                text,
                                this.expanded_text_scroll.clone(),
                                this.file_icon_paths.get(&entry.id).cloned(),
                                this.missing_file_entries.contains(&entry.id),
                                (
                                    show_copy_time,
                                    show_text_length,
                                    double_click_copy,
                                    quick_paste,
                                ),
                                cx,
                            )
                        })
                    })
                    .collect()
            },
        )
        .track_scroll(&self.history_scroll)
        .size_full();

        let content = if let Some(error) = self.startup_error.as_ref() {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .child("无法加载剪贴板历史")
                .child(div().text_sm().child(error.clone()))
                .child(
                    Button::new("retry-startup")
                        .label("重试")
                        .on_click(cx.listener(|this, _, _, cx| this.retry_startup(cx))),
                )
                .into_any_element()
        } else if self.history_loading || self.search_pending {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(Spinner::new())
                .child(div().text_sm().child(if self.search_pending {
                    "正在搜索..."
                } else {
                    "正在加载剪贴板历史..."
                }))
                .into_any_element()
        } else if self.visible_entries.is_empty() {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(IconName::Inbox)
                .child(div().font_medium().child(if self.query.is_empty() {
                    "暂无剪贴板记录"
                } else {
                    "没有匹配的记录"
                }))
                .child(div().text_sm().child("复制文本、图片或文件后会显示在这里"))
                .into_any_element()
        } else {
            div()
                .relative()
                .size_full()
                .overflow_hidden()
                .child(list)
                .child(Scrollbar::vertical(&self.history_scroll).mode(ScrollbarMode::Scrolling))
                .into_any_element()
        };

        let app = cx.entity().downgrade();
        v_flex()
            .size_full()
            .bg(cx.theme().colors.list)
            // Observe the actual Kit content bounds, including native/CSD
            // insets, instead of assuming content fills the window viewport.
            .on_prepaint(move |bounds, _, cx| {
                app.update(cx, |this, cx| {
                    if this.history_render_cache.available_width != Some(bounds.size.width) {
                        this.history_render_cache.available_width = Some(bounds.size.width);
                        cx.notify();
                    }
                })
                .ok();
            })
            .child(
                h_flex()
                    .px_4()
                    .py_2()
                    .gap_4()
                    .items_center()
                    .bg(cx.theme().background)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(filters)
                    .child(div().flex_1())
                    .child(
                        Input::new(&self.search)
                            .large()
                            .w(px(300.))
                            .prefix(IconName::Search)
                            .cleanable(true),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
    }

    fn filter_tab(icon: IconName, label: &'static str, count: usize) -> Tab {
        Tab::new().child(
            h_flex()
                .gap_1()
                .child(Icon::new(icon).small())
                .child(format!("{label} {count}")),
        )
    }

    pub(super) fn preload_file_icons(&mut self, cx: &mut Context<Self>) {
        const MAX_FILE_ICON_LOADS: usize = 4;
        let active_loads = self.file_icon_loading.len();
        let mut started_loads = 0;
        for entry in &self.visible_entries {
            if active_loads + started_loads >= MAX_FILE_ICON_LOADS {
                break;
            }
            let ClipboardContent::Files(files) = &entry.content else {
                continue;
            };
            let Some(file) = files.first().cloned() else {
                continue;
            };
            if self.file_icon_paths.contains_key(&entry.id)
                || self.file_icon_failed.contains(&entry.id)
                || !self.file_icon_loading.insert(entry.id)
            {
                continue;
            }
            let id = entry.id;
            let files = files.clone();
            started_loads += 1;
            cx.spawn(async move |entity, cx| {
                let (icon_path, missing) = cx
                    .background_spawn(async move {
                        let missing = files.iter().any(|file| {
                            matches!(std::path::Path::new(file).try_exists(), Ok(false))
                        });
                        let icon_path = if missing {
                            None
                        } else {
                            crate::platform::file_icon::icon_path(std::path::Path::new(&file))
                        };
                        (icon_path, missing)
                    })
                    .await;
                let _ = entity.update(cx, |this, cx| {
                    this.file_icon_loading.remove(&id);
                    if missing {
                        this.missing_file_entries.insert(id);
                    } else {
                        this.missing_file_entries.remove(&id);
                    }
                    if let Some(path) = icon_path {
                        this.file_icon_paths.insert(id, path);
                    } else {
                        this.file_icon_failed.insert(id);
                    }
                    this.preload_file_icons(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    pub(super) fn handle_history_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page != super::AppPage::History
            || !self.initial_focus.is_focused(window)
            || self.visible_entries.is_empty()
        {
            return;
        }

        let key = event.keystroke.key.as_str();
        if key == "delete" && event.keystroke.modifiers == Modifiers::none() {
            let mut ids = self.selected_entry_ids.iter().copied().collect::<Vec<_>>();
            if let Some(id) = self.navigation_entry_id
                && !ids.contains(&id)
            {
                ids.push(id);
            }
            if !ids.is_empty() {
                self.delete_entries(ids, cx);
                cx.stop_propagation();
            }
            return;
        }
        if event.keystroke.modifiers != Modifiers::none() {
            return;
        }

        let Some(next_index) = (match key {
            "up" => Some(
                self.navigation_index()
                    .map_or(self.visible_entries.len() - 1, |index| {
                        index.saturating_sub(1)
                    }),
            ),
            "down" => self.navigation_index().map_or(Some(0), |index| {
                Some((index + 1).min(self.visible_entries.len() - 1))
            }),
            "home" => Some(0),
            "end" => Some(self.visible_entries.len() - 1),
            "escape" => {
                self.navigation_entry_id = None;
                cx.stop_propagation();
                cx.notify();
                None
            }
            "enter" => {
                let target = self.navigation_entry_id.or_else(|| {
                    if self.selected_entry_ids.len() == 1 {
                        self.selected_entry_ids.iter().next().copied()
                    } else {
                        None
                    }
                });
                if let Some(id) =
                    target.filter(|id| self.visible_entries.iter().any(|entry| entry.id == *id))
                {
                    self.selected_entry_ids.clear();
                    self.selection_anchor_id = None;
                    self.navigation_entry_id = None;
                    let quick_paste = self.settings.quick_paste;
                    self.copy_entry(id, quick_paste, Some(window.window_handle()), cx);
                }
                cx.stop_propagation();
                cx.notify();
                None
            }
            _ => return,
        }) else {
            return;
        };

        self.navigation_entry_id = Some(self.visible_entries[next_index].id);
        self.history_scroll
            .scroll_to_item(next_index, ScrollStrategy::Center);
        cx.stop_propagation();
        cx.notify();
    }

    fn navigation_index(&self) -> Option<usize> {
        self.navigation_entry_id
            .or(self.selection_anchor_id)
            .and_then(|id| self.visible_entries.iter().position(|entry| entry.id == id))
    }

    #[allow(clippy::too_many_arguments)]
    fn render_entry(
        entry: Arc<ClipboardEntry>,
        position: usize,
        language: AppLanguage,
        selected: bool,
        multiple_selected: bool,
        selected_ids: Vec<u64>,
        all_selected_favorite: bool,
        navigated: bool,
        image_expanded: bool,
        text_expanded: bool,
        image_max_width: f32,
        row_height: f32,
        text: Option<TextRowPresentation>,
        text_scroll: ScrollHandle,
        file_icon_path: Option<std::path::PathBuf>,
        file_missing: bool,
        options: (bool, bool, bool, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (show_copy_time, _show_text_length, double_click_copy, quick_paste) = options;
        let id = entry.id;
        let is_image = matches!(entry.content, ClipboardContent::Image(_));
        let is_file = matches!(entry.content, ClipboardContent::Files(_));
        let title = text.as_ref().map_or_else(
            || SharedString::from(entry.title_with_language(language)),
            |text| text.title.clone(),
        );
        let meta = match &entry.content {
            ClipboardContent::Text(_) => text
                .as_ref()
                .and_then(|text| text.character_count)
                .map(|count| crate::i18n::character_count(language, count)),
            ClipboardContent::Image(image) => Some(format!("{} x {}", image.width, image.height)),
            _ => Some(entry.size_label_with_language(language)),
        };
        let copy_time = crate::i18n::relative_time(language, entry.captured_at);
        let favorite = entry.favorite;
        let app = cx.entity().downgrade();
        let muted_foreground = cx.theme().muted_foreground;
        let image_content = match &entry.content {
            ClipboardContent::Image(image) => {
                let (preview_width, preview_height) = Self::image_display_size(
                    image.width,
                    image.height,
                    image_expanded,
                    image_max_width,
                );
                let placeholder = move || {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(muted_foreground)
                        .child(Icon::new(IconName::Frame))
                        .into_any_element()
                };
                let preview = ClipboardService::image_preview_path(image.preview_url.as_deref())
                    .map(|path| {
                        img(path)
                            .size_full()
                            .object_fit(if image_expanded {
                                ObjectFit::Contain
                            } else {
                                ObjectFit::ScaleDown
                            })
                            .with_loading(placeholder)
                            .with_fallback(placeholder)
                            .into_any_element()
                    })
                    .unwrap_or_else(placeholder);
                let preview_area = div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .p_2()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .w(px(preview_width))
                            .h(px(preview_height))
                            .flex_none()
                            .overflow_hidden()
                            .bg(cx.theme().background)
                            .child(preview),
                    );
                Some(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .overflow_hidden()
                        .child(if image_expanded {
                            expand_reveal("image-reveal", id, preview_area)
                        } else {
                            preview_area.into_any_element()
                        })
                        .child(
                            h_flex()
                                .h(px(24.))
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(muted_foreground)
                                .when(show_copy_time, |this| this.child(copy_time.clone()))
                                .when(favorite, |this| {
                                    this.child(
                                        Tag::warning()
                                            .small()
                                            .rounded_full()
                                            .ml_2()
                                            .h(px(14.))
                                            .py_0()
                                            .text_size(px(10.))
                                            .child("收藏"),
                                    )
                                })
                                .child(div().flex_1())
                                .child(
                                    h_flex()
                                        .id(ElementId::NamedInteger("image-expand".into(), id))
                                        .gap_1()
                                        .px_2()
                                        .py_1()
                                        .rounded_sm()
                                        .cursor_pointer()
                                        .hover(|style| style.bg(cx.theme().secondary_hover))
                                        .child(
                                            Icon::new(if image_expanded {
                                                IconName::ChevronUp
                                            } else {
                                                IconName::ChevronDown
                                            })
                                            .xsmall(),
                                        )
                                        .child(if image_expanded { "收起" } else { "展开" })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.toggle_image_expansion(id, cx);
                                        })),
                                )
                                .child(div().flex_1())
                                .when_some(meta.clone(), |this, meta| this.child(meta))
                                .child(div().w(px(20.)).text_right().child(position.to_string())),
                        ),
                )
            }
            _ => None,
        };
        let text_lines = text.as_ref().map(|text| text.lines);
        let can_expand_text = text
            .as_ref()
            .is_some_and(|text| text.truncated || text.lines > COLLAPSED_TEXT_LINES);
        let collapsed_text_lines =
            text_lines.map_or(0, |lines| lines.clamp(1, COLLAPSED_TEXT_LINES));
        let content = if let ClipboardContent::Files(files) = &entry.content {
            let first_file = files.first().map(String::as_str).unwrap_or("未知文件");
            let file_name = Self::file_name(first_file);

            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .justify_center()
                .gap_0()
                .child(
                    h_flex()
                        .min_w_0()
                        .h(px(30.))
                        .gap_2()
                        .child(if file_missing {
                            Icon::default()
                                .path("icons/file-missing.svg")
                                .small()
                                .text_color(cx.theme().muted_foreground)
                                .into_any_element()
                        } else {
                            file_icon_path
                                .map(|path| {
                                    img(path)
                                        .flex_none()
                                        .size(px(20.))
                                        .object_fit(ObjectFit::Contain)
                                        .into_any_element()
                                })
                                .unwrap_or_else(|| {
                                    Icon::new(IconName::File).small().into_any_element()
                                })
                        })
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_size(px(14.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().muted_foreground)
                                .when(!file_missing, |this| this.text_color(cx.theme().foreground))
                                .truncate()
                                .child(file_name),
                        ),
                )
                .child(
                    h_flex()
                        .h(px(22.))
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .when(show_copy_time, |this| this.child(copy_time.clone()))
                        .when(favorite, |this| {
                            this.child(
                                Tag::warning()
                                    .small()
                                    .rounded_full()
                                    .ml_2()
                                    .h(px(14.))
                                    .py_0()
                                    .text_size(px(10.))
                                    .child("收藏"),
                            )
                        }),
                )
                .into_any_element()
        } else {
            v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .py_2()
                .child(if text_expanded {
                    // Always scroll expanded text, even if platform fallback
                    // glyphs wrap differently from the cached measurement.
                    expand_reveal(
                        "text-reveal",
                        id,
                        div()
                            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
                                div()
                                    .id(ElementId::NamedInteger("text-scroll".into(), id))
                                    .size_full()
                                    .overflow_y_scroll()
                                    .track_scroll(&text_scroll)
                                    .pr_3()
                                    .text_size(px(14.))
                                    .line_height(px(TEXT_LINE_HEIGHT))
                                    .child(title),
                            )
                            .child(
                                ScrollableMask::new(Axis::Vertical, &text_scroll)
                                    .id(ElementId::NamedInteger("text-scroll-mask".into(), id)),
                            )
                            .child(
                                Scrollbar::vertical(&text_scroll).mode(ScrollbarMode::Scrolling),
                            ),
                    )
                } else {
                    div()
                        .min_h_0()
                        .flex_none()
                        .h(px(collapsed_text_lines as f32 * TEXT_LINE_HEIGHT))
                        .overflow_hidden()
                        .text_size(px(14.))
                        .line_height(px(TEXT_LINE_HEIGHT))
                        .line_clamp(COLLAPSED_TEXT_LINES)
                        .text_ellipsis()
                        .child(title)
                        .into_any_element()
                })
                .child(
                    h_flex()
                        .h(px(24.))
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(cx.theme().muted_foreground)
                        .when(can_expand_text, |this| {
                            this.child(
                                h_flex()
                                    .id(ElementId::NamedInteger("text-expand".into(), id))
                                    .flex_none()
                                    .gap_1()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(cx.theme().secondary_hover))
                                    .child(
                                        Icon::new(if text_expanded {
                                            IconName::ChevronUp
                                        } else {
                                            IconName::ChevronDown
                                        })
                                        .xsmall(),
                                    )
                                    .child(if text_expanded { "收起" } else { "展开" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.toggle_text_expansion(id, cx);
                                    })),
                            )
                        })
                        // Expansion comes first and never shrinks. Optional
                        // metadata yields the remaining width on narrow rows.
                        .child(
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .when(show_copy_time, |this| this.child(copy_time.clone()))
                                .when(favorite, |this| {
                                    this.child(
                                        Tag::warning()
                                            .small()
                                            .rounded_full()
                                            .ml_2()
                                            .h(px(14.))
                                            .py_0()
                                            .text_size(px(10.))
                                            .child("收藏"),
                                    )
                                })
                                .child(div().flex_1())
                                .when_some(meta, |this, meta| this.child(meta)),
                        ),
                )
                .into_any_element()
        };

        h_flex()
            .id(ElementId::NamedInteger("entry".into(), id))
            .w_full()
            .h(px(row_height))
            .relative()
            .overflow_hidden()
            .gap_2()
            .pl_4()
            .pr(px(52.))
            .border_2()
            .border_color(cx.theme().border.opacity(0.))
            .rounded_sm()
            .when(selected, |this| {
                this.bg(cx.theme().blue.opacity(0.12))
                    .when(!multiple_selected, |this| {
                        this.border_color(cx.theme().blue.opacity(0.78))
                    })
            })
            .when(navigated && !selected, |this| {
                this.border_color(cx.theme().blue.opacity(0.78))
            })
            .when(!selected && !navigated, |this| {
                this.child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .right_0()
                        .h(px(1.))
                        .bg(cx.theme().border),
                )
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if (double_click_copy || quick_paste) && event.click_count() == 2 {
                    this.selected_entry_ids.clear();
                    this.selection_anchor_id = None;
                    this.navigation_entry_id = None;
                    this.copy_entry(id, quick_paste, Some(window.window_handle()), cx);
                    return;
                }
                this.navigation_entry_id = None;
                let modifiers = event.modifiers();
                if modifiers.shift {
                    let anchor_index = this
                        .selection_anchor_id
                        .and_then(|anchor_id| {
                            this.visible_entries
                                .iter()
                                .position(|entry| entry.id == anchor_id)
                        })
                        .unwrap_or(position - 1);
                    let clicked_index = position - 1;
                    let (start, end) = if anchor_index <= clicked_index {
                        (anchor_index, clicked_index)
                    } else {
                        (clicked_index, anchor_index)
                    };
                    this.selected_entry_ids = this.visible_entries[start..=end]
                        .iter()
                        .map(|entry| entry.id)
                        .collect();
                    this.selection_anchor_id.get_or_insert(id);
                } else if modifiers.secondary() {
                    if !this.selected_entry_ids.remove(&id) {
                        this.selected_entry_ids.insert(id);
                    }
                    this.selection_anchor_id = Some(id);
                } else {
                    let was_only_selected =
                        this.selected_entry_ids.len() == 1 && this.selected_entry_ids.contains(&id);
                    this.selected_entry_ids.clear();
                    this.selection_anchor_id = if was_only_selected { None } else { Some(id) };
                    if !was_only_selected {
                        this.selected_entry_ids.insert(id);
                    }
                }
                cx.notify();
            }))
            .when_some(image_content, |this, image| this.child(image))
            .when(!is_image, |this| {
                this.child(
                    h_flex()
                        .w(px(24.))
                        .flex_none()
                        .justify_end()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(position.to_string()),
                )
                .child(div().w(px(4.)).flex_none())
                .child(content)
                .when(is_file, |this| {
                    this.child(
                        div()
                            .w(px(68.))
                            .flex_none()
                            .text_right()
                            .text_size(px(11.))
                            .text_color(cx.theme().muted_foreground)
                            .child(entry.size_label_with_language(language)),
                    )
                })
            })
            .context_menu(move |menu, _, cx| {
                if let Some(app) = app.upgrade() {
                    app.update(cx, |this, cx| {
                        this.navigation_entry_id = Some(id);
                        cx.notify();
                    });
                }
                let copy_app = app.clone();
                let favorite_app = app.clone();
                let delete_app = app.clone();
                if multiple_selected {
                    let selected_ids_for_favorite = selected_ids.clone();
                    let selected_ids_for_delete = selected_ids.clone();
                    let favorite_label = if all_selected_favorite {
                        "取消收藏"
                    } else {
                        "收藏"
                    };
                    menu.item(
                        PopupMenuItem::new(favorite_label)
                            .icon(if all_selected_favorite {
                                IconName::HeartOff
                            } else {
                                IconName::Heart
                            })
                            .on_click(move |_, _, cx| {
                                if let Some(app) = favorite_app.upgrade() {
                                    app.update(cx, |this, cx| {
                                        this.set_favorite_for_entries(
                                            selected_ids_for_favorite.clone(),
                                            !all_selected_favorite,
                                            cx,
                                        );
                                    });
                                }
                            }),
                    )
                    .item(
                        PopupMenuItem::new("删除").icon(IconName::Delete).on_click(
                            move |_, _, cx| {
                                if let Some(app) = delete_app.upgrade() {
                                    app.update(cx, |this, cx| {
                                        this.delete_entries(selected_ids_for_delete.clone(), cx)
                                    });
                                }
                            },
                        ),
                    )
                } else {
                    let favorite_label = if favorite { "取消收藏" } else { "收藏" };
                    menu.item(PopupMenuItem::new("复制").icon(IconName::Copy).on_click(
                        move |_, _, cx| {
                            if let Some(app) = copy_app.upgrade() {
                                app.update(cx, |this, cx| this.copy_entry(id, false, None, cx));
                            }
                        },
                    ))
                    .item(
                        PopupMenuItem::new(favorite_label)
                            .icon(if favorite {
                                IconName::HeartOff
                            } else {
                                IconName::Heart
                            })
                            .on_click(move |_, _, cx| {
                                if let Some(app) = favorite_app.upgrade() {
                                    app.update(cx, |this, cx| this.toggle_favorite(id, cx));
                                }
                            }),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new("删除").icon(IconName::Delete).on_click(
                            move |_, _, cx| {
                                if let Some(app) = delete_app.upgrade() {
                                    app.update(cx, |this, cx| this.delete_entry(id, cx));
                                }
                            },
                        ),
                    )
                }
            })
            .into_any_element()
    }

    fn entry_height(
        entry: &ClipboardEntry,
        image_expanded: bool,
        text_expanded: bool,
        image_max_width: f32,
    ) -> f32 {
        match &entry.content {
            ClipboardContent::Image(image) => {
                if !image_expanded {
                    return COLLAPSED_IMAGE_ROW_HEIGHT;
                }
                let (_, image_height) =
                    Self::image_display_size(image.width, image.height, true, image_max_width);
                image_height.max(MIN_EXPANDED_IMAGE_AREA_HEIGHT) + IMAGE_ROW_CHROME_HEIGHT
            }
            ClipboardContent::Text(text) => {
                Self::measured_text_row_height(Self::text_line_estimate(text), text_expanded)
            }
            ClipboardContent::Files(_) => 76.,
        }
    }

    fn image_display_size(
        width: usize,
        height: usize,
        expanded: bool,
        image_max_width: f32,
    ) -> (f32, f32) {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let max_width = if expanded {
            image_max_width
        } else {
            image_max_width.min(COLLAPSED_IMAGE_MAX_WIDTH)
        };
        let max_height = if expanded {
            EXPANDED_IMAGE_MAX_HEIGHT
        } else {
            COLLAPSED_IMAGE_MAX_HEIGHT
        };
        let scale = (max_width / width).min(max_height / height).min(1.);

        (width * scale, height * scale)
    }

    fn toggle_image_expansion(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.expanded_image_id == Some(id) {
            self.expanded_image_id = None;
            if let Some(offset) = self.expanded_image_scroll_offset.take() {
                self.history_scroll.set_offset(offset);
            }
            cx.notify();
            return;
        }

        self.expanded_image_scroll_offset = Some(self.history_scroll.offset());
        self.expanded_image_id = Some(id);
        cx.notify();
    }

    fn toggle_text_expansion(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.expanded_text_id == Some(id) {
            self.expanded_text_id = None;
            if let Some(offset) = self.expanded_text_scroll_offset.take() {
                self.history_scroll.set_offset(offset);
            }
            cx.notify();
            return;
        }

        self.expanded_text_scroll_offset = Some(self.history_scroll.offset());
        self.expanded_text_scroll.set_offset(point(px(0.), px(0.)));
        self.expanded_text_id = Some(id);
        cx.notify();
    }

    #[cfg(test)]
    fn expanded_text_overflows(text: &str) -> bool {
        Self::text_row_height(Self::text_line_estimate(text)) > MAX_EXPANDED_TEXT_ROW_HEIGHT
    }

    fn text_row_height(line_count: usize) -> f32 {
        line_count as f32 * TEXT_LINE_HEIGHT
            + TEXT_VERTICAL_PADDING
            + TEXT_FOOTER_HEIGHT
            + TEXT_ROW_CHROME_HEIGHT
    }

    fn measured_text_row_height(lines: usize, expanded: bool) -> f32 {
        if expanded {
            Self::text_row_height(lines).min(MAX_EXPANDED_TEXT_ROW_HEIGHT)
        } else if lines == 1 {
            SINGLE_LINE_TEXT_ROW_HEIGHT
        } else {
            Self::text_row_height(lines.clamp(1, COLLAPSED_TEXT_LINES))
        }
    }

    fn text_line_estimate(text: &str) -> usize {
        let prefix = estimate_scan_prefix(text);
        if prefix.len() < text.len() {
            return TEXT_LINE_ESTIMATE_CAP;
        }
        let text = prefix.trim_end_matches(['\r', '\n']);
        if text.is_empty() {
            return 1;
        }

        let mut lines = 0;
        for line in text.lines() {
            let unit_budget = (TEXT_LINE_ESTIMATE_CAP - lines) * TEXT_WIDTH_UNITS_PER_PREVIEW_LINE;
            let mut units = 0;
            for character in line.chars() {
                units += if character.is_ascii() { 1 } else { 2 };
                if units >= unit_budget {
                    return TEXT_LINE_ESTIMATE_CAP;
                }
            }
            lines += units.div_ceil(TEXT_WIDTH_UNITS_PER_PREVIEW_LINE).max(1);
            if lines >= TEXT_LINE_ESTIMATE_CAP {
                return TEXT_LINE_ESTIMATE_CAP;
            }
        }
        lines
    }

    fn file_name(path: &str) -> String {
        Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(path)
            .to_string()
    }

    pub(super) fn copy_entry(
        &mut self,
        id: u64,
        allow_quick_paste: bool,
        window: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.history.entry(id) else {
            return;
        };
        let Some(session) = self.storage_session.as_ref() else {
            return;
        };
        let storage = session.storage.clone();
        let content = entry.content.clone();
        let should_quick_paste = allow_quick_paste
            && self.settings.quick_paste
            && matches!(&content, ClipboardContent::Text(_));
        let should_promote =
            self.settings.promote_copied_entries && self.history.should_promote(id);
        let language = self.settings.language;
        self.status = "复制中...".into();
        cx.notify();
        cx.spawn(async move |entity, cx| {
            let result = cx
                .background_spawn(
                    async move { ClipboardService::copy_content(&storage, id, content) },
                )
                .await;
            let copied = result.is_ok();
            entity
                .update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            if should_promote && let Some(updated) = this.history.promote(id) {
                                this.update_visible_entry(id, cx);
                                if let Some(session) = this.storage_session.as_ref() {
                                    let reply = session.writer.promote(id, updated.captured_at);
                                    this.persist(reply, |_, (), _| {}, cx);
                                }
                            }
                            if should_quick_paste {
                                if let Some(window) = window
                                    && window
                                        .update(cx, |_, window, cx| {
                                            this.search.update(cx, |search, cx| {
                                                search.set_value("", window, cx);
                                            });
                                            crate::platform::hide_window(window);
                                        })
                                        .is_ok()
                                {
                                    this.query.clear();
                                    this.selected_entry_ids.clear();
                                    this.selection_anchor_id = None;
                                    this.navigation_entry_id = None;
                                    this.expanded_image_id = None;
                                    this.expanded_image_scroll_offset = None;
                                    this.expanded_text_id = None;
                                    this.expanded_text_scroll_offset = None;
                                    this.refresh_visible_entries(cx);
                                    this.history_scroll.set_offset(point(px(0.), px(0.)));
                                    this.history_scroll.scroll_to_item(0, ScrollStrategy::Top);
                                    this.preload_file_icons(cx);
                                }
                                this.status = "正在切换窗口并粘贴...".into();
                            } else {
                                this.status = "已复制".into();
                            }
                        }
                        Err(error) => {
                            let message = error.to_localized_string(language);
                            this.status = message.clone();
                            this.show_error("复制失败", message, cx);
                        }
                    }
                    cx.notify();
                })
                .ok();

            if copied && should_quick_paste {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(260))
                    .await;
                let result = cx
                    .background_spawn(async { crate::platform::clipboard::paste_shortcut() })
                    .await;
                entity
                    .update(cx, |this, cx| {
                        this.status = match result {
                            Ok(()) => "已快捷粘贴".into(),
                            Err(error) => {
                                let message = error.to_string();
                                this.show_error("快捷粘贴失败", message.clone(), cx);
                                message
                            }
                        };
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    }

    fn toggle_favorite(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(session) = self.storage_session.as_ref() else {
            return;
        };
        let writer = session.writer.clone();
        if let Some(updated) = self.history.toggle_favorite(id) {
            self.update_visible_entry(id, cx);
            let reply = writer.set_favorites(vec![(id, updated.favorite)]);
            self.persist(reply, |_, (), _| {}, cx);
        }
        cx.notify();
    }

    fn set_favorite_for_entries(&mut self, ids: Vec<u64>, favorite: bool, cx: &mut Context<Self>) {
        let Some(session) = self.storage_session.as_ref() else {
            return;
        };
        let writer = session.writer.clone();
        let mut updated_entries = Vec::new();
        for id in ids {
            let already_favorite = self.history.entry(id).is_some_and(|entry| entry.favorite);
            if already_favorite != favorite
                && let Some(updated) = self.history.toggle_favorite(id)
            {
                self.update_visible_entry(id, cx);
                updated_entries.push((updated.id, updated.favorite));
            }
        }

        if !updated_entries.is_empty() {
            let reply = writer.set_favorites(updated_entries);
            self.persist(reply, |_, (), _| {}, cx);
        }
        cx.notify();
    }

    fn delete_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        self.delete_entries(vec![id], cx);
    }

    fn delete_entries(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        let Some(session) = self.storage_session.as_ref() else {
            return;
        };
        let writer = session.writer.clone();
        let ids = ids
            .into_iter()
            .filter(|id| self.history.entry(*id).is_some() && !self.pending_delete_ids.contains(id))
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return;
        }

        // Do not tombstone on GPUI: that would cancel saves already ahead of
        // this deletion in the FIFO. Exclude IDs only from capture deduplication.
        self.pending_delete_ids.extend(ids.iter().copied());
        self.status = "删除中...".into();
        cx.notify();
        let reply = writer.delete(ids.clone());
        cx.spawn(async move |entity, cx| {
            let result = reply
                .recv()
                .await
                .unwrap_or(Err(crate::storage::StorageError::WorkerUnavailable));
            entity
                .update(cx, |this, cx| {
                    for id in &ids {
                        this.pending_delete_ids.remove(id);
                    }
                    match result {
                        Ok(()) => {
                            this.history.remove_entries(&ids);
                            for id in &ids {
                                this.selected_entry_ids.remove(id);
                            }
                            this.remove_visible_entries(&ids, cx);
                            if this.navigation_entry_id.is_some_and(|id| ids.contains(&id)) {
                                this.navigation_entry_id = None;
                            }
                            if this.selection_anchor_id.is_some_and(|id| ids.contains(&id)) {
                                this.selection_anchor_id = None;
                            }
                            if this.expanded_image_id.is_some_and(|id| ids.contains(&id)) {
                                this.expanded_image_id = None;
                                this.expanded_image_scroll_offset = None;
                            }
                            if this.expanded_text_id.is_some_and(|id| ids.contains(&id)) {
                                this.expanded_text_id = None;
                                this.expanded_text_scroll_offset = None;
                            }
                            this.status = format!("已删除 {} 条记录", ids.len());
                        }
                        Err(error) => {
                            if let Some(session) = this.storage_session.as_ref() {
                                ClipboardService::allow_entry_saves(&session.storage, &ids);
                            }
                            let message = error.to_string();
                            this.report_persistence_error(message, cx);
                        }
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    pub(super) fn clear_current_filter(&mut self, cx: &mut Context<Self>) {
        // Clear exactly the records present at confirmation time. Both storage
        // and memory remove the same IDs; later captures must not be wiped out.
        let ids = self.history.deletable_ids_for_filter(self.filter, false);
        self.delete_entries(ids, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ClipboardApp, HistoryRenderCache, MAX_EXPANDED_TEXT_ROW_HEIGHT, RowLayoutKey,
        TEXT_ESTIMATE_SCAN_LIMIT, TEXT_LINE_ESTIMATE_CAP, TextLayoutKey,
    };
    use crate::model::{ClipboardContent, ClipboardEntry};
    use gpui_kit::{Font, TestAppContext, px, size};
    use std::rc::Rc;
    use std::sync::Arc;

    #[test]
    fn english_command_that_fits_one_line_stays_compact() {
        assert_eq!(
            ClipboardApp::text_line_estimate(
                "watchxec --restart --watch src --watch assets --watch Cargo.toml --exts rs,toml,png,svg,ico -- cargo run"
            ),
            1
        );
    }

    #[test]
    fn wide_characters_reserve_more_lines_than_ascii() {
        assert_eq!(ClipboardApp::text_line_estimate(&"中".repeat(65)), 1);
        assert_eq!(ClipboardApp::text_line_estimate(&"中".repeat(66)), 2);
    }

    #[test]
    fn explicit_lines_are_estimated_independently() {
        assert_eq!(ClipboardApp::text_line_estimate("short\nshort\nshort"), 3);
    }

    #[test]
    fn expanded_text_height_tracks_its_content() {
        let entry = ClipboardEntry::new(1, ClipboardContent::Text("1\n2\n3\n4\n5\n6".to_string()));

        assert_eq!(ClipboardApp::entry_height(&entry, false, true, 800.), 192.);
        assert!(!ClipboardApp::expanded_text_overflows("1\n2\n3\n4\n5\n6"));
    }

    #[test]
    fn expanded_text_height_is_capped_and_scrollable() {
        let text = (1..=17)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let entry = ClipboardEntry::new(1, ClipboardContent::Text(text.clone()));

        assert_eq!(
            ClipboardApp::entry_height(&entry, false, true, 800.),
            MAX_EXPANDED_TEXT_ROW_HEIGHT
        );
        assert!(ClipboardApp::expanded_text_overflows(&text));
    }

    #[test]
    fn huge_texts_are_estimated_with_a_bounded_scan() {
        let single_line = "a".repeat(4 * 1024 * 1024);
        assert_eq!(
            ClipboardApp::text_line_estimate(&single_line),
            TEXT_LINE_ESTIMATE_CAP
        );
        let entry = ClipboardEntry::new(1, ClipboardContent::Text(single_line.clone()));
        assert_eq!(
            ClipboardApp::entry_height(&entry, false, true, 800.),
            MAX_EXPANDED_TEXT_ROW_HEIGHT
        );
        assert!(ClipboardApp::expanded_text_overflows(&single_line));

        let many_lines = "行\n".repeat(500_000);
        assert_eq!(
            ClipboardApp::text_line_estimate(&many_lines),
            TEXT_LINE_ESTIMATE_CAP
        );
    }

    #[test]
    fn huge_trailing_newline_suffix_is_not_trimmed_or_copied() {
        let text = format!("short{}", "\r\n".repeat(2 * 1024 * 1024));
        assert_eq!(
            ClipboardApp::text_line_estimate(&text),
            TEXT_LINE_ESTIMATE_CAP
        );
        let mut cache = HistoryRenderCache::default();
        let cached = cache.text(1, &text);
        assert!(cached.truncated);
        assert_eq!(cached.preview.as_ref(), "short");
        assert!(cached.preview.len() <= TEXT_ESTIMATE_SCAN_LIMIT);
    }

    #[test]
    fn bounded_previews_preserve_utf8_and_reuse_immutable_entry_content() {
        let entry = ClipboardEntry::new(
            7,
            ClipboardContent::Text(format!("a{}", "中🙂".repeat(300_000))),
        );
        let mut cache = HistoryRenderCache::default();
        let first = cache.presentation(&entry, false, false).unwrap();
        assert!(first.truncated);
        assert!(first.title.len() <= TEXT_ESTIMATE_SCAN_LIMIT);
        assert!(first.title.is_char_boundary(first.title.len()));
        assert!(cache.expanded.is_none());

        let mut metadata_update = entry.clone();
        metadata_update.favorite = true;
        let second = cache.presentation(&metadata_update, false, false).unwrap();
        assert_eq!(first.title.as_ptr(), second.title.as_ptr());
        assert!(cache.texts[&entry.id].character_count.is_none());
    }

    #[test]
    fn character_count_and_expanded_text_are_shared_until_invalidation() {
        let entry = Arc::new(ClipboardEntry::new(
            7,
            ClipboardContent::Text("hello 中🙂".repeat(10_000)),
        ));
        let mut cache = HistoryRenderCache::default();
        let first = cache.presentation(&entry, true, true).unwrap();
        let second = cache.presentation(&entry, true, true).unwrap();
        assert_eq!(first.character_count, Some(80_000));
        assert_eq!(first.title.as_ptr(), second.title.as_ptr());
        assert_eq!(
            first.title.len(),
            match &entry.content {
                ClipboardContent::Text(text) => text.len(),
                _ => unreachable!(),
            }
        );

        cache.invalidate_rows(std::slice::from_ref(&entry));
        assert_eq!(cache.texts.len(), 1);
        assert!(cache.expanded.is_some());
        cache.invalidate_rows(&[]);
        assert!(cache.texts.is_empty());
        assert!(cache.expanded.is_none());
    }

    #[test]
    fn row_sizes_are_invalidated_and_pruned_with_visible_entries() {
        let key = RowLayoutKey {
            text: TextLayoutKey {
                width: px(300.),
                font: Font::default(),
            },
            image_width: 800.,
            expanded_image: None,
            expanded_text: None,
        };
        let mut cache = HistoryRenderCache::default();
        cache.text(1, "first");
        cache.text(2, "second");
        cache.sizes = Some((key, Rc::new(vec![size(px(0.), px(64.)); 2])));
        let retained = Arc::new(ClipboardEntry::new(
            2,
            ClipboardContent::Text("second".into()),
        ));
        cache.invalidate_rows(&[retained]);
        assert!(cache.sizes.is_none());
        assert!(!cache.texts.contains_key(&1));
        assert!(cache.texts.contains_key(&2));
    }

    #[gpui_kit::test]
    async fn cached_layout_remeasures_when_width_or_font_changes(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _| {
            let mut cache = HistoryRenderCache::default();
            let text = "one two three four five six seven eight nine ten";
            let wide = TextLayoutKey {
                width: px(1_000.),
                font: Font::default(),
            };
            assert_eq!(cache.text_lines(1, text, &wide, window), 1);
            assert_eq!(cache.text_lines(1, text, &wide, window), 1);

            let narrow = TextLayoutKey {
                width: px(35.),
                ..wide.clone()
            };
            let lines = cache.text_lines(1, text, &narrow, window);
            assert!(lines > super::COLLAPSED_TEXT_LINES);
            assert_eq!(cache.texts[&1].layout.as_ref().unwrap().0.width, px(35.));
            assert_eq!(
                ClipboardApp::measured_text_row_height(lines, false),
                ClipboardApp::text_row_height(super::COLLAPSED_TEXT_LINES),
            );
            let mut changed_font = narrow.clone();
            changed_font.font.weight = gpui_kit::FontWeight::BOLD;
            cache.text_lines(1, text, &changed_font, window);
            assert!(cache.texts[&1].layout.as_ref().unwrap().0 == changed_font);
            assert_eq!(cache.text_lines(1, text, &wide, window), 1);
        });
    }

    #[test]
    fn tall_image_preview_preserves_aspect_ratio() {
        let collapsed = ClipboardApp::image_display_size(342, 723, false, 800.);
        let expanded = ClipboardApp::image_display_size(342, 723, true, 800.);

        assert!((collapsed.0 / collapsed.1 - 342. / 723.).abs() < f32::EPSILON);
        assert_eq!(collapsed.1, 140.);
        assert!((expanded.0 / expanded.1 - 342. / 723.).abs() < f32::EPSILON);
        assert_eq!(expanded.1, 600.);
    }

    #[test]
    fn image_preview_never_upscales_or_exceeds_available_width() {
        assert_eq!(
            ClipboardApp::image_display_size(50, 100, true, 800.),
            (50., 100.)
        );

        let narrow_window = ClipboardApp::image_display_size(1600, 900, true, 400.);
        assert_eq!(narrow_window, (400., 225.));
    }
}

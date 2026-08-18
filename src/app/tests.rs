use crate::model::{ClipboardContent, ClipboardEntry, ClipboardFilter, ClipboardHistory};
use gpui::{AppContext as _, TestAppContext};
use std::rc::Rc;

struct HistoryView {
    history: ClipboardHistory,
    query: String,
    filter: ClipboardFilter,
    visible_entries: Vec<Rc<ClipboardEntry>>,
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

#[gpui::test]
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

#[gpui::test]
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

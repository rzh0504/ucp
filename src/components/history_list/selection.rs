use dioxus::prelude::*;

pub(super) fn focused_entry_id(entry_ids: &[u64], focused_id: Option<u64>) -> Option<u64> {
    focused_id
        .filter(|id| entry_ids.contains(id))
        .or_else(|| entry_ids.first().copied())
}

fn focused_index(entry_ids: &[u64], focused_id: Option<u64>) -> Option<usize> {
    focused_id
        .and_then(|id| entry_ids.iter().position(|entry_id| *entry_id == id))
        .or(if entry_ids.is_empty() { None } else { Some(0) })
}

pub(super) fn move_focus(
    entry_ids: &[u64],
    focused_id: &mut Signal<Option<u64>>,
    selected_ids: &mut Signal<Vec<u64>>,
    selection_anchor_id: &mut Signal<Option<u64>>,
    offset: isize,
    shift: bool,
    preserve_selection: bool,
) {
    let Some(index) = focused_index(entry_ids, *focused_id.read()) else {
        return;
    };
    let next_index = index
        .saturating_add_signed(offset)
        .min(entry_ids.len().saturating_sub(1));

    focus_index(
        entry_ids,
        focused_id,
        selected_ids,
        selection_anchor_id,
        next_index,
        shift,
        preserve_selection,
    );
}

pub(super) fn focus_index(
    entry_ids: &[u64],
    focused_id: &mut Signal<Option<u64>>,
    selected_ids: &mut Signal<Vec<u64>>,
    selection_anchor_id: &mut Signal<Option<u64>>,
    index: usize,
    shift: bool,
    preserve_selection: bool,
) {
    let Some(id) = entry_ids.get(index).copied() else {
        return;
    };

    let mut selection = selected_ids.read().clone();
    let mut anchor = (*selection_anchor_id.read()).or(*focused_id.read());

    if shift {
        update_selection(
            entry_ids,
            &mut selection,
            &mut anchor,
            id,
            preserve_selection,
            true,
        );
        selected_ids.set(selection);
        selection_anchor_id.set(anchor);
    } else if !preserve_selection {
        selected_ids.set(vec![id]);
        selection_anchor_id.set(Some(id));
    }

    focused_id.set(Some(id));
}

pub(super) fn update_selection(
    entry_ids: &[u64],
    selected_ids: &mut Vec<u64>,
    anchor_id: &mut Option<u64>,
    id: u64,
    ctrl: bool,
    shift: bool,
) {
    if shift {
        let Some(anchor) = *anchor_id else {
            selected_ids.clear();
            selected_ids.push(id);
            *anchor_id = Some(id);
            return;
        };

        let Some(anchor_index) = entry_ids.iter().position(|entry_id| *entry_id == anchor) else {
            selected_ids.clear();
            selected_ids.push(id);
            *anchor_id = Some(id);
            return;
        };

        let Some(target_index) = entry_ids.iter().position(|entry_id| *entry_id == id) else {
            return;
        };

        let (start, end) = if anchor_index <= target_index {
            (anchor_index, target_index)
        } else {
            (target_index, anchor_index)
        };
        let range_ids = &entry_ids[start..=end];

        if ctrl {
            for range_id in range_ids {
                if !selected_ids.contains(range_id) {
                    selected_ids.push(*range_id);
                }
            }
        } else {
            selected_ids.clear();
            selected_ids.extend_from_slice(range_ids);
        }

        return;
    }

    if let Some(index) = selected_ids
        .iter()
        .position(|selected_id| *selected_id == id)
    {
        selected_ids.remove(index);
        *anchor_id = selected_ids.last().copied();
        return;
    }

    if ctrl {
        selected_ids.push(id);
    } else {
        selected_ids.clear();
        selected_ids.push(id);
    }

    *anchor_id = Some(id);
}

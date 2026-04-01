use crate::core::inventory::items::{EMPTY_ITEM_ID, ItemId, ItemRegistry};
use bevy::prelude::Resource;

pub const CREATIVE_PANEL_COLUMNS: usize = 6;
pub const CREATIVE_PANEL_ROWS: usize = 8;
pub const CREATIVE_PANEL_PAGE_SIZE: usize = CREATIVE_PANEL_COLUMNS * CREATIVE_PANEL_ROWS;

#[derive(Resource, Clone, Debug, Default)]
pub struct CreativePanelState {
    pub page: usize,
    pub item_ids: Vec<ItemId>,
}

impl CreativePanelState {
    #[inline]
    pub fn item_count(&self) -> usize {
        self.item_ids.len()
    }

    #[inline]
    pub fn page_count(&self) -> usize {
        creative_page_count(self.item_count())
    }

    #[inline]
    pub fn page_label(&self) -> String {
        format!("{}/{}", self.page.saturating_add(1), self.page_count())
    }

    pub fn rebuild_from_registry(&mut self, item_registry: &ItemRegistry) {
        self.item_ids = collect_creative_panel_item_ids(item_registry);
        self.clamp_page();
    }

    #[inline]
    pub fn item_at_page_slot(&self, slot_index: usize) -> Option<ItemId> {
        page_item_index(self.page, slot_index).and_then(|index| self.item_ids.get(index).copied())
    }

    pub fn next_page(&mut self) -> bool {
        let pages = self.page_count();
        if self.page + 1 >= pages {
            return false;
        }
        self.page += 1;
        true
    }

    pub fn prev_page(&mut self) -> bool {
        if self.page == 0 {
            return false;
        }
        self.page -= 1;
        true
    }

    #[inline]
    pub fn clamp_page(&mut self) {
        let last_page = self.page_count().saturating_sub(1);
        if self.page > last_page {
            self.page = last_page;
        }
    }
}

pub fn creative_page_count(item_count: usize) -> usize {
    if item_count == 0 {
        return 1;
    }
    item_count.div_ceil(CREATIVE_PANEL_PAGE_SIZE)
}

#[inline]
pub fn page_item_index(page: usize, slot_index: usize) -> Option<usize> {
    if slot_index >= CREATIVE_PANEL_PAGE_SIZE {
        return None;
    }
    page.checked_mul(CREATIVE_PANEL_PAGE_SIZE)
        .and_then(|offset| offset.checked_add(slot_index))
}

pub fn collect_creative_panel_item_ids(item_registry: &ItemRegistry) -> Vec<ItemId> {
    let mut ids = Vec::with_capacity(item_registry.defs.len().saturating_sub(1));
    for raw in 1..item_registry.defs.len() {
        let Ok(item_id) = ItemId::try_from(raw) else {
            break;
        };
        if item_id == EMPTY_ITEM_ID || item_registry.def_opt(item_id).is_none() {
            continue;
        }
        ids.push(item_id);
    }

    ids.sort_by(|left, right| {
        let left_def = item_registry.def(*left);
        let right_def = item_registry.def(*right);
        left_def
            .category
            .cmp(&right_def.category)
            .then_with(|| left_def.name.cmp(&right_def.name))
            .then_with(|| left_def.key.cmp(&right_def.key))
    });
    ids
}

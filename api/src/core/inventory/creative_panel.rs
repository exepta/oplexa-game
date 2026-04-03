use crate::core::inventory::items::{EMPTY_ITEM_ID, ItemId, ItemRegistry};
use bevy::prelude::Resource;

pub const CREATIVE_PANEL_COLUMNS: usize = 6;
pub const CREATIVE_PANEL_ROWS: usize = 8;
pub const CREATIVE_PANEL_PAGE_SIZE: usize = CREATIVE_PANEL_COLUMNS * CREATIVE_PANEL_ROWS;

/// Represents creative panel state used by the `core::inventory::creative_panel` module.
#[derive(Resource, Clone, Debug, Default)]
pub struct CreativePanelState {
    pub page: usize,
    pub item_ids: Vec<ItemId>,
}

impl CreativePanelState {
    /// Runs the `item_count` routine for item count in the `core::inventory::creative_panel` module.
    #[inline]
    pub fn item_count(&self) -> usize {
        self.item_ids.len()
    }

    /// Runs the `page_count` routine for page count in the `core::inventory::creative_panel` module.
    #[inline]
    pub fn page_count(&self) -> usize {
        creative_page_count(self.item_count())
    }

    /// Runs the `page_label` routine for page label in the `core::inventory::creative_panel` module.
    #[inline]
    pub fn page_label(&self) -> String {
        format!("{}/{}", self.page.saturating_add(1), self.page_count())
    }

    /// Runs the `rebuild_from_registry` routine for rebuild from registry in the `core::inventory::creative_panel` module.
    pub fn rebuild_from_registry(&mut self, item_registry: &ItemRegistry) {
        self.item_ids = collect_creative_panel_item_ids(item_registry);
        self.clamp_page();
    }

    /// Runs the `item_at_page_slot` routine for item at page slot in the `core::inventory::creative_panel` module.
    #[inline]
    pub fn item_at_page_slot(&self, slot_index: usize) -> Option<ItemId> {
        page_item_index(self.page, slot_index).and_then(|index| self.item_ids.get(index).copied())
    }

    /// Runs the `next_page` routine for next page in the `core::inventory::creative_panel` module.
    pub fn next_page(&mut self) -> bool {
        let pages = self.page_count();
        if self.page + 1 >= pages {
            return false;
        }
        self.page += 1;
        true
    }

    /// Runs the `prev_page` routine for prev page in the `core::inventory::creative_panel` module.
    pub fn prev_page(&mut self) -> bool {
        if self.page == 0 {
            return false;
        }
        self.page -= 1;
        true
    }

    /// Runs the `clamp_page` routine for clamp page in the `core::inventory::creative_panel` module.
    #[inline]
    pub fn clamp_page(&mut self) {
        let last_page = self.page_count().saturating_sub(1);
        if self.page > last_page {
            self.page = last_page;
        }
    }
}

/// Runs the `creative_page_count` routine for creative page count in the `core::inventory::creative_panel` module.
pub fn creative_page_count(item_count: usize) -> usize {
    if item_count == 0 {
        return 1;
    }
    item_count.div_ceil(CREATIVE_PANEL_PAGE_SIZE)
}

/// Runs the `page_item_index` routine for page item index in the `core::inventory::creative_panel` module.
#[inline]
pub fn page_item_index(page: usize, slot_index: usize) -> Option<usize> {
    if slot_index >= CREATIVE_PANEL_PAGE_SIZE {
        return None;
    }
    page.checked_mul(CREATIVE_PANEL_PAGE_SIZE)
        .and_then(|offset| offset.checked_add(slot_index))
}

/// Runs the `collect_creative_panel_item_ids` routine for collect creative panel item ids in the `core::inventory::creative_panel` module.
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
            .then_with(|| left_def.localized_name.cmp(&right_def.localized_name))
    });
    ids
}

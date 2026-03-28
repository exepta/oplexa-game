use bevy::prelude::Resource;

pub const HOTBAR_SLOTS: usize = 6;

#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct UiInteractionState {
    pub inventory_open: bool,
}

#[derive(Resource, Debug, Clone, Copy)]
pub struct HotbarSelectionState {
    pub selected_index: usize,
}

impl Default for HotbarSelectionState {
    fn default() -> Self {
        Self { selected_index: 0 }
    }
}

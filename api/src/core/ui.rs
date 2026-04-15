use bevy::prelude::Resource;

pub const HOTBAR_SLOTS: usize = 6;

/// Represents ui interaction state used by the `core::ui` module.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct UiInteractionState {
    pub inventory_open: bool,
    pub menu_open: bool,
    pub chat_open: bool,
    pub structure_menu_open: bool,
    pub workbench_menu_open: bool,
    pub benchmark_input_lock: bool,
}

impl UiInteractionState {
    /// Runs the `blocks_game_input` routine for blocks game input in the `core::ui` module.
    #[inline]
    pub fn blocks_game_input(&self) -> bool {
        self.inventory_open
            || self.menu_open
            || self.chat_open
            || self.structure_menu_open
            || self.workbench_menu_open
            || self.benchmark_input_lock
    }
}

/// Represents hotbar selection state used by the `core::ui` module.
#[derive(Resource, Debug, Clone, Copy)]
pub struct HotbarSelectionState {
    pub selected_index: usize,
}

impl Default for HotbarSelectionState {
    /// Runs the `default` routine for default in the `core::ui` module.
    fn default() -> Self {
        Self { selected_index: 0 }
    }
}

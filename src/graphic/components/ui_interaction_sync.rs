/// Returns whether a UI node should be treated as visible for interaction state.
#[inline]
fn is_ui_open_visibility(visibility: Visibility) -> bool {
    !matches!(visibility, Visibility::Hidden)
}

/// Keeps pause/inventory state, visibility and cursor mode consistent while in-game.
///
/// This prevents stale UI flags from blocking gameplay input after rapid toggles
/// or state transitions.
fn sync_ingame_ui_interaction_state(
    mut ui_interaction: ResMut<UiInteractionState>,
    mut pause_menu: ResMut<PauseMenuState>,
    mut inventory_ui: ResMut<PlayerInventoryUiState>,
    mut structure_menu: ResMut<StructureBuildMenuState>,
    mut workbench_menu: ResMut<WorkbenchRecipeMenuState>,
    mut roots: ParamSet<(
        Query<&mut Visibility, With<PauseMenuRoot>>,
        Query<&mut Visibility, With<PlayerInventoryRoot>>,
        Query<&mut Visibility, With<StructureBuildRoot>>,
        Query<&mut Visibility, With<WorkbenchRecipeRoot>>,
    )>,
    mut cursor_q: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let pause_visible = roots
        .p0()
        .single_mut()
        .map(|visibility| is_ui_open_visibility(*visibility))
        .unwrap_or(false);
    let structure_visible = roots
        .p2()
        .single_mut()
        .map(|visibility| is_ui_open_visibility(*visibility))
        .unwrap_or(false);
    let workbench_visible = roots
        .p3()
        .single_mut()
        .map(|visibility| is_ui_open_visibility(*visibility))
        .unwrap_or(false);

    let resolved_pause_open = pause_menu.open || pause_visible;
    let resolved_inventory_open = inventory_ui.open;
    let resolved_structure_open = structure_menu.open || structure_visible;
    let resolved_workbench_open = workbench_menu.open || workbench_visible;

    pause_menu.open = resolved_pause_open;
    inventory_ui.open = resolved_inventory_open;
    structure_menu.open = resolved_structure_open;
    workbench_menu.open = resolved_workbench_open;
    ui_interaction.menu_open = resolved_pause_open;
    ui_interaction.inventory_open = resolved_inventory_open;
    ui_interaction.structure_menu_open = resolved_structure_open;
    ui_interaction.workbench_menu_open = resolved_workbench_open;

    if let Ok(mut visibility) = roots.p0().single_mut() {
        *visibility = if resolved_pause_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visibility) = roots.p1().single_mut() {
        *visibility = if resolved_inventory_open || resolved_workbench_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visibility) = roots.p2().single_mut() {
        *visibility = if resolved_structure_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Ok(mut visibility) = roots.p3().single_mut() {
        *visibility = if resolved_workbench_open {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if let Ok(mut cursor) = cursor_q.single_mut() {
        if ui_interaction.blocks_game_input() {
            cursor.grab_mode = CursorGrabMode::None;
            cursor.visible = true;
        } else {
            cursor.grab_mode = CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
}

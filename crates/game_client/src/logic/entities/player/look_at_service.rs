use crate::core::config::GlobalConfig;
use crate::core::entities::player::block_selection::{SelectionState, StructureHit};
use crate::core::entities::player::inventory::{InventorySlot, PlayerInventory};
use crate::core::entities::player::PlayerCamera;
use crate::core::entities::player::{FpsController, GameMode, GameModeState, Player};
use crate::core::inventory::items::ItemRegistry;
use crate::core::inventory::recipe::{
    ActiveStructurePlacementState, ActiveStructureRecipeState, BuildingStructureRecipe,
    BuildingStructureRecipeRegistry,
};
use crate::core::states::states::{AppState, InGameStates};
use crate::core::ui::{HotbarSelectionState, UiInteractionState, HOTBAR_SLOTS};
use crate::core::world::block::{
    get_block_world, get_stacked_block_world, BlockId, BlockRegistry, Face, SelectedBlock,
    VOXEL_SIZE,
};
use crate::core::world::chunk::{ChunkMap, VoxelStage};
use crate::core::world::chunk_dimension::{world_to_chunk_xz, CX, CY, CZ, Y_MIN};
use crate::core::world::ray_cast_voxels;
use crate::logic::events::block_event_handler::{
    normalize_rotation_quarters, normalize_rotation_steps, resolve_placement_for_selected,
    rotated_structure_offset, rotation_steps_to_placement_quarters, structure_model_translation,
    PlacedStructureMetadata,
};
use bevy::camera::visibility::RenderLayers;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use bevy_rapier3d::prelude::{QueryFilter, ReadRapierContext};

/// Represents look at service used by the `logic::entities::player::look_at_service` module.
pub struct LookAtService;

/// Represents selection outline root used by the `logic::entities::player::look_at_service` module.
#[derive(Component)]
struct SelectionOutlineRoot;

/// Represents placement preview root used by the `logic::entities::player::look_at_service` module.
#[derive(Component)]
struct PlacementPreviewRoot;

/// Represents structure placement preview root used by the `logic::entities::player::look_at_service` module.
#[derive(Component)]
struct StructurePlacementPreviewRoot;

/// Runtime state for placement preview material updates.
#[derive(Component, Default)]
struct PlacementPreviewState {
    block_id: BlockId,
}

/// Runtime state for structure placement preview material updates.
#[derive(Component, Default)]
struct StructurePlacementPreviewState {
    can_place: bool,
    recipe_name: Option<String>,
    scene_entity: Option<Entity>,
}

#[derive(Component)]
struct StructurePlacementPreviewSceneRoot;

#[derive(Component)]
struct StructurePreviewOwnedMaterial;

impl Plugin for LookAtService {
    /// Builds this component for the `logic::entities::player::look_at_service` module.
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_selection_outline);
        app.add_systems(Startup, spawn_placement_preview);
        app.add_systems(Startup, spawn_structure_placement_preview);

        app.configure_sets(
            Update,
            (
                VoxelStage::Input,
                VoxelStage::WorldEdit,
                VoxelStage::Meshing,
            )
                .chain(),
        );

        app.add_systems(
            Update,
            (
                update_selection.in_set(VoxelStage::Input),
                sync_selection_outline.in_set(VoxelStage::Input),
                sync_placement_preview.in_set(VoxelStage::Input),
                sync_structure_placement_preview.in_set(VoxelStage::Input),
                pick_block_from_look,
            )
                .chain()
                .run_if(in_state(AppState::InGame(InGameStates::Game))),
        );
    }
}

/// Spawns placement preview for the `logic::entities::player::look_at_service` module.
fn spawn_placement_preview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q_existing: Query<Entity, With<PlacementPreviewRoot>>,
) {
    if !q_existing.is_empty() {
        return;
    }

    let mesh = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.75, 0.92, 1.0, 0.25),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        PlacementPreviewRoot,
        PlacementPreviewState::default(),
        Name::new("PlacementPreview"),
        Mesh3d(mesh),
        MeshMaterial3d(material),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Hidden,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        RenderLayers::layer(0),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Spawns structure placement preview for the `logic::entities::player::look_at_service` module.
fn spawn_structure_placement_preview(
    mut commands: Commands,
    q_existing: Query<Entity, With<StructurePlacementPreviewRoot>>,
) {
    if !q_existing.is_empty() {
        return;
    }

    commands.spawn((
        StructurePlacementPreviewRoot,
        StructurePlacementPreviewState::default(),
        Name::new("StructurePlacementPreview"),
        Transform::default(),
        GlobalTransform::default(),
        Visibility::Hidden,
        InheritedVisibility::default(),
        ViewVisibility::default(),
        RenderLayers::layer(0),
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

/// Spawns selection outline for the `logic::entities::player::look_at_service` module.
fn spawn_selection_outline(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game_config: Res<GlobalConfig>,
    q_existing: Query<Entity, With<SelectionOutlineRoot>>,
) {
    if !q_existing.is_empty() {
        return;
    }

    let edge_mesh = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let outline_color =
        parse_hex_color(game_config.interface.block_selection_border_color.as_str())
            .unwrap_or_else(|| {
                warn!(
                    "Invalid interface.block-selection-border-color='{}', using fallback '#111111'",
                    game_config.interface.block_selection_border_color
                );
                Color::srgba(17.0 / 255.0, 17.0 / 255.0, 17.0 / 255.0, 1.0)
            });
    let edge_mat = materials.add(StandardMaterial {
        base_color: outline_color,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let s = VOXEL_SIZE;
    let half = s * 0.5 + 0.008;
    let len = s + 0.016;
    let line_width = game_config.interface.selection_line_width.clamp(0.1, 16.0);
    let t = (s * 0.010 * line_width).max(0.002);

    commands
        .spawn((
            SelectionOutlineRoot,
            Name::new("SelectionOutline"),
            Transform::default(),
            GlobalTransform::default(),
            Visibility::Hidden,
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RenderLayers::layer(0),
        ))
        .with_children(|p| {
            for y in [-half, half] {
                for z in [-half, half] {
                    p.spawn((
                        Mesh3d(edge_mesh.clone()),
                        MeshMaterial3d(edge_mat.clone()),
                        Transform::from_translation(Vec3::new(0.0, y, z))
                            .with_scale(Vec3::new(len, t, t)),
                        RenderLayers::layer(0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            for x in [-half, half] {
                for z in [-half, half] {
                    p.spawn((
                        Mesh3d(edge_mesh.clone()),
                        MeshMaterial3d(edge_mat.clone()),
                        Transform::from_translation(Vec3::new(x, 0.0, z))
                            .with_scale(Vec3::new(t, len, t)),
                        RenderLayers::layer(0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
            for x in [-half, half] {
                for y in [-half, half] {
                    p.spawn((
                        Mesh3d(edge_mesh.clone()),
                        MeshMaterial3d(edge_mat.clone()),
                        Transform::from_translation(Vec3::new(x, y, 0.0))
                            .with_scale(Vec3::new(t, t, len)),
                        RenderLayers::layer(0),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            }
        });
}

/// Updates selection for the `logic::entities::player::look_at_service` module.
fn update_selection(
    mut sel: ResMut<SelectionState>,
    game_mode: Res<GameModeState>,
    registry: Res<BlockRegistry>,
    rapier_context: ReadRapierContext,
    q_player_cam: Query<(&GlobalTransform, &Camera), With<PlayerCamera>>,
    q_fallback_cam: Query<(&GlobalTransform, &Camera), With<Camera3d>>,
    q_structure_meta: Query<&PlacedStructureMetadata>,
    q_parents: Query<&ChildOf>,
    chunk_map: Res<ChunkMap>,
) {
    if matches!(game_mode.0, GameMode::Spectator) {
        sel.hit = None;
        sel.structure_hit = None;
        return;
    }

    let cam = q_player_cam
        .iter()
        .next()
        .or_else(|| q_fallback_cam.iter().next());
    let Some((tf, _cam)) = cam else {
        sel.hit = None;
        sel.structure_hit = None;
        return;
    };

    let origin_bs = tf.translation() / VOXEL_SIZE;
    let dir_bs: Vec3 = tf.forward().into();
    let max_dist_blocks = 8.0;

    let voxel_hit = ray_cast_voxels(origin_bs, dir_bs, max_dist_blocks, &chunk_map, &registry);
    let voxel_hit_dist = voxel_hit.map(|hit| {
        let hit_world = (hit.block_pos.as_vec3() + hit.hit_local) * VOXEL_SIZE;
        tf.translation().distance(hit_world)
    });

    let mut structure_hit = None;
    if let Ok(ctx) = rapier_context.single() {
        let structure_filter = |entity: Entity| -> bool {
            is_structure_collider_entity(entity, &q_structure_meta, &q_parents)
        };
        if let Some((hit_entity, intersection)) = ctx.cast_ray_and_get_normal(
            tf.translation(),
            tf.forward().into(),
            max_dist_blocks * VOXEL_SIZE,
            true,
            QueryFilter::default()
                .exclude_sensors()
                .predicate(&structure_filter),
        ) {
            let direction: Vec3 = tf.forward().into();
            let hit_world = tf.translation() + direction * intersection.time_of_impact;
            let hit_normal_world: Vec3 = intersection.normal;
            let mut current = hit_entity;
            loop {
                if let Ok(meta) = q_structure_meta.get(current) {
                    structure_hit = Some((
                        StructureHit {
                            entity: current,
                            hit_world,
                            hit_normal_world,
                            selection_center_world: meta.selection_center_world,
                            selection_size_world: meta.selection_size_world,
                        },
                        intersection.time_of_impact,
                    ));
                    break;
                }

                let Ok(parent) = q_parents.get(current) else {
                    break;
                };
                current = parent.parent();
            }
        }
    }

    match (voxel_hit, voxel_hit_dist, structure_hit) {
        (Some(block_hit), Some(block_dist), Some((structure_hit, structure_dist))) => {
            if structure_dist <= block_dist {
                sel.hit = None;
                sel.structure_hit = Some(structure_hit);
            } else {
                sel.hit = Some(block_hit);
                sel.structure_hit = None;
            }
        }
        (Some(block_hit), _, None) => {
            sel.hit = Some(block_hit);
            sel.structure_hit = None;
        }
        (None, _, Some((structure_hit, _))) => {
            sel.hit = None;
            sel.structure_hit = Some(structure_hit);
        }
        _ => {
            sel.hit = None;
            sel.structure_hit = None;
        }
    }
}

/// Synchronizes selection outline for the `logic::entities::player::look_at_service` module.
fn sync_selection_outline(
    sel: Res<SelectionState>,
    game_mode: Res<GameModeState>,
    registry: Res<BlockRegistry>,
    mut q_outline: Query<(&mut Transform, &mut Visibility), With<SelectionOutlineRoot>>,
) {
    let Ok((mut tf, mut vis)) = q_outline.single_mut() else {
        return;
    };

    if matches!(game_mode.0, GameMode::Spectator) {
        *vis = Visibility::Hidden;
        return;
    }

    if let Some(hit) = sel.structure_hit {
        tf.translation = hit.selection_center_world;
        tf.scale = hit.selection_size_world.max(Vec3::splat(0.02));
        tf.rotation = Quat::IDENTITY;
        *vis = Visibility::Visible;
        return;
    }

    if let Some(hit) = sel.hit {
        let id = hit.block_id;
        let Some((size, offset)) = registry.selection_box(id) else {
            *vis = Visibility::Hidden;
            return;
        };
        let s = VOXEL_SIZE;
        tf.translation = Vec3::new(
            (hit.block_pos.x as f32 + 0.5 + offset[0]) * s,
            (hit.block_pos.y as f32 + 0.5 + offset[1]) * s,
            (hit.block_pos.z as f32 + 0.5 + offset[2]) * s,
        );
        tf.scale = Vec3::new(size[0], size[1], size[2]).max(Vec3::splat(0.02));
        tf.rotation = Quat::IDENTITY;
        *vis = Visibility::Visible;
    } else {
        *vis = Visibility::Hidden;
    }
}

fn is_structure_collider_entity(
    entity: Entity,
    q_structure_meta: &Query<&PlacedStructureMetadata>,
    q_parents: &Query<&ChildOf>,
) -> bool {
    let mut current = entity;
    loop {
        if q_structure_meta.get(current).is_ok() {
            return true;
        }
        let Ok(parent) = q_parents.get(current) else {
            return false;
        };
        current = parent.parent();
    }
}

/// Synchronizes placement preview for slab placement in the `logic::entities::player::look_at_service` module.
fn sync_placement_preview(
    sel: Res<SelectionState>,
    selected: Res<SelectedBlock>,
    game_mode: Res<GameModeState>,
    registry: Res<BlockRegistry>,
    chunk_map: Res<ChunkMap>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q_player_controls: Query<&FpsController, With<Player>>,
    mut q_preview: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut PlacementPreviewState,
            &MeshMaterial3d<StandardMaterial>,
        ),
        With<PlacementPreviewRoot>,
    >,
) {
    let Ok((mut tf, mut vis, mut preview_state, preview_mat)) = q_preview.single_mut() else {
        return;
    };

    if matches!(game_mode.0, GameMode::Spectator) {
        *vis = Visibility::Hidden;
        return;
    }

    let Some(selected_name) = registry.name_opt(selected.id) else {
        *vis = Visibility::Hidden;
        return;
    };
    if !is_slab_block_name(selected_name) {
        *vis = Visibility::Hidden;
        return;
    }

    let Some(hit) = sel.hit else {
        *vis = Visibility::Hidden;
        return;
    };

    let (player_yaw, player_pitch) = q_player_controls
        .iter()
        .next()
        .map(|ctrl| (ctrl.yaw, ctrl.pitch))
        .unwrap_or((0.0, 0.0));
    let placement = resolve_placement_for_selected(
        selected.id,
        hit,
        player_yaw,
        player_pitch,
        &chunk_map,
        &registry,
    );
    let mut preview_world_pos = placement.world_pos;
    let mut preview_place_into_stacked = placement.place_into_stacked;
    let hit_primary_id = get_block_world(&chunk_map, hit.block_pos);
    if !hit.is_stacked && hit_primary_id != 0 && registry.is_overridable(hit_primary_id) {
        preview_world_pos = hit.block_pos;
        preview_place_into_stacked = false;
    }
    let existing_primary_id = get_block_world(&chunk_map, preview_world_pos);
    let existing_stacked_id = get_stacked_block_world(&chunk_map, preview_world_pos);
    if !preview_place_into_stacked
        && registry.is_water_logged(placement.block_id)
        && existing_primary_id != 0
        && registry.is_fluid(existing_primary_id)
        && existing_stacked_id == 0
    {
        preview_place_into_stacked = true;
    }

    if !placement_target_can_place(
        &chunk_map,
        &registry,
        preview_world_pos,
        preview_place_into_stacked,
        placement.block_id,
    ) {
        *vis = Visibility::Hidden;
        return;
    }

    let Some((size, offset)) = registry.selection_box(placement.block_id) else {
        *vis = Visibility::Hidden;
        return;
    };

    if preview_state.block_id != placement.block_id {
        if let Some(mat) = materials.get_mut(&preview_mat.0) {
            mat.base_color_texture = Some(registry.def(placement.block_id).image.clone());
            mat.base_color = Color::srgba(1.0, 1.0, 1.0, 0.5);
            mat.alpha_mode = AlphaMode::Blend;
            mat.unlit = false;
            mat.cull_mode = None;
        }
        preview_state.block_id = placement.block_id;
    }

    let s = VOXEL_SIZE;
    const PREVIEW_GROWTH: f32 = 0.02;
    tf.translation = Vec3::new(
        (preview_world_pos.x as f32 + 0.5 + offset[0]) * s,
        (preview_world_pos.y as f32 + 0.5 + offset[1]) * s,
        (preview_world_pos.z as f32 + 0.5 + offset[2]) * s,
    ) + preview_face_nudge(hit.face, 0.008, 0.004);
    tf.scale =
        (Vec3::new(size[0], size[1], size[2]) + Vec3::splat(PREVIEW_GROWTH)).max(Vec3::splat(0.02));
    *vis = Visibility::Visible;
}

/// Synchronizes placement preview for active structure recipes in the `logic::entities::player::look_at_service` module.
fn sync_structure_placement_preview(
    sel: Res<SelectionState>,
    game_mode: Res<GameModeState>,
    inventory: Res<PlayerInventory>,
    hotbar_selection: Res<HotbarSelectionState>,
    item_registry: Res<ItemRegistry>,
    registry: Res<BlockRegistry>,
    chunk_map: Res<ChunkMap>,
    asset_server: Res<AssetServer>,
    structure_recipe_registry: Option<Res<BuildingStructureRecipeRegistry>>,
    active_structure_recipe: Res<ActiveStructureRecipeState>,
    active_structure_placement: Res<ActiveStructurePlacementState>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q_children: Query<&Children>,
    mut q_scene_materials: Query<(
        Entity,
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&StructurePreviewOwnedMaterial>,
    )>,
    mut commands: Commands,
    mut q_preview: Query<
        (
            Entity,
            &mut Transform,
            &mut Visibility,
            &mut StructurePlacementPreviewState,
        ),
        With<StructurePlacementPreviewRoot>,
    >,
) {
    let Ok((preview_entity, mut tf, mut vis, mut preview_state)) = q_preview.single_mut() else {
        return;
    };

    if matches!(game_mode.0, GameMode::Spectator) {
        *vis = Visibility::Hidden;
        return;
    }
    if !is_hammer_selected(&inventory, &hotbar_selection, &item_registry) {
        *vis = Visibility::Hidden;
        return;
    }

    let Some(structure_recipe_registry) = structure_recipe_registry.as_ref() else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some(recipe_name) = active_structure_recipe.selected_recipe_name.as_deref() else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some(recipe) = structure_recipe_registry.recipe_by_name(recipe_name) else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some(hit) = sel.hit else {
        *vis = Visibility::Hidden;
        return;
    };

    if preview_state.recipe_name.as_deref() != Some(recipe.name.as_str()) {
        if let Some(scene_entity) = preview_state.scene_entity.take() {
            commands.entity(scene_entity).despawn();
        }
        let scene_handle = asset_server.load(recipe.model_asset_path.clone());
        let scene_entity = commands
            .spawn((
                Name::new("StructurePlacementPreviewScene"),
                StructurePlacementPreviewSceneRoot,
                SceneRoot(scene_handle),
                Transform::default(),
                GlobalTransform::default(),
                Visibility::Inherited,
                InheritedVisibility::default(),
                ViewVisibility::default(),
                RenderLayers::layer(0),
                NotShadowCaster,
                NotShadowReceiver,
                Pickable::IGNORE,
            ))
            .id();
        commands.entity(preview_entity).add_child(scene_entity);
        preview_state.scene_entity = Some(scene_entity);
        preview_state.recipe_name = Some(recipe.name.clone());
    }

    // Keep placement preview constrained to right angles.
    let rotation_steps =
        normalize_rotation_steps(active_structure_placement.rotation_quarters) & !1;
    let rotation_quarters = rotation_steps_to_placement_quarters(rotation_steps);
    let place_origin = resolve_structure_place_origin(hit, &chunk_map, &registry);
    let can_place = can_place_structure_recipe_at(
        place_origin,
        recipe.space,
        rotation_quarters,
        &chunk_map,
        &registry,
    );

    *tf = structure_preview_model_transform(recipe, place_origin, rotation_steps);

    if let Some(scene_root) = preview_state.scene_entity {
        apply_structure_preview_scene_materials(
            scene_root,
            can_place,
            &q_children,
            &mut q_scene_materials,
            &mut materials,
            &mut commands,
        );
    }

    preview_state.can_place = can_place;
    *vis = Visibility::Visible;
}

/// Picks block from look for the `logic::entities::player::look_at_service` module.
fn pick_block_from_look(
    buttons: Res<ButtonInput<MouseButton>>,
    game_mode: Res<GameModeState>,
    sel_state: Res<SelectionState>,
    item_registry: Res<ItemRegistry>,
    reg: Res<BlockRegistry>,
    ui_state: Option<Res<UiInteractionState>>,
    mut inventory: ResMut<PlayerInventory>,
    mut hotbar_state: ResMut<HotbarSelectionState>,
    mut selected: ResMut<SelectedBlock>,
) {
    if matches!(game_mode.0, GameMode::Spectator) {
        return;
    }
    if ui_state
        .as_ref()
        .is_some_and(|state| state.blocks_game_input())
    {
        return;
    }
    if !buttons.just_pressed(MouseButton::Middle) {
        return;
    }
    let Some(hit) = sel_state.hit else {
        return;
    };

    let id = hit.block_id;
    if id == 0 {
        return;
    }

    selected.id = id;
    selected.name = reg.display_name_opt(id).unwrap_or("").to_string();

    if !matches!(game_mode.0, GameMode::Creative) {
        debug!("Picked block: {} ({})", selected.name, selected.id);
        return;
    }

    let Some(item_id) = item_registry.item_for_block(id) else {
        debug!(
            "Picked block has no block-item mapping: {} ({})",
            selected.name, selected.id
        );
        return;
    };

    let hotbar_len = HOTBAR_SLOTS.min(inventory.slots.len());
    if hotbar_len == 0 {
        return;
    }

    if let Some(existing_index) = inventory.slots[..hotbar_len]
        .iter()
        .position(|slot| slot.item_id == item_id && slot.count > 0)
    {
        hotbar_state.selected_index = existing_index;
        debug!("Picked block: {} ({})", selected.name, selected.id);
        return;
    }

    let selected_index = hotbar_state
        .selected_index
        .min(hotbar_len.saturating_sub(1));
    let selected_slot = inventory.slots[selected_index];
    let target_index = if selected_slot.is_empty() {
        selected_index
    } else {
        (1..hotbar_len)
            .map(|offset| (selected_index + offset) % hotbar_len)
            .find(|index| inventory.slots[*index].is_empty())
            .unwrap_or(selected_index)
    };

    inventory.slots[target_index] = InventorySlot { item_id, count: 1 };
    hotbar_state.selected_index = target_index;
    debug!("Picked block: {} ({})", selected.name, selected.id);
}

fn parse_hex_color(raw: &str) -> Option<Color> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);

    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::srgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                1.0,
            ))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color::srgba(
                r as f32 / 255.0,
                g as f32 / 255.0,
                b as f32 / 255.0,
                a as f32 / 255.0,
            ))
        }
        _ => None,
    }
}

#[inline]
fn is_slab_block_name(name: &str) -> bool {
    const SUFFIXES: [&str; 6] = [
        "_slab_block",
        "_slab_top_block",
        "_slab_north_block",
        "_slab_south_block",
        "_slab_east_block",
        "_slab_west_block",
    ];
    SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SlabVariantPreview {
    Bottom,
    Top,
    North,
    South,
    East,
    West,
}

#[inline]
fn slab_variant_from_name(name: &str) -> Option<SlabVariantPreview> {
    if name.ends_with("_slab_block") {
        return Some(SlabVariantPreview::Bottom);
    }
    if name.ends_with("_slab_top_block") {
        return Some(SlabVariantPreview::Top);
    }
    if name.ends_with("_slab_north_block") {
        return Some(SlabVariantPreview::North);
    }
    if name.ends_with("_slab_south_block") {
        return Some(SlabVariantPreview::South);
    }
    if name.ends_with("_slab_east_block") {
        return Some(SlabVariantPreview::East);
    }
    if name.ends_with("_slab_west_block") {
        return Some(SlabVariantPreview::West);
    }
    None
}

#[inline]
fn slab_variant_from_block_id(
    block_id: BlockId,
    registry: &BlockRegistry,
) -> Option<SlabVariantPreview> {
    let name = registry.name_opt(block_id)?;
    slab_variant_from_name(name)
}

#[inline]
fn slab_ids_are_complementary(a: BlockId, b: BlockId, registry: &BlockRegistry) -> bool {
    let Some(a_variant) = slab_variant_from_block_id(a, registry) else {
        return false;
    };
    let Some(b_variant) = slab_variant_from_block_id(b, registry) else {
        return false;
    };
    matches!(
        (a_variant, b_variant),
        (SlabVariantPreview::Bottom, SlabVariantPreview::Top)
            | (SlabVariantPreview::Top, SlabVariantPreview::Bottom)
            | (SlabVariantPreview::North, SlabVariantPreview::South)
            | (SlabVariantPreview::South, SlabVariantPreview::North)
            | (SlabVariantPreview::East, SlabVariantPreview::West)
            | (SlabVariantPreview::West, SlabVariantPreview::East)
    )
}

fn is_hammer_selected(
    inventory: &PlayerInventory,
    hotbar_selection: &HotbarSelectionState,
    item_registry: &ItemRegistry,
) -> bool {
    let Some(slot) = inventory.slots.get(hotbar_selection.selected_index) else {
        return false;
    };
    if slot.is_empty() {
        return false;
    }
    let Some(item) = item_registry.def_opt(slot.item_id) else {
        return false;
    };
    item.localized_name == "oplexa:hammer" || item.key == "hammer"
}

fn resolve_structure_place_origin(
    hit: crate::core::entities::player::block_selection::BlockHit,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> IVec3 {
    let hit_primary_id = get_block_world(chunk_map, hit.block_pos);
    if !hit.is_stacked && hit_primary_id != 0 && registry.is_overridable(hit_primary_id) {
        hit.block_pos
    } else {
        hit.place_pos
    }
}

fn can_place_structure_recipe_at(
    place_origin: IVec3,
    space: UVec3,
    rotation_quarters: u8,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> bool {
    for y_offset in 0..space.y as i32 {
        for local_z in 0..space.z as i32 {
            for local_x in 0..space.x as i32 {
                let (x_offset, z_offset) = rotated_structure_offset(
                    local_x,
                    local_z,
                    space.x as i32,
                    space.z as i32,
                    rotation_quarters,
                );
                let world_pos = place_origin + IVec3::new(x_offset, y_offset, z_offset);
                if !is_structure_cell_placeable(world_pos, chunk_map, registry) {
                    return false;
                }
            }
        }
    }

    for local_z in 0..space.z as i32 {
        for local_x in 0..space.x as i32 {
            let (x_offset, z_offset) = rotated_structure_offset(
                local_x,
                local_z,
                space.x as i32,
                space.z as i32,
                rotation_quarters,
            );
            let support_pos = place_origin + IVec3::new(x_offset, -1, z_offset);
            if !is_structure_support_cell(support_pos, chunk_map, registry) {
                return false;
            }
        }
    }

    true
}

fn apply_structure_preview_scene_materials(
    scene_root: Entity,
    can_place: bool,
    q_children: &Query<&Children>,
    q_scene_materials: &mut Query<(
        Entity,
        &mut MeshMaterial3d<StandardMaterial>,
        Option<&StructurePreviewOwnedMaterial>,
    )>,
    materials: &mut Assets<StandardMaterial>,
    commands: &mut Commands,
) {
    let mut stack = vec![scene_root];
    while let Some(entity) = stack.pop() {
        commands.entity(entity).insert(Pickable::IGNORE);

        if let Ok(children) = q_children.get(entity) {
            for child in children.iter() {
                stack.push(child);
            }
        }

        let Ok((mesh_entity, mut mesh_material, maybe_owned_material)) =
            q_scene_materials.get_mut(entity)
        else {
            continue;
        };

        if maybe_owned_material.is_none() {
            let mut preview_material = materials.get(&mesh_material.0).cloned().unwrap_or_default();
            preview_material.alpha_mode = AlphaMode::Blend;
            preview_material.cull_mode = None;
            preview_material.unlit = false;
            preview_material.base_color = Color::srgba(1.0, 1.0, 1.0, 0.5);

            mesh_material.0 = materials.add(preview_material);
            commands
                .entity(mesh_entity)
                .insert(StructurePreviewOwnedMaterial);
        }

        if let Some(material) = materials.get_mut(&mesh_material.0) {
            material.alpha_mode = AlphaMode::Blend;
            material.cull_mode = None;
            material.unlit = false;
            material.base_color = if can_place {
                Color::srgba(1.0, 1.0, 1.0, 0.5)
            } else {
                Color::srgba(1.0, 0.45, 0.45, 0.5)
            };
        }
    }
}

fn is_structure_cell_placeable(
    world_pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> bool {
    if world_pos.y < Y_MIN || world_pos.y > (Y_MIN + CY as i32 - 1) {
        return false;
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let Some(chunk) = chunk_map.chunks.get(&chunk_coord) else {
        return false;
    };

    let lx = local.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = local.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    let existing = chunk.get(lx, ly, lz);
    let stacked = chunk.get_stacked(lx, ly, lz);
    (existing == 0 || registry.is_overridable(existing)) && stacked == 0
}

fn is_structure_support_cell(
    world_pos: IVec3,
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
) -> bool {
    if world_pos.y < Y_MIN || world_pos.y > (Y_MIN + CY as i32 - 1) {
        return false;
    }

    let (chunk_coord, local) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let Some(chunk) = chunk_map.chunks.get(&chunk_coord) else {
        return false;
    };

    let lx = local.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = local.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    let existing = chunk.get(lx, ly, lz);
    let stacked = chunk.get_stacked(lx, ly, lz);
    (existing != 0 && !registry.is_overridable(existing))
        || (stacked != 0 && !registry.is_overridable(stacked))
}

fn structure_preview_model_transform(
    recipe: &BuildingStructureRecipe,
    place_origin: IVec3,
    rotation_steps: u8,
) -> Transform {
    let placement_quarters = rotation_steps_to_placement_quarters(rotation_steps);
    let model_rotation_quarters =
        normalize_rotation_quarters(recipe.model_meta.model_rotation_quarters as i32);
    let model_rotation_steps =
        normalize_rotation_steps(rotation_steps as i32 + (model_rotation_quarters as i32 * 2));
    let rotation =
        Quat::from_rotation_y(-(model_rotation_steps as f32) * std::f32::consts::FRAC_PI_4);
    let translation = structure_model_translation(
        recipe,
        place_origin,
        placement_quarters,
        model_rotation_quarters,
    ) + (rotation * recipe.model_meta.model_offset) * VOXEL_SIZE
        + Vec3::Y * (0.1 * VOXEL_SIZE);

    Transform {
        translation,
        rotation,
        scale: Vec3::ONE,
    }
}

#[inline]
fn placement_target_can_place(
    chunk_map: &ChunkMap,
    registry: &BlockRegistry,
    world_pos: IVec3,
    place_into_stacked: bool,
    place_id: BlockId,
) -> bool {
    let (chunk_coord, l) = world_to_chunk_xz(world_pos.x, world_pos.z);
    let lx = l.x.clamp(0, (CX as i32 - 1) as u32) as usize;
    let lz = l.y.clamp(0, (CZ as i32 - 1) as u32) as usize;
    let ly = (world_pos.y - Y_MIN).clamp(0, CY as i32 - 1) as usize;

    chunk_map
        .chunks
        .get(&chunk_coord)
        .map(|ch| {
            let current = ch.get(lx, ly, lz);
            let stacked = ch.get_stacked(lx, ly, lz);
            if place_into_stacked {
                if current == 0 {
                    false
                } else if stacked == 0 {
                    true
                } else {
                    registry.is_fluid(current)
                        && slab_ids_are_complementary(stacked, place_id, registry)
                }
            } else {
                current == 0 || registry.is_overridable(current)
            }
        })
        .unwrap_or(false)
}

#[inline]
fn preview_face_nudge(face: Face, normal_amount: f32, omni_amount: f32) -> Vec3 {
    let normal = match face {
        Face::Top => Vec3::new(0.0, normal_amount, 0.0),
        Face::Bottom => Vec3::new(0.0, -normal_amount, 0.0),
        Face::North => Vec3::new(0.0, 0.0, -normal_amount),
        Face::South => Vec3::new(0.0, 0.0, normal_amount),
        Face::East => Vec3::new(normal_amount, 0.0, 0.0),
        Face::West => Vec3::new(-normal_amount, 0.0, 0.0),
    };
    // Keep preview slightly detached in all axes to avoid side-face z-fighting.
    let omni = match face {
        Face::Top => Vec3::new(omni_amount, omni_amount, omni_amount),
        Face::Bottom => Vec3::new(omni_amount, -omni_amount, omni_amount),
        Face::North => Vec3::new(omni_amount, omni_amount, -omni_amount),
        Face::South => Vec3::new(omni_amount, omni_amount, omni_amount),
        Face::East => Vec3::new(omni_amount, omni_amount, omni_amount),
        Face::West => Vec3::new(-omni_amount, omni_amount, omni_amount),
    };
    normal + omni
}

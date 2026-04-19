/// Chest structure animation control linked to chest inventory UI open/close events.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChestAnimationPlayback {
    Idle,
    Opening,
    Closing,
}

#[derive(Component, Clone)]
struct ChestAnimationController {
    clip_handle: Handle<AnimationClip>,
    graph_handle: Handle<AnimationGraph>,
    node_index: AnimationNodeIndex,
    player_entity: Option<Entity>,
    block_name: String,
    animation_missing_warned: bool,
    animation_disabled: bool,
    desired_open: bool,
    is_open: bool,
    playback: ChestAnimationPlayback,
}

fn mark_chest_structures_for_animation(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
    q_new: Query<
        (Entity, &PlacedStructureMetadata),
        (Added<PlacedStructureMetadata>, Without<ChestAnimationController>),
    >,
) {
    for (entity, meta) in &q_new {
        if !is_chest_structure_metadata(meta) {
            continue;
        }
        if !meta.model_animated {
            continue;
        }
        let Some(model_glb_path) = normalize_structure_scene_asset_path(meta.model_asset_path.as_str())
        else {
            continue;
        };
        let clip_handle: Handle<AnimationClip> = asset_server.load(
            bevy::gltf::GltfAssetLabel::Animation(0).from_asset(model_glb_path),
        );
        let (graph, node_index) = AnimationGraph::from_clip(clip_handle.clone());
        let graph_handle = animation_graphs.add(graph);
        commands.entity(entity).insert(ChestAnimationController {
            clip_handle,
            graph_handle,
            node_index,
            player_entity: None,
            block_name: chest_animation_block_name(meta),
            animation_missing_warned: false,
            animation_disabled: false,
            desired_open: false,
            is_open: false,
            playback: ChestAnimationPlayback::Idle,
        });
    }
}

fn bind_chest_animation_players(
    mut commands: Commands,
    scene_spawner: Res<bevy::scene::SceneSpawner>,
    q_children: Query<&Children>,
    q_player_present: Query<(), With<AnimationPlayer>>,
    mut q_players: Query<&mut AnimationPlayer>,
    mut q_chests: Query<(Entity, &bevy::scene::SceneInstance, &mut ChestAnimationController)>,
) {
    for (structure_entity, scene_instance, mut controller) in &mut q_chests {
        if controller.animation_disabled {
            continue;
        }
        if controller.player_entity.is_some() {
            continue;
        }
        if !scene_spawner.instance_is_ready(**scene_instance) {
            continue;
        }

        let mut resolved_player_entity = None;
        for child_entity in q_children.iter_descendants(structure_entity) {
            if q_player_present.get(child_entity).is_ok() {
                resolved_player_entity = Some(child_entity);
                break;
            }
        }
        let Some(player_entity) = resolved_player_entity else {
            continue;
        };

        commands
            .entity(player_entity)
            .insert(AnimationGraphHandle(controller.graph_handle.clone()));

        if let Ok(mut player) = q_players.get_mut(player_entity) {
            let active = player.start(controller.node_index);
            active
                .set_repeat(bevy::animation::RepeatAnimation::Never)
                .seek_to(0.0)
                .pause();
        }
        controller.player_entity = Some(player_entity);
        controller.is_open = false;
        controller.playback = ChestAnimationPlayback::Idle;
    }
}

fn apply_chest_ui_animation_requests(
    mut opened: MessageReader<ChestInventoryUiOpened>,
    mut closed: MessageReader<ChestInventoryUiClosed>,
    mut q_chests: Query<(&PlacedStructureMetadata, &mut ChestAnimationController)>,
) {
    for message in opened.read() {
        let target = IVec3::new(message.world_pos[0], message.world_pos[1], message.world_pos[2]);
        for (meta, mut controller) in &mut q_chests {
            if meta.place_origin == target {
                controller.desired_open = true;
                break;
            }
        }
    }

    for message in closed.read() {
        let target = IVec3::new(message.world_pos[0], message.world_pos[1], message.world_pos[2]);
        for (meta, mut controller) in &mut q_chests {
            if meta.place_origin == target {
                controller.desired_open = false;
                break;
            }
        }
    }
}

fn update_chest_animation_playback(
    asset_server: Res<AssetServer>,
    clips: Res<Assets<AnimationClip>>,
    mut q_players: Query<&mut AnimationPlayer>,
    mut q_chests: Query<&mut ChestAnimationController>,
) {
    for mut controller in &mut q_chests {
        if controller.animation_disabled {
            continue;
        }
        if matches!(
            asset_server.get_load_state(controller.clip_handle.id()),
            Some(bevy::asset::LoadState::Failed(_))
        )
        {
            if !controller.animation_missing_warned {
                controller.animation_missing_warned = true;
                bevy::log::warn!(
                    "No animation found for block {}...",
                    controller.block_name
                );
            }
            controller.animation_disabled = true;
            controller.playback = ChestAnimationPlayback::Idle;
            continue;
        }
        let Some(player_entity) = controller.player_entity else {
            continue;
        };
        let Some(clip) = clips.get(&controller.clip_handle) else {
            continue;
        };
        let duration = clip.duration().max(0.001);
        let open_end = duration * 0.5;
        let close_end = (duration - 0.0001).max(0.0);

        let Ok(mut player) = q_players.get_mut(player_entity) else {
            controller.player_entity = None;
            controller.playback = ChestAnimationPlayback::Idle;
            continue;
        };

        let should_start_open = controller.desired_open
            && !controller.is_open
            && controller.playback != ChestAnimationPlayback::Opening;
        let should_start_close = !controller.desired_open
            && (controller.is_open || controller.playback == ChestAnimationPlayback::Opening)
            && controller.playback != ChestAnimationPlayback::Closing;

        if should_start_open || should_start_close {
            let active = player.start(controller.node_index);
            active
                .set_repeat(bevy::animation::RepeatAnimation::Never)
                .set_speed(1.0)
                .resume();
            if should_start_open {
                active.seek_to(0.0);
                controller.playback = ChestAnimationPlayback::Opening;
            } else {
                active.seek_to(open_end);
                controller.playback = ChestAnimationPlayback::Closing;
            }
        }

        match controller.playback {
            ChestAnimationPlayback::Opening => {
                if let Some(active) = player.animation_mut(controller.node_index)
                    && active.seek_time() >= open_end
                {
                    active.seek_to(open_end).pause();
                    controller.is_open = true;
                    controller.playback = ChestAnimationPlayback::Idle;
                }
            }
            ChestAnimationPlayback::Closing => {
                if let Some(active) = player.animation_mut(controller.node_index)
                    && (active.is_finished() || active.seek_time() >= close_end)
                {
                    active.seek_to(close_end).pause();
                    controller.is_open = false;
                    controller.playback = ChestAnimationPlayback::Idle;
                }
            }
            ChestAnimationPlayback::Idle => {}
        }
    }
}

#[inline]
fn is_chest_structure_metadata(meta: &PlacedStructureMetadata) -> bool {
    if meta.recipe_name.eq_ignore_ascii_case("chest") {
        return true;
    }
    meta.registration.as_ref().is_some_and(|registration| {
        registration
            .localized_name
            .eq_ignore_ascii_case("chest_block")
    })
}

#[inline]
fn normalize_structure_scene_asset_path(raw_scene_path: &str) -> Option<String> {
    let glb_path = raw_scene_path.split('#').next()?.trim();
    if glb_path.is_empty() {
        return None;
    }
    Some(glb_path.to_string())
}

#[inline]
fn chest_animation_block_name(meta: &PlacedStructureMetadata) -> String {
    meta.registration
        .as_ref()
        .map(|registration| registration.localized_name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| meta.recipe_name.clone())
}

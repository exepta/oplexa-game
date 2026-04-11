/// Represents mining overlay used by the `logic::events::block_event_handler` module.
#[derive(Component)]
struct MiningOverlay;

/// Represents mining overlay face used by the `logic::events::block_event_handler` module.
#[derive(Component)]
struct MiningOverlayFace;

/// Defines the possible axis variants in the `logic::events::block_event_handler` module.
#[derive(Clone, Copy)]
enum Axis {
    XY,
    XZ,
    YZ,
}

/// Synchronizes mining overlay for the `logic::events::block_event_handler` module.
fn sync_mining_overlay(
    mut commands: Commands,
    mut root: ResMut<MiningOverlayRoot>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    state: Res<MiningState>,
    mut q_faces: Query<
        (&mut Transform, &MiningOverlayFace),
        (With<MiningOverlayFace>, Without<MiningOverlay>),
    >,
    mut q_parent_tf: Query<&mut Transform, (With<MiningOverlay>, Without<MiningOverlayFace>)>,
) {
    let Some(target) = state.target else {
        if let Some(e) = root.0.take() {
            safe_despawn_entity(&mut commands, e);
        }
        return;
    };

    let now = time.elapsed_secs();
    let progress = ((now - target.started_at) / target.duration).clamp(0.0, 1.0);

    let s = VOXEL_SIZE;
    let center = Vec3::new(
        (target.loc.x as f32 + 0.5) * s,
        (target.loc.y as f32 + 0.5) * s,
        (target.loc.z as f32 + 0.5) * s,
    );

    let parent_e = if let Some(e) = root.0 {
        e
    } else {
        let e = spawn_overlay_at(
            &mut commands,
            &mut meshes,
            &mut mats,
            center,
            Some(RenderLayers::layer(2)),
            progress,
        );
        root.0 = Some(e);
        e
    };

    if let Ok(mut tf) = q_parent_tf.get_mut(parent_e) {
        tf.translation = center;
    }

    let max_scale = 0.98 * s;
    let size = max_scale * progress;
    let face_scale = Vec3::new(size, size, 1.0);

    for (mut tf, _) in q_faces.iter_mut() {
        tf.scale = face_scale;
    }

    if progress >= 1.0 {
        if let Some(e) = root.0.take() {
            safe_despawn_entity(&mut commands, e);
        }
    }
}

/// Spawns overlay at for the `logic::events::block_event_handler` module.
#[inline]
fn spawn_overlay_at(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    mats: &mut Assets<StandardMaterial>,
    world_center: Vec3,
    layer: Option<RenderLayers>,
    initial_progress: f32,
) -> Entity {
    let quad = meshes.add(unit_centered_quad());
    let mat = mats.add(StandardMaterial {
        base_color: Color::srgba(0.9, 0.9, 0.9, 0.02),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        perceptual_roughness: 1.0,
        ..default()
    });

    let s = VOXEL_SIZE;
    let half = 0.5 * s;
    let eps = 0.003 * s;

    let max_scale = 0.98 * s;
    let init_scale = (initial_progress.clamp(0.0, 1.0).max(0.001)) * max_scale;
    let init_vec = Vec3::new(init_scale, init_scale, 1.0);

    let mut parent = commands.spawn((
        MiningOverlay,
        Visibility::default(),
        NoFrustumCulling,
        Transform::from_translation(world_center),
        GlobalTransform::default(),
        NotShadowCaster,
        NotShadowReceiver,
        Name::new("MiningOverlay"),
    ));
    if let Some(l) = layer.as_ref() {
        parent.insert(l.clone());
    }
    let parent_id = parent.id();

    let spawn_face = |c: &mut RelatedSpawnerCommands<ChildOf>, _: Axis, tf: Transform| {
        let mut e = c.spawn((
            MiningOverlayFace,
            Visibility::default(),
            Mesh3d(quad.clone()),
            MeshMaterial3d(mat.clone()),
            tf.with_scale(init_vec),
            GlobalTransform::default(),
            NotShadowCaster,
            NotShadowReceiver,
            Name::new("MiningOverlayFace"),
        ));
        if let Some(l) = layer.as_ref() {
            e.insert(l.clone());
        }
    };

    commands.entity(parent_id).with_children(|c| {
        // +Z / -Z (XY)
        spawn_face(
            c,
            Axis::XY,
            Transform::from_translation(Vec3::new(0.0, 0.0, half + eps)),
        );
        spawn_face(
            c,
            Axis::XY,
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                .with_translation(Vec3::new(0.0, 0.0, -half - eps)),
        );

        // +Y / -Y (XZ)
        spawn_face(
            c,
            Axis::XZ,
            Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(0.0, half + eps, 0.0)),
        );
        spawn_face(
            c,
            Axis::XZ,
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(0.0, -half - eps, 0.0)),
        );

        // +X / -X (YZ)
        spawn_face(
            c,
            Axis::YZ,
            Transform::from_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(half + eps, 0.0, 0.0)),
        );
        spawn_face(
            c,
            Axis::YZ,
            Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
                .with_translation(Vec3::new(-half - eps, 0.0, 0.0)),
        );
    });

    parent_id
}

/// Runs the `unit_centered_quad` routine for unit centered quad in the `logic::events::block_event_handler` module.
#[inline]
fn unit_centered_quad() -> Mesh {
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::prelude::Mesh;
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, Default::default());
    m.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
        ],
    );
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4]);
    m.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
    );
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    m
}

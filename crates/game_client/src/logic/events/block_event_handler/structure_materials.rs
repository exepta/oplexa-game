/// Structure mesh collider filtering and material/texture application helpers.
fn configure_structure_mesh_collider_name_filters(
    mut commands: Commands,
    scene_spawner: Res<bevy::scene::SceneSpawner>,
    children: Query<&Children>,
    mesh_names: Query<&Name, With<Mesh3d>>,
    mut q_pending: Query<
        (Entity, &bevy::scene::SceneInstance, &mut AsyncSceneCollider),
        With<StructureMeshColliderNameFilterPending>,
    >,
) {
    for (structure_entity, scene_instance, mut async_scene_collider) in &mut q_pending {
        if !scene_spawner.instance_is_ready(**scene_instance) {
            continue;
        }

        for child_entity in children.iter_descendants(structure_entity) {
            let Ok(name) = mesh_names.get(child_entity) else {
                continue;
            };
            if !name.as_str().to_ascii_lowercase().contains("none") {
                continue;
            }
            async_scene_collider
                .named_shapes
                .insert(name.as_str().to_string(), None);
        }

        commands
            .entity(structure_entity)
            .remove::<StructureMeshColliderNameFilterPending>();
    }
}

fn cleanup_structure_none_mesh_colliders(
    mut commands: Commands,
    scene_spawner: Res<bevy::scene::SceneSpawner>,
    q_children: Query<&Children>,
    q_names: Query<&Name>,
    q_parents: Query<&ChildOf>,
    q_colliders: Query<(), With<Collider>>,
    q_pending: Query<
        (Entity, &bevy::scene::SceneInstance, Has<AsyncSceneCollider>),
        With<StructureMeshColliderCleanupPending>,
    >,
) {
    for (structure_entity, scene_instance, has_async_scene_collider) in &q_pending {
        if !scene_spawner.instance_is_ready(**scene_instance) {
            continue;
        }
        // Wait until Rapier has finished generating child colliders.
        if has_async_scene_collider {
            continue;
        }

        for child_entity in q_children.iter_descendants(structure_entity) {
            if q_colliders.get(child_entity).is_err() {
                continue;
            }
            if !entity_or_ancestor_name_contains_none(
                child_entity,
                structure_entity,
                &q_names,
                &q_parents,
            ) {
                continue;
            }
            commands.entity(child_entity).remove::<Collider>();
        }

        commands
            .entity(structure_entity)
            .remove::<StructureMeshColliderCleanupPending>();
    }
}

fn entity_or_ancestor_name_contains_none(
    entity: Entity,
    root_entity: Entity,
    q_names: &Query<&Name>,
    q_parents: &Query<&ChildOf>,
) -> bool {
    let mut current = entity;
    loop {
        if q_names
            .get(current)
            .is_ok_and(|name| name.as_str().to_ascii_lowercase().contains("none"))
        {
            return true;
        }
        if current == root_entity {
            return false;
        }
        let Ok(parent) = q_parents.get(current) else {
            return false;
        };
        current = parent.parent();
    }
}

fn apply_structure_style_material_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    meshes: Res<Assets<Mesh>>,
    item_registry: Res<ItemRegistry>,
    block_registry: Res<BlockRegistry>,
    q_pending: Query<
        (
            Entity,
            Option<&StructureStyleSourceItem>,
            Option<&StructureTextureBindings>,
        ),
        With<StructureStyleMaterialPending>,
    >,
    q_children: Query<&Children>,
    q_names: Query<&Name>,
    q_parents: Query<&ChildOf>,
    q_meshes: Query<&Mesh3d>,
    mut q_mesh_materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
) {
    for (structure_entity, style_source, texture_bindings) in &q_pending {
        let style_source_item_id = style_source
            .map(|style_source| style_source.item_id)
            .filter(|item_id| *item_id != 0);
        let style_material = style_source_item_id.and_then(|item_id| {
            resolve_structure_style_material_handle(item_id, &item_registry, &block_registry)
        });
        let apply_stats = apply_materials_to_structure_descendants(
            structure_entity,
            style_source_item_id,
            style_material.as_ref(),
            texture_bindings.map(|bindings| bindings.entries.as_slice()),
            &asset_server,
            &mut materials,
            &mut images,
            &meshes,
            &item_registry,
            &block_registry,
            &q_children,
            &q_names,
            &q_parents,
            &q_meshes,
            &mut q_mesh_materials,
        );

        if apply_stats.mesh_count > 0 || (style_material.is_none() && texture_bindings.is_none()) {
            commands
                .entity(structure_entity)
                .remove::<StructureStyleMaterialPending>();
        }
    }
}

fn resolve_structure_style_material_handle(
    style_source_item_id: ItemId,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
) -> Option<Handle<StandardMaterial>> {
    if style_source_item_id == 0 {
        return None;
    }
    let style_item_id = item_registry
        .related_item_in_group(style_source_item_id, "planks")
        .unwrap_or(style_source_item_id);
    let block_id = item_registry
        .block_for_item(style_item_id)
        .or_else(|| item_registry.block_for_item(style_source_item_id))?;
    block_registry
        .def_opt(block_id)
        .map(|block| block.material.clone())
}

#[derive(Clone, Copy, Debug, Default)]
struct StructureMaterialApplyStats {
    mesh_count: usize,
    changed: usize,
}

fn apply_materials_to_structure_descendants(
    structure_entity: Entity,
    style_source_item_id: Option<ItemId>,
    style_material: Option<&Handle<StandardMaterial>>,
    texture_bindings: Option<&[BuildingStructureTextureBinding]>,
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    meshes: &Assets<Mesh>,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    q_children: &Query<&Children>,
    q_names: &Query<&Name>,
    q_parents: &Query<&ChildOf>,
    q_meshes: &Query<&Mesh3d>,
    q_mesh_materials: &mut Query<&mut MeshMaterial3d<StandardMaterial>>,
) -> StructureMaterialApplyStats {
    let mut stats = StructureMaterialApplyStats::default();
    let mut stack: Vec<Entity> = Vec::new();
    let mut uv_bounds_cache: HashMap<AssetId<Mesh>, Option<[[f32; 2]; 2]>> = HashMap::new();
    if let Ok(children) = q_children.get(structure_entity) {
        stack.extend(children.iter());
    }

    while let Some(entity) = stack.pop() {
        if let Ok(mut mesh_material) = q_mesh_materials.get_mut(entity) {
            stats.mesh_count += 1;
            let mesh_name_haystack =
                mesh_name_haystack(entity, structure_entity, q_names, q_parents);
            let uv_bounds =
                mesh_uv_bounds_for_entity(entity, q_meshes, meshes, &mut uv_bounds_cache);
            let target_material = texture_bindings.and_then(|bindings| {
                resolve_texture_binding_material_for_mesh(
                    bindings,
                    mesh_name_haystack.as_str(),
                    style_source_item_id,
                    style_material,
                    asset_server,
                    materials,
                    images,
                    item_registry,
                    block_registry,
                    uv_bounds,
                )
            });
            if let Some(target_material) = target_material.or_else(|| style_material.cloned())
                && mesh_material.0 != target_material
            {
                mesh_material.0 = target_material;
                stats.changed += 1;
            }
        }
        if let Ok(children) = q_children.get(entity) {
            stack.extend(children.iter());
        }
    }

    stats
}

fn resolve_texture_binding_material_for_mesh(
    texture_bindings: &[BuildingStructureTextureBinding],
    mesh_name_haystack: &str,
    style_source_item_id: Option<ItemId>,
    style_material: Option<&Handle<StandardMaterial>>,
    asset_server: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    uv_bounds: Option<[[f32; 2]; 2]>,
) -> Option<Handle<StandardMaterial>> {
    let binding = texture_bindings
        .iter()
        .find(|binding| mesh_name_haystack.contains(binding.mesh_name_contains.as_str()))?;

    let resolved = match &binding.source {
        BuildingStructureTextureSource::Group { group, tile } => resolve_group_texture_material(
            style_source_item_id,
            style_material,
            group.as_str(),
            *tile,
            binding.uv_repeat,
            materials,
            images,
            item_registry,
            block_registry,
            uv_bounds,
        )?,
        BuildingStructureTextureSource::DirectPath { asset_path } => {
            let mut material = style_material
                .and_then(|handle| materials.get(handle))
                .cloned()
                .unwrap_or(StandardMaterial {
                    metallic: 0.0,
                    perceptual_roughness: 1.0,
                    reflectance: 0.0,
                    ..default()
                });
            let texture_handle: Handle<Image> = asset_server.load(asset_path.clone());
            apply_nearest_sampler_to_texture_handle(images, &texture_handle, true);
            material.base_color_texture = Some(texture_handle);
            material.uv_transform =
                build_uv_transform([0.0, 0.0], [1.0, 1.0], binding.uv_repeat, uv_bounds);
            materials.add(material)
        }
    };
    Some(resolved)
}

fn mesh_name_haystack(
    entity: Entity,
    root_entity: Entity,
    q_names: &Query<&Name>,
    q_parents: &Query<&ChildOf>,
) -> String {
    let mut names = Vec::<String>::new();
    let mut current = entity;
    loop {
        if let Ok(name) = q_names.get(current) {
            names.push(name.as_str().to_ascii_lowercase());
        }
        if current == root_entity {
            break;
        }
        let Ok(parent) = q_parents.get(current) else {
            break;
        };
        current = parent.parent();
    }
    names.join(" > ")
}

fn resolve_group_texture_material(
    style_source_item_id: Option<ItemId>,
    style_material: Option<&Handle<StandardMaterial>>,
    group: &str,
    tile: Option<[u32; 2]>,
    uv_repeat: Option<[f32; 2]>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    item_registry: &ItemRegistry,
    block_registry: &BlockRegistry,
    uv_bounds: Option<[[f32; 2]; 2]>,
) -> Option<Handle<StandardMaterial>> {
    let style_item_id = resolve_item_for_group(style_source_item_id, group, item_registry)?;
    let block_id = item_registry
        .block_for_item(style_item_id)
        .or_else(|| style_source_item_id.and_then(|source| item_registry.block_for_item(source)))?;
    let block_def = block_registry.def_opt(block_id)?;
    apply_nearest_sampler_to_texture_handle(images, &block_def.image, tile.is_none());

    let mut material = materials
        .get(&block_def.material)
        .cloned()
        .or_else(|| {
            style_material
                .and_then(|handle| materials.get(handle))
                .cloned()
        })
        .unwrap_or(StandardMaterial {
            metallic: 0.0,
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            ..default()
        });

    if let Some([tile_x, tile_y]) = tile {
        let (tile_offset, tile_scale) =
            uv_rect_for_block_tile(block_def.localized_name.as_str(), tile_x, tile_y)?;
        material.uv_transform = build_uv_transform(tile_offset, tile_scale, uv_repeat, uv_bounds);
    } else {
        material.uv_transform = build_uv_transform([0.0, 0.0], [1.0, 1.0], uv_repeat, uv_bounds);
    }

    Some(materials.add(material))
}

#[inline]
fn mesh_uv_bounds_for_entity(
    entity: Entity,
    q_meshes: &Query<&Mesh3d>,
    meshes: &Assets<Mesh>,
    cache: &mut HashMap<AssetId<Mesh>, Option<[[f32; 2]; 2]>>,
) -> Option<[[f32; 2]; 2]> {
    let mesh_handle = q_meshes.get(entity).ok()?;
    let mesh_id = mesh_handle.0.id();
    if let Some(cached) = cache.get(&mesh_id) {
        return *cached;
    }
    let bounds = meshes
        .get(&mesh_handle.0)
        .and_then(mesh_uv_bounds)
        .map(|(min, max)| [min, max]);
    cache.insert(mesh_id, bounds);
    bounds
}

fn mesh_uv_bounds(mesh: &Mesh) -> Option<([f32; 2], [f32; 2])> {
    let values = mesh.attribute(Mesh::ATTRIBUTE_UV_0)?;
    let VertexAttributeValues::Float32x2(uvs) = values else {
        return None;
    };
    let first = uvs.first()?;
    let mut min_u = first[0];
    let mut max_u = first[0];
    let mut min_v = first[1];
    let mut max_v = first[1];
    for uv in uvs.iter().skip(1) {
        min_u = min_u.min(uv[0]);
        max_u = max_u.max(uv[0]);
        min_v = min_v.min(uv[1]);
        max_v = max_v.max(uv[1]);
    }
    Some(([min_u, min_v], [max_u, max_v]))
}

fn build_uv_transform(
    base_offset: [f32; 2],
    base_scale: [f32; 2],
    uv_repeat: Option<[f32; 2]>,
    uv_bounds: Option<[[f32; 2]; 2]>,
) -> Affine2 {
    let repeat = uv_repeat.unwrap_or([1.0, 1.0]);
    let bounds = uv_bounds.unwrap_or([[0.0, 0.0], [1.0, 1.0]]);
    let min_u = bounds[0][0];
    let min_v = bounds[0][1];
    let range_u = (bounds[1][0] - bounds[0][0]).abs().max(0.000_01);
    let range_v = (bounds[1][1] - bounds[0][1]).abs().max(0.000_01);

    let scale_u = base_scale[0] * repeat[0] / range_u;
    let scale_v = base_scale[1] * repeat[1] / range_v;
    let translate_u = base_offset[0] - (min_u * scale_u);
    let translate_v = base_offset[1] - (min_v * scale_v);

    Affine2::from_scale_angle_translation(
        Vec2::new(scale_u, scale_v),
        0.0,
        Vec2::new(translate_u, translate_v),
    )
}

#[inline]
fn apply_nearest_sampler_to_image(
    images: &mut Assets<Image>,
    image_id: AssetId<Image>,
    repeat: bool,
) {
    let Some(image) = images.get_mut(image_id) else {
        return;
    };
    let address_mode = if repeat {
        bevy::image::ImageAddressMode::Repeat
    } else {
        bevy::image::ImageAddressMode::ClampToEdge
    };
    image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
        address_mode_u: address_mode,
        address_mode_v: address_mode,
        address_mode_w: address_mode,
        mag_filter: bevy::image::ImageFilterMode::Nearest,
        min_filter: bevy::image::ImageFilterMode::Nearest,
        mipmap_filter: bevy::image::ImageFilterMode::Nearest,
        anisotropy_clamp: 1,
        ..default()
    });
}

#[inline]
fn apply_nearest_sampler_to_texture_handle(
    images: &mut Assets<Image>,
    texture: &Handle<Image>,
    repeat: bool,
) {
    apply_nearest_sampler_to_image(images, texture.id(), repeat);
}

fn resolve_item_for_group(
    style_source_item_id: Option<ItemId>,
    group: &str,
    item_registry: &ItemRegistry,
) -> Option<ItemId> {
    if let Some(style_source_item_id) = style_source_item_id {
        if let Some(related_item_id) =
            item_registry.related_item_in_group(style_source_item_id, group)
        {
            return Some(related_item_id);
        }
        if item_registry.has_group(style_source_item_id, group) {
            return Some(style_source_item_id);
        }
    }
    first_item_in_group(item_registry, group)
}

#[derive(Deserialize)]
struct StructureTextureDirJson {
    #[serde(default)]
    texture_dir: Option<String>,
}

#[derive(Deserialize)]
struct StructureTilesetJson {
    #[serde(default)]
    tile_size: u32,
    columns: u32,
    rows: u32,
}

fn uv_rect_for_block_tile(
    block_localized_name: &str,
    tile_x: u32,
    tile_y: u32,
) -> Option<([f32; 2], [f32; 2])> {
    const ATLAS_PAD_PX: f32 = 0.5;
    let texture_dir = resolve_texture_dir_for_block(block_localized_name);
    let tileset_path = format!("assets/{texture_dir}/data.json");
    let raw = fs::read_to_string(tileset_path).ok()?;
    let tileset = serde_json::from_str::<StructureTilesetJson>(raw.as_str()).ok()?;
    if tileset.columns == 0 || tileset.rows == 0 {
        return None;
    }
    if tile_x >= tileset.columns || tile_y >= tileset.rows {
        return None;
    }
    let tile_size = tileset.tile_size.max(1);
    let image_w = tileset.columns as f32 * tile_size as f32;
    let image_h = tileset.rows as f32 * tile_size as f32;
    let tile_w = image_w / tileset.columns as f32;
    let tile_h = image_h / tileset.rows as f32;

    let u0 = (tile_x as f32 * tile_w + ATLAS_PAD_PX) / image_w;
    let v0 = (tile_y as f32 * tile_h + ATLAS_PAD_PX) / image_h;
    let u1 = ((tile_x as f32 + 1.0) * tile_w - ATLAS_PAD_PX) / image_w;
    let v1 = ((tile_y as f32 + 1.0) * tile_h - ATLAS_PAD_PX) / image_h;

    let scale_x = (u1 - u0).max(0.000_01);
    let scale_y = (v1 - v0).max(0.000_01);
    Some(([u0, v0], [scale_x, scale_y]))
}

fn resolve_texture_dir_for_block(block_localized_name: &str) -> String {
    let block_file = format!("assets/blocks/{block_localized_name}.json");
    if let Ok(raw) = fs::read_to_string(block_file.as_str())
        && let Ok(parsed) = serde_json::from_str::<StructureTextureDirJson>(raw.as_str())
        && let Some(texture_dir) = parsed.texture_dir
    {
        let normalized = normalize_asset_path(texture_dir.as_str());
        if !normalized.is_empty() {
            return normalized;
        }
    }
    let base = block_localized_name
        .trim()
        .strip_suffix("_block")
        .unwrap_or(block_localized_name)
        .trim_matches('/');
    format!("textures/blocks/{base}")
}

#[inline]
fn normalize_asset_path(raw: &str) -> String {
    let mut value = raw.trim().replace('\\', "/");
    if let Some(stripped) = value.strip_prefix("assets/") {
        value = stripped.to_string();
    }
    if let Some(stripped) = value.strip_prefix("./") {
        value = stripped.to_string();
    }
    if Path::new(value.as_str()).as_os_str().is_empty() {
        return String::new();
    }
    value
}

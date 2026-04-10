const PLAYER_INVENTORY_SAVE_FILE_NAME: &str = "save.data";
const PLAYER_INVENTORY_SAVE_MAGIC: [u8; 4] = *b"PINV";
const PLAYER_INVENTORY_SAVE_VERSION_LEGACY: u8 = 1;
const PLAYER_INVENTORY_SAVE_VERSION_POSITION: u8 = 2;
const PLAYER_INVENTORY_SAVE_VERSION: u8 = 3;
const PLAYER_SAVE_FLAG_HAS_POSITION: u8 = 0x01;
const PLAYER_SAVE_FLAG_HAS_YAW_PITCH: u8 = 0x02;
const PLAYER_POSITION_SAVE_INTERVAL_SECS: f32 = 1.0;

/// Represents player persisted data used by the `graphic::components::inventory_persistence` module.
#[derive(Debug, Clone)]
struct PlayerPersistedData {
    inventory: PlayerInventory,
    position: Option<[f32; 3]>,
    yaw_pitch: Option<[f32; 2]>,
}

/// Represents active inventory save path used by the `graphic::components::inventory_persistence` module.
#[derive(Resource, Debug, Default, Clone)]
struct ActiveInventorySavePath {
    path: Option<PathBuf>,
    last_saved_position: Option<[f32; 3]>,
    last_saved_yaw_pitch: Option<[f32; 2]>,
    last_position_save_at_secs: f32,
}

/// Clears inventory context when entering screen for the `graphic::components::inventory_persistence` module.
fn clear_inventory_context_when_entering_screen(
    mut inventory: ResMut<PlayerInventory>,
    mut active_save_path: ResMut<ActiveInventorySavePath>,
) {
    *inventory = PlayerInventory::default();
    active_save_path.path = None;
    active_save_path.last_saved_position = None;
    active_save_path.last_saved_yaw_pitch = None;
    active_save_path.last_position_save_at_secs = 0.0;
}

/// Loads inventory for world entry for the `graphic::components::inventory_persistence` module.
fn load_inventory_for_world_entry(
    multiplayer_connection: Res<MultiplayerConnectionState>,
    world_save: Option<Res<WorldSave>>,
    mut inventory: ResMut<PlayerInventory>,
    mut active_save_path: ResMut<ActiveInventorySavePath>,
) {
    let target_path = resolve_inventory_save_path(&multiplayer_connection, world_save.as_deref());
    active_save_path.path = target_path.clone();

    let Some(path) = target_path else {
        if multiplayer_connection.uses_local_save_data() {
            *inventory = PlayerInventory::default();
        }
        active_save_path.last_saved_position = None;
        active_save_path.last_saved_yaw_pitch = None;
        return;
    };

    match read_player_data_from_file(path.as_path()) {
        Ok(Some(loaded)) => {
            *inventory = loaded.inventory;
            active_save_path.last_saved_position = loaded.position;
            active_save_path.last_saved_yaw_pitch = loaded.yaw_pitch;
        }
        Ok(None) => {
            *inventory = PlayerInventory::default();
            active_save_path.last_saved_position = None;
            active_save_path.last_saved_yaw_pitch = None;
        }
        Err(error) => {
            warn!(
                "Failed to load player inventory from {:?}: {}. Falling back to empty inventory.",
                path, error
            );
            *inventory = PlayerInventory::default();
            active_save_path.last_saved_position = None;
            active_save_path.last_saved_yaw_pitch = None;
        }
    }
}

/// Persists inventory on change for the `graphic::components::inventory_persistence` module.
fn persist_inventory_on_change(
    time: Res<Time>,
    inventory: Res<PlayerInventory>,
    q_player: Query<(&Transform, &FpsController), With<Player>>,
    mut active_save_path: ResMut<ActiveInventorySavePath>,
) {
    let Some(path) = active_save_path.path.as_ref() else {
        return;
    };

    let (player_position, player_yaw_pitch) = if let Ok((transform, controller)) = q_player.single() {
        (
            Some([
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
            ]),
            Some([controller.yaw, controller.pitch]),
        )
    } else {
        (
            active_save_path.last_saved_position,
            active_save_path.last_saved_yaw_pitch,
        )
    };
    let elapsed = time.elapsed_secs();
    let inventory_changed = inventory.is_changed();
    let position_changed = player_position != active_save_path.last_saved_position;
    let yaw_pitch_changed = player_yaw_pitch != active_save_path.last_saved_yaw_pitch;
    let periodic_position_due = elapsed - active_save_path.last_position_save_at_secs
        >= PLAYER_POSITION_SAVE_INTERVAL_SECS;

    if !inventory_changed && !(periodic_position_due && (position_changed || yaw_pitch_changed)) {
        return;
    }

    if let Err(error) = write_player_data_to_file(
        path.as_path(),
        &inventory,
        player_position,
        player_yaw_pitch,
    ) {
        warn!("Failed to save player inventory to {:?}: {}", path, error);
        return;
    }
    active_save_path.last_saved_position = player_position;
    active_save_path.last_saved_yaw_pitch = player_yaw_pitch;
    active_save_path.last_position_save_at_secs = elapsed;
}

/// Persists inventory on world exit for the `graphic::components::inventory_persistence` module.
fn persist_inventory_on_world_exit(
    inventory: Res<PlayerInventory>,
    q_player: Query<(&Transform, &FpsController), With<Player>>,
    mut active_save_path: ResMut<ActiveInventorySavePath>,
) {
    let Some(path) = active_save_path.path.as_ref() else {
        return;
    };

    let (player_position, player_yaw_pitch) = if let Ok((transform, controller)) = q_player.single() {
        (
            Some([
                transform.translation.x,
                transform.translation.y,
                transform.translation.z,
            ]),
            Some([controller.yaw, controller.pitch]),
        )
    } else {
        (
            active_save_path.last_saved_position,
            active_save_path.last_saved_yaw_pitch,
        )
    };

    if let Err(error) = write_player_data_to_file(
        path.as_path(),
        &inventory,
        player_position,
        player_yaw_pitch,
    ) {
        warn!(
            "Failed to save player inventory during world exit to {:?}: {}",
            path, error
        );
        return;
    }
    active_save_path.last_saved_position = player_position;
    active_save_path.last_saved_yaw_pitch = player_yaw_pitch;
}

/// Clears inventory after world exit for the `graphic::components::inventory_persistence` module.
fn clear_inventory_after_world_exit(
    mut inventory: ResMut<PlayerInventory>,
    mut active_save_path: ResMut<ActiveInventorySavePath>,
) {
    *inventory = PlayerInventory::default();
    active_save_path.path = None;
    active_save_path.last_saved_position = None;
    active_save_path.last_saved_yaw_pitch = None;
    active_save_path.last_position_save_at_secs = 0.0;
}

/// Resolves inventory save path for the `graphic::components::inventory_persistence` module.
fn resolve_inventory_save_path(
    multiplayer_connection: &MultiplayerConnectionState,
    world_save: Option<&WorldSave>,
) -> Option<PathBuf> {
    if !multiplayer_connection.uses_local_save_data() {
        return None;
    }

    let world_root = world_save.map(|save| save.root.clone())?;
    Some(world_root.join(PLAYER_INVENTORY_SAVE_FILE_NAME))
}

/// Reads player data from file for the `graphic::components::inventory_persistence` module.
fn read_player_data_from_file(path: &Path) -> Result<Option<PlayerPersistedData>, std::io::Error> {
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(path)?;
    decode_inventory_blob(&bytes).map(Some).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid inventory data: {error}"),
        )
    })
}

/// Writes player data to file for the `graphic::components::inventory_persistence` module.
fn write_player_data_to_file(
    path: &Path,
    inventory: &PlayerInventory,
    position: Option<[f32; 3]>,
    yaw_pitch: Option<[f32; 2]>,
) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let blob = encode_inventory_blob(inventory, position, yaw_pitch);
    let mut temp_path = path.to_path_buf();
    let temp_name = match path.file_name().and_then(|name| name.to_str()) {
        Some(file_name) if !file_name.is_empty() => format!("{file_name}.tmp"),
        _ => "save.data.tmp".to_string(),
    };
    temp_path.set_file_name(temp_name);

    fs::write(&temp_path, blob)?;
    fs::rename(temp_path, path)?;
    Ok(())
}

/// Encodes inventory blob for the `graphic::components::inventory_persistence` module.
fn encode_inventory_blob(
    inventory: &PlayerInventory,
    position: Option<[f32; 3]>,
    yaw_pitch: Option<[f32; 2]>,
) -> Vec<u8> {
    let has_position = position.is_some();
    let has_yaw_pitch = yaw_pitch.is_some();
    let mut out = Vec::with_capacity(8 + PLAYER_INVENTORY_SLOTS * 4 + 20);
    out.extend_from_slice(&PLAYER_INVENTORY_SAVE_MAGIC);
    out.push(PLAYER_INVENTORY_SAVE_VERSION);
    out.extend_from_slice(&(PLAYER_INVENTORY_SLOTS as u16).to_le_bytes());
    let mut flags = 0u8;
    if has_position {
        flags |= PLAYER_SAVE_FLAG_HAS_POSITION;
    }
    if has_yaw_pitch {
        flags |= PLAYER_SAVE_FLAG_HAS_YAW_PITCH;
    }
    out.push(flags);

    for slot in inventory.slots {
        out.extend_from_slice(&slot.item_id.to_le_bytes());
        out.extend_from_slice(&slot.count.to_le_bytes());
    }

    if let Some([x, y, z]) = position {
        out.extend_from_slice(&x.to_le_bytes());
        out.extend_from_slice(&y.to_le_bytes());
        out.extend_from_slice(&z.to_le_bytes());
    }
    if let Some([yaw, pitch]) = yaw_pitch {
        out.extend_from_slice(&yaw.to_le_bytes());
        out.extend_from_slice(&pitch.to_le_bytes());
    }

    out
}

/// Decodes inventory blob for the `graphic::components::inventory_persistence` module.
fn decode_inventory_blob(blob: &[u8]) -> Result<PlayerPersistedData, &'static str> {
    if blob.len() < 7 {
        return Err("file too small");
    }

    if blob[0..4] != PLAYER_INVENTORY_SAVE_MAGIC {
        return Err("magic mismatch");
    }

    let version = blob[4];
    if version != PLAYER_INVENTORY_SAVE_VERSION
        && version != PLAYER_INVENTORY_SAVE_VERSION_POSITION
        && version != PLAYER_INVENTORY_SAVE_VERSION_LEGACY
    {
        return Err("unsupported version");
    }

    let slot_count = u16::from_le_bytes([blob[5], blob[6]]) as usize;
    let header_len = if version == PLAYER_INVENTORY_SAVE_VERSION
        || version == PLAYER_INVENTORY_SAVE_VERSION_POSITION
    {
        8usize
    } else {
        7usize
    };
    let expected_len = header_len + slot_count.saturating_mul(4);
    if blob.len() < expected_len {
        return Err("truncated payload");
    }

    let mut inventory = PlayerInventory::default();
    let copy_count = slot_count.min(PLAYER_INVENTORY_SLOTS);

    let mut offset = header_len;
    for index in 0..copy_count {
        let item_id = u16::from_le_bytes([blob[offset], blob[offset + 1]]);
        let count = u16::from_le_bytes([blob[offset + 2], blob[offset + 3]]);
        offset += 4;

        if item_id == 0 || count == 0 {
            inventory.slots[index] = InventorySlot::default();
            continue;
        }

        inventory.slots[index] = InventorySlot {
            item_id,
            count: count.min(PLAYER_INVENTORY_STACK_MAX),
        };
    }

    let mut position = None;
    let mut yaw_pitch = None;
    if version == PLAYER_INVENTORY_SAVE_VERSION || version == PLAYER_INVENTORY_SAVE_VERSION_POSITION
    {
        let flags = blob[7];
        if (flags & PLAYER_SAVE_FLAG_HAS_POSITION) != 0 {
            if blob.len() < offset + 12 {
                return Err("truncated player position");
            }
            let x = f32::from_le_bytes([
                blob[offset],
                blob[offset + 1],
                blob[offset + 2],
                blob[offset + 3],
            ]);
            let y = f32::from_le_bytes([
                blob[offset + 4],
                blob[offset + 5],
                blob[offset + 6],
                blob[offset + 7],
            ]);
            let z = f32::from_le_bytes([
                blob[offset + 8],
                blob[offset + 9],
                blob[offset + 10],
                blob[offset + 11],
            ]);
            position = Some([x, y, z]);
            offset += 12;
        }
        if version == PLAYER_INVENTORY_SAVE_VERSION
            && (flags & PLAYER_SAVE_FLAG_HAS_YAW_PITCH) != 0
        {
            if blob.len() < offset + 8 {
                return Err("truncated yaw/pitch");
            }
            let yaw = f32::from_le_bytes([
                blob[offset],
                blob[offset + 1],
                blob[offset + 2],
                blob[offset + 3],
            ]);
            let pitch = f32::from_le_bytes([
                blob[offset + 4],
                blob[offset + 5],
                blob[offset + 6],
                blob[offset + 7],
            ]);
            yaw_pitch = Some([yaw, pitch]);
        }
    }

    Ok(PlayerPersistedData {
        inventory,
        position,
        yaw_pitch,
    })
}

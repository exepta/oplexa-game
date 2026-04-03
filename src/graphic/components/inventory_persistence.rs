const PLAYER_INVENTORY_SAVE_FILE_NAME: &str = "save.data";
const PLAYER_INVENTORY_MP_SAVE_PREFIX: &str = "save-";
const PLAYER_INVENTORY_MP_SAVE_SUFFIX: &str = ".data";
const PLAYER_INVENTORY_SAVE_MAGIC: [u8; 4] = *b"PINV";
const PLAYER_INVENTORY_SAVE_VERSION: u8 = 1;

/// Represents active inventory save path used by the `graphic::components::inventory_persistence` module.
#[derive(Resource, Debug, Default, Clone)]
struct ActiveInventorySavePath {
    path: Option<PathBuf>,
}

/// Clears inventory context when entering screen for the `graphic::components::inventory_persistence` module.
fn clear_inventory_context_when_entering_screen(
    mut inventory: ResMut<PlayerInventory>,
    mut active_save_path: ResMut<ActiveInventorySavePath>,
) {
    *inventory = PlayerInventory::default();
    active_save_path.path = None;
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
        *inventory = PlayerInventory::default();
        return;
    };

    match read_inventory_from_file(path.as_path()) {
        Ok(Some(loaded)) => {
            *inventory = loaded;
        }
        Ok(None) => {
            *inventory = PlayerInventory::default();
        }
        Err(error) => {
            warn!(
                "Failed to load player inventory from {:?}: {}. Falling back to empty inventory.",
                path, error
            );
            *inventory = PlayerInventory::default();
        }
    }
}

/// Persists inventory on change for the `graphic::components::inventory_persistence` module.
fn persist_inventory_on_change(
    inventory: Res<PlayerInventory>,
    active_save_path: Res<ActiveInventorySavePath>,
) {
    if !inventory.is_changed() {
        return;
    }

    let Some(path) = active_save_path.path.as_ref() else {
        return;
    };

    if let Err(error) = write_inventory_to_file(path.as_path(), &inventory) {
        warn!("Failed to save player inventory to {:?}: {}", path, error);
    }
}

/// Persists inventory on world exit for the `graphic::components::inventory_persistence` module.
fn persist_inventory_on_world_exit(
    inventory: Res<PlayerInventory>,
    active_save_path: Res<ActiveInventorySavePath>,
) {
    let Some(path) = active_save_path.path.as_ref() else {
        return;
    };

    if let Err(error) = write_inventory_to_file(path.as_path(), &inventory) {
        warn!(
            "Failed to save player inventory during world exit to {:?}: {}",
            path, error
        );
    }
}

/// Clears inventory after world exit for the `graphic::components::inventory_persistence` module.
fn clear_inventory_after_world_exit(
    mut inventory: ResMut<PlayerInventory>,
    mut active_save_path: ResMut<ActiveInventorySavePath>,
) {
    *inventory = PlayerInventory::default();
    active_save_path.path = None;
}

/// Resolves inventory save path for the `graphic::components::inventory_persistence` module.
fn resolve_inventory_save_path(
    multiplayer_connection: &MultiplayerConnectionState,
    world_save: Option<&WorldSave>,
) -> Option<PathBuf> {
    if multiplayer_connection.uses_local_save_data() {
        let world_root = world_save.map(|save| save.root.clone())?;
        return Some(world_root.join(PLAYER_INVENTORY_SAVE_FILE_NAME));
    }

    let world_name = sanitize_world_name(
        multiplayer_connection
            .world_name
            .as_deref()
            .unwrap_or_default(),
    );
    let player_uuid = sanitize_uuid(
        multiplayer_connection
            .client_uuid
            .as_deref()
            .unwrap_or_default(),
    );

    if player_uuid.is_empty() {
        return None;
    }

    Some(
        PathBuf::from("worlds")
            .join(world_name)
            .join("data")
            .join(format!(
                "{PLAYER_INVENTORY_MP_SAVE_PREFIX}{player_uuid}{PLAYER_INVENTORY_MP_SAVE_SUFFIX}"
            )),
    )
}

/// Sanitizes world name for filesystem usage in the `graphic::components::inventory_persistence` module.
fn sanitize_world_name(raw: &str) -> String {
    let sanitized = raw
        .trim()
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect::<String>();

    if sanitized.trim().is_empty() {
        "world".to_string()
    } else {
        sanitized
    }
}

/// Sanitizes uuid for filesystem usage in the `graphic::components::inventory_persistence` module.
fn sanitize_uuid(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|ch| ch.is_ascii_hexdigit() || *ch == '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Reads inventory from file for the `graphic::components::inventory_persistence` module.
fn read_inventory_from_file(path: &Path) -> Result<Option<PlayerInventory>, std::io::Error> {
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

/// Writes inventory to file for the `graphic::components::inventory_persistence` module.
fn write_inventory_to_file(path: &Path, inventory: &PlayerInventory) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let blob = encode_inventory_blob(inventory);
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
fn encode_inventory_blob(inventory: &PlayerInventory) -> Vec<u8> {
    let mut out = Vec::with_capacity(7 + PLAYER_INVENTORY_SLOTS * 4);
    out.extend_from_slice(&PLAYER_INVENTORY_SAVE_MAGIC);
    out.push(PLAYER_INVENTORY_SAVE_VERSION);
    out.extend_from_slice(&(PLAYER_INVENTORY_SLOTS as u16).to_le_bytes());

    for slot in inventory.slots {
        out.extend_from_slice(&slot.item_id.to_le_bytes());
        out.extend_from_slice(&slot.count.to_le_bytes());
    }

    out
}

/// Decodes inventory blob for the `graphic::components::inventory_persistence` module.
fn decode_inventory_blob(blob: &[u8]) -> Result<PlayerInventory, &'static str> {
    if blob.len() < 7 {
        return Err("file too small");
    }

    if blob[0..4] != PLAYER_INVENTORY_SAVE_MAGIC {
        return Err("magic mismatch");
    }

    let version = blob[4];
    if version != PLAYER_INVENTORY_SAVE_VERSION {
        return Err("unsupported version");
    }

    let slot_count = u16::from_le_bytes([blob[5], blob[6]]) as usize;
    let expected_len = 7 + slot_count.saturating_mul(4);
    if blob.len() < expected_len {
        return Err("truncated payload");
    }

    let mut inventory = PlayerInventory::default();
    let copy_count = slot_count.min(PLAYER_INVENTORY_SLOTS);

    let mut offset = 7usize;
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

    Ok(inventory)
}

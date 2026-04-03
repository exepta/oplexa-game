# Oplexa Content & Command Guide

This document explains how to extend the game with:

1. Chat commands and registration
2. Recipe JSON files
3. Custom recipe systems
4. Item and block JSON files
5. Biome JSON files

All paths and examples match the current project layout.

## 1. Create and register commands

### 1.1 Register command metadata (name, aliases, help, autocomplete)

Commands are declared in:

- `api/src/core/commands/registry.rs`

Add a new descriptor in `default_chat_command_registry()`:

```rust
registry.register(CommandDescriptor::new(
    "time",
    ["t"],
    "/time <day|night>",
    "Sets world time.",
));
```

What this gives you:

- visible in `/help`
- autocomplete in chat (e.g. `/ti` -> `/time`)
- alias support (`/t`)

### 1.2 Implement execution on server (multiplayer)

Server execution is currently handled in:

- `server/src/services/chat.rs` in `handle_chat_messages()`

The parser (`/cmd args`) is shared and already used. Add a new match arm:

```rust
match descriptor.name.as_str() {
    "time" => {
        let Some(value) = command.args.first() else {
            send_system_to_single(
                &mut multi_sender,
                *server,
                &q_remote_id,
                entity,
                SystemMessageLevel::Warn,
                "Usage: /time <day|night>".to_string(),
            );
            continue;
        };

        // TODO: apply gameplay logic
        send_system_to_single(
            &mut multi_sender,
            *server,
            &q_remote_id,
            entity,
            SystemMessageLevel::Info,
            format!("Time set to {}.", value),
        );
    }
    // ...
}
```

### 1.3 Optional: implement local/offline behavior

Offline command execution is in:

- `src/client/mod.rs` in `handle_chat_submit_requests()`

If a command should also work without server, add the same `match` arm there.

### 1.4 Optional: dedicated server console commands

Console command parsing is in:

- `server/src/services/chat.rs` in `run_console_command()`

Add your command there if it should work from terminal (`stdin`) too.

### 1.5 Important architectural note

`CommandRegistry` currently stores command metadata (lookup/help/autocomplete).
Execution is still hardcoded in `match` blocks on server/client.

For plugin-style commands in future, add an executor registry (function map) and dispatch by command key.

## 2. Create recipe JSON files

### 2.1 Location and loading

- Place recipe files in `assets/recipes/` (recursive loading is supported).
- Loader: `api/src/core/inventory/recipe/loader.rs`

### 2.2 JSON format

```json
{
  "type": "crafting_shaped",
  "crafting": [
    {
      "type": "oplexa:hand_crafted",
      "data": {
        "craft": {
          "0": { "item": "oplexa:log_block", "count": 1 },
          "1": { "item": "oplexa:log_block", "count": 1 }
        }
      }
    }
  ],
  "result": {
    "item": "oplexa:stick",
    "count": 12
  }
}
```

Field notes:

- `type`: free-form recipe kind label (informational).
- `crafting[].type`: namespaced recipe matcher key (`provider:key`), for example `oplexa:hand_crafted`.
- `crafting[].data`: matcher-specific payload.
- `result.item`: must exist in item registry.
- `result.count`: optional, defaults to `1`.

### 2.3 Hand-crafted matcher specifics (`oplexa:hand_crafted`)

Defined in:

- `api/src/core/inventory/recipe/hand_crafted.rs`

Current constraints:

- input slots are indexed as strings (`"0"`, `"1"`)
- only first two hand-craft slots are considered
- slots not listed in `craft` must be empty

If `crafting[].type` is unknown, recipe is loaded but stays inactive until a handler is registered.

## 3. Create custom recipe systems

There are two layers:

1. Recipe type matcher (validate required inputs)
2. Execution flow (when/how crafting is triggered)

### 3.1 Register a custom matcher type

Core APIs:

- `api/src/core/inventory/recipe/registry.rs`
- `api/src/core/inventory/recipe/types.rs`

Required matcher signature:

```rust
fn(
    data: &serde_json::Value,
    input_slots: &[InventorySlot],
    item_registry: &ItemRegistry
) -> Option<Vec<RecipeInputRequirement>>
```

Register it with a namespaced key:

```rust
let recipe_type = NamespacedKey::parse("myplugin:anvil").unwrap();
recipe_type_registry.register_handler(
    recipe_type,
    RecipeTypeHandler { matcher: my_matcher_fn },
);
```

### 3.2 Ensure registration happens before recipe loading

Recipes are loaded in:

- `src/logic/registry/block_registry.rs` (`start_block_registry`)

Current order is:

1. `RecipeTypeRegistry::with_defaults()`
2. `load_recipe_registry(...)`

If you add custom types, register them on `recipe_type_registry` before calling `load_recipe_registry(...)`.

### 3.3 Add JSON recipes for your new type

Example:

```json
{
  "type": "anvil_upgrade",
  "crafting": [
    {
      "type": "myplugin:anvil",
      "data": {
        "base": "oplexa:wood_pickaxe",
        "material": "oplexa:stone_block"
      }
    }
  ],
  "result": {
    "item": "oplexa:stone_pickaxe",
    "count": 1
  }
}
```

### 3.4 Add/extend execution system

Current runtime crafting flow is hand-crafted only:

- `api/src/handlers/recipe/mod.rs`
- plugin registration in `src/logic/events/mod.rs`

For a fully custom system (for example anvil/furnace/machine), create a dedicated handler plugin:

- new request message/event
- lookup in `RecipeRegistry` + `RecipeTypeRegistry`
- consume inputs and insert result into inventory/container

## 4. Create item and block JSON files

## 4.1 Blocks

### 4.1.1 Location

- Block defs: `assets/blocks/*.json`
- Texture atlas metadata per block set: `assets/textures/blocks/<set>/data.json`
- Atlas image file referenced by `data.json`

### 4.1.2 Block JSON format

```json
{
  "name": "stone_block",
  "texture_dir": "textures/blocks/stone",
  "texture": {
    "all": "stone"
  },
  "stats": {
    "hardness": 4.5,
    "blast_resistance": 3.5,
    "level": "1",
    "opaque": true,
    "fluid": false,
    "emissive": 0.0
  }
}
```

Texture face keys:

- direct: `top`, `bottom`, `north`, `east`, `south`, `west`
- grouped: `all`, `vertical`, `horizontal`
- legacy alias: `nord` (fallback for `north`)

`texture_dir` is optional; if omitted it is inferred from block name:

- `log_block` -> `textures/blocks/log`

### 4.1.3 Atlas `data.json` format

```json
{
  "image": "stone.png",
  "tile_size": 128,
  "columns": 1,
  "rows": 1,
  "tiles": {
    "stone": [0, 0]
  }
}
```

The `texture` names in block JSON must exist in `tiles`.

## 4.2 Items

### 4.2.1 Location

- Item defs: `assets/items/*.json`

### 4.2.2 Item JSON format (flat item)

```json
{
  "localized_name": "oplexa:stick",
  "name": "Stick",
  "max_stack_size": -1,
  "category": "material",
  "block_item": false,
  "placeable": false,
  "tags": ["wood", "crafting_material"],
  "rarity": "common",
  "render": {
    "type": "flat",
    "texture": "textures/items/stick.png"
  },
  "world_drop": {
    "pickupable": true
  }
}
```

### 4.2.3 Item JSON format (block item / placeable)

```json
{
  "localized_name": "oplexa:log_block",
  "name": "Log Block",
  "max_stack_size": -1,
  "category": "block",
  "block_item": true,
  "placeable": true,
  "block": "log_block",
  "render": {
    "type": "block",
    "block": "log_block",
    "projection": "isometric"
  }
}
```

Notes:

- `localized_name` supports namespaced format (`provider:key`); plain keys default to provider `oplexa`.
- `max_stack_size <= 0` falls back to default stack size.
- If `block_item=true`, mapping to a block can come from:
  - `block`
  - `render.block`
  - inferred from key (e.g. `stone` -> `stone_block`).
- Missing explicit block-items are auto-generated for all blocks.

## 5. Create biome JSON files

### 5.1 Location and loading

- Place biome files in `assets/biomes/*.json`
- Loaded by client and server biome registries

### 5.2 Biome JSON format

```json
{
  "localized_name": "plains",
  "name": "Plains",
  "stand_alone": true,
  "subs": ["mountains"],
  "rarity": 0.15,
  "sizes": ["Large", "Huge"],
  "surface": {
    "top": ["grass_block"],
    "bottom": ["dirt_block"],
    "sea_floor": ["sand_block"],
    "upper_zero": ["stone_block"],
    "under_zero": ["deep_stone_block"]
  },
  "settings": {
    "height_offset": 8.0,
    "land_amp": 18.0,
    "land_freq": 0.0091
  },
  "generation": {
    "rivers": true,
    "river_chance": 0.5,
    "river_size_between": "24:24"
  }
}
```

Field notes:

- `localized_name`: internal biome key.
- `name`: display/debug name.
- `stand_alone`: can spawn as own biome site.
- `subs`: optional list of sub-biomes that may appear inside this biome.
- `rarity`: weighted selection probability.
- `sizes`: any of `VeryTiny`, `Tiny`, `Small`, `Medium`, `Large`, `Huge`, `Giant`, `Ocean`.
- `surface.*`: block ids by localized block name.
- `generation.river_size_between`: string `"min:max"`.

### 5.3 Validation checklist

Before running, verify:

- every referenced block exists in `assets/blocks`
- every referenced item exists in `assets/items`
- every recipe result item exists
- every recipe `crafting[].type` has a registered matcher handler
- biome `sizes` use valid enum names

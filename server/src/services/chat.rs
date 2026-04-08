use crate::state::{ServerRuntimeConfig, ServerState};
use api::core::commands::{
    CommandRegistry, CommandSender, EntitySender, GameModeKind, SystemMessageLevel, SystemSender,
    default_chat_command_registry, parse_chat_command,
};
use api::core::network::protocols::{
    ClientChatMessage, OrderedReliable, PlayerSnapshot, ServerChatMessage, ServerGameModeChanged,
    ServerTeleport, UnorderedReliable, UnorderedUnreliable,
};
use api::core::world::biome::func::locate_biome_chunk_by_localized_name;
use api::core::world::chunk_dimension::{CX, CZ, world_to_chunk_xz};
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use std::io::BufRead;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Instant;

const LOCATE_MAX_RADIUS_BLOCKS_CAP: i32 = 1000;

/// Converts locate radius from blocks into chunks and clamps it to safe bounds.
fn locate_radius_chunks_from_blocks(radius_blocks: i32) -> i32 {
    let clamped_blocks = radius_blocks.clamp(1, LOCATE_MAX_RADIUS_BLOCKS_CAP);
    let chunk_span = (CX as i32).max(CZ as i32);
    (clamped_blocks + (chunk_span - 1)) / chunk_span
}

/// Shared server command registry resource.
#[derive(Resource, Clone, Debug)]
pub struct ServerCommandRegistry(pub CommandRegistry);

impl Default for ServerCommandRegistry {
    /// Runs the `default` routine for default in the `services::chat` module.
    fn default() -> Self {
        Self(default_chat_command_registry())
    }
}

/// Console input receiver used for dedicated server commands.
#[derive(Resource)]
pub struct ServerConsoleInput(pub Mutex<mpsc::Receiver<String>>);

/// Creates the shared console input resource and starts a blocking stdin reader thread.
pub fn create_server_console_input() -> ServerConsoleInput {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();

        loop {
            let mut line = String::new();
            match handle.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    if tx.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    ServerConsoleInput(Mutex::new(rx))
}

/// Handles incoming client chat lines and command execution.
pub fn handle_chat_messages(
    mut q: Query<(Entity, &mut MessageReceiver<ClientChatMessage>), With<ClientOf>>,
    q_remote_id: Query<&RemoteId>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
    registry: Res<ServerCommandRegistry>,
    runtime_config: Res<ServerRuntimeConfig>,
) {
    let locate_max_radius_chunks =
        locate_radius_chunks_from_blocks(runtime_config.locate_search_radius);

    let mut buffered = Vec::<(Entity, ClientChatMessage)>::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            buffered.push((entity, message));
        }
    }

    for (entity, payload) in buffered {
        let text = payload.text.trim();
        if text.is_empty() {
            continue;
        }

        let (player_id, player_name, player_translation) = {
            let Some(player) = state.players.get_mut(&entity) else {
                continue;
            };
            player.last_seen = Instant::now();
            (
                player.player_id,
                player.username.clone(),
                player.translation,
            )
        };

        if let Some(command) = parse_chat_command(text) {
            let Some(descriptor) = registry.0.find(command.name.as_str()) else {
                send_system_to_single(
                    &mut multi_sender,
                    *server,
                    &q_remote_id,
                    entity,
                    SystemMessageLevel::Warn,
                    format!("Unknown command '/{}'. Use /help.", command.name),
                );
                continue;
            };

            match descriptor.name.as_str() {
                "help" => {
                    let names = registry
                        .0
                        .sorted_descriptors()
                        .into_iter()
                        .map(|entry| format!("/{}", entry.name))
                        .collect::<Vec<_>>()
                        .join(", ");

                    send_system_to_single(
                        &mut multi_sender,
                        *server,
                        &q_remote_id,
                        entity,
                        SystemMessageLevel::Info,
                        format!("Available commands: {}", names),
                    );
                }
                "gamemode" => {
                    let Some(raw_mode) = command.args.first() else {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            "Usage: /gamemode <survival|creative|spectator>".to_string(),
                        );
                        continue;
                    };

                    let Some(mode) = GameModeKind::parse(raw_mode) else {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            format!(
                                "Unknown game mode '{}'. Use survival, creative, or spectator.",
                                raw_mode
                            ),
                        );
                        continue;
                    };

                    let Some(player) = state.players.get_mut(&entity) else {
                        continue;
                    };
                    player.game_mode = mode;

                    let _ = multi_sender.send::<_, OrderedReliable>(
                        &ServerGameModeChanged::new(player_id, mode),
                        *server,
                        &NetworkTarget::All,
                    );
                    send_system_to_single(
                        &mut multi_sender,
                        *server,
                        &q_remote_id,
                        entity,
                        SystemMessageLevel::Info,
                        format!("Game mode set to {}.", mode.as_str()),
                    );
                    let _ = multi_sender.send::<_, UnorderedReliable>(
                        &ServerChatMessage::new(
                            CommandSender::System(SystemSender::Server {
                                level: SystemMessageLevel::Debug,
                            }),
                            format!("Player {} changed mode to {}.", player_name, mode.as_str()),
                        ),
                        *server,
                        &NetworkTarget::All,
                    );
                }
                "locate" => {
                    let Some(raw_type) = command.args.first() else {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            "Usage: /locate <biome> <name:key>".to_string(),
                        );
                        continue;
                    };
                    let Some(raw_target) = command.args.get(1) else {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            "Usage: /locate <biome> <name:key>".to_string(),
                        );
                        continue;
                    };

                    if !raw_type.eq_ignore_ascii_case("biome") {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            "Only type 'biome' is supported right now.".to_string(),
                        );
                        continue;
                    }

                    let target = raw_target.trim();
                    if !target.contains(':') {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            "Biome key must be in format 'name:key'.".to_string(),
                        );
                        continue;
                    }

                    if state.biome_registry.get_by_localized_name(target).is_none() {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            format!("Unknown biome '{}'.", target),
                        );
                        continue;
                    }

                    let world_x = player_translation[0].floor() as i32;
                    let world_z = player_translation[2].floor() as i32;
                    let (origin_chunk, _) = world_to_chunk_xz(world_x, world_z);

                    let locate_result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            locate_biome_chunk_by_localized_name(
                                &state.biome_registry,
                                state.world_gen_config.seed,
                                origin_chunk,
                                target,
                                locate_max_radius_chunks,
                            )
                        }));
                    let Some(found_chunk) = (match locate_result {
                        Ok(found) => found,
                        Err(_) => {
                            send_system_to_single(
                                &mut multi_sender,
                                *server,
                                &q_remote_id,
                                entity,
                                SystemMessageLevel::Warn,
                                "Locate failed due to an internal error.".to_string(),
                            );
                            continue;
                        }
                    }) else {
                        send_system_to_single(
                            &mut multi_sender,
                            *server,
                            &q_remote_id,
                            entity,
                            SystemMessageLevel::Warn,
                            format!("Biome '{}' not found nearby.", target),
                        );
                        continue;
                    };

                    let found_x = found_chunk.x * CX as i32 + (CX as i32 / 2);
                    let found_z = found_chunk.y * CZ as i32 + (CZ as i32 / 2);
                    send_system_to_single(
                        &mut multi_sender,
                        *server,
                        &q_remote_id,
                        entity,
                        SystemMessageLevel::Info,
                        format!("found: [{}, {}]", found_x, found_z),
                    );
                }
                "tp" => {
                    let result = execute_tp_command(
                        entity,
                        player_name.as_str(),
                        &command.args,
                        &mut state,
                        &q_remote_id,
                        &mut multi_sender,
                        *server,
                    );
                    match result {
                        Ok(message) => {
                            send_system_to_single(
                                &mut multi_sender,
                                *server,
                                &q_remote_id,
                                entity,
                                SystemMessageLevel::Info,
                                message,
                            );
                        }
                        Err(message) => {
                            send_system_to_single(
                                &mut multi_sender,
                                *server,
                                &q_remote_id,
                                entity,
                                SystemMessageLevel::Warn,
                                message,
                            );
                        }
                    }
                }
                _ => {
                    send_system_to_single(
                        &mut multi_sender,
                        *server,
                        &q_remote_id,
                        entity,
                        SystemMessageLevel::Warn,
                        format!("Command '/{}' is not executable yet.", descriptor.name),
                    );
                }
            }

            continue;
        }

        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerChatMessage::new(
                CommandSender::Entity(EntitySender::Player {
                    player_id,
                    player_name,
                }),
                text.to_string(),
            ),
            *server,
            &NetworkTarget::All,
        );
    }
}

fn send_system_to_single(
    multi_sender: &mut ServerMultiMessageSender,
    server: &Server,
    q_remote_id: &Query<&RemoteId>,
    entity: Entity,
    level: SystemMessageLevel,
    message: String,
) {
    let peer_id = q_remote_id
        .get(entity)
        .map(|remote| remote.0)
        .unwrap_or(PeerId::Entity(entity.to_bits()));
    let _ = multi_sender.send::<_, UnorderedReliable>(
        &ServerChatMessage::new(
            CommandSender::System(SystemSender::Server { level }),
            message,
        ),
        server,
        &NetworkTarget::Single(peer_id),
    );
}

fn execute_tp_command(
    caller_entity: Entity,
    caller_name: &str,
    args: &[String],
    state: &mut ServerState,
    q_remote_id: &Query<&RemoteId>,
    multi_sender: &mut ServerMultiMessageSender,
    server: &Server,
) -> Result<String, String> {
    if args.is_empty() {
        return Err("Usage: /tp <player>|<x y z>|<player player>|<player x y z>".to_string());
    }

    match args.len() {
        1 => {
            let target_entity = find_player_entity_by_name(state, &args[0])
                .ok_or_else(|| format!("Player '{}' not found.", args[0].trim()))?;
            let target = state
                .players
                .get(&target_entity)
                .map(|player| player.translation)
                .ok_or_else(|| "Teleport target is unavailable.".to_string())?;
            teleport_player_to(
                state,
                q_remote_id,
                multi_sender,
                server,
                caller_entity,
                target,
                Some(caller_name),
            )?;
            Ok(format!(
                "Teleported to {}.",
                state
                    .players
                    .get(&target_entity)
                    .map(|player| player.username.as_str())
                    .unwrap_or("target")
            ))
        }
        2 => {
            let from_entity = find_player_entity_by_name(state, &args[0])
                .ok_or_else(|| format!("Player '{}' not found.", args[0].trim()))?;
            let to_entity = find_player_entity_by_name(state, &args[1])
                .ok_or_else(|| format!("Player '{}' not found.", args[1].trim()))?;
            let to_translation = state
                .players
                .get(&to_entity)
                .map(|player| player.translation)
                .ok_or_else(|| "Teleport target is unavailable.".to_string())?;

            let from_name = state
                .players
                .get(&from_entity)
                .map(|player| player.username.clone())
                .unwrap_or_else(|| args[0].trim().to_string());
            let to_name = state
                .players
                .get(&to_entity)
                .map(|player| player.username.clone())
                .unwrap_or_else(|| args[1].trim().to_string());

            teleport_player_to(
                state,
                q_remote_id,
                multi_sender,
                server,
                from_entity,
                to_translation,
                Some(caller_name),
            )?;
            Ok(format!("Teleported {} to {}.", from_name, to_name))
        }
        3 => {
            let target = parse_xyz_args(args)?;
            teleport_player_to(
                state,
                q_remote_id,
                multi_sender,
                server,
                caller_entity,
                target,
                Some(caller_name),
            )?;
            Ok(format!(
                "Teleported to [{:.2}, {:.2}, {:.2}].",
                target[0], target[1], target[2]
            ))
        }
        4 => {
            let from_entity = find_player_entity_by_name(state, &args[0])
                .ok_or_else(|| format!("Player '{}' not found.", args[0].trim()))?;
            let target = parse_xyz_slice(&args[1], &args[2], &args[3])?;
            let from_name = state
                .players
                .get(&from_entity)
                .map(|player| player.username.clone())
                .unwrap_or_else(|| args[0].trim().to_string());
            teleport_player_to(
                state,
                q_remote_id,
                multi_sender,
                server,
                from_entity,
                target,
                Some(caller_name),
            )?;
            Ok(format!(
                "Teleported {} to [{:.2}, {:.2}, {:.2}].",
                from_name, target[0], target[1], target[2]
            ))
        }
        _ => Err("Usage: /tp <player>|<x y z>|<player player>|<player x y z>".to_string()),
    }
}

fn find_player_entity_by_name(state: &ServerState, raw_name: &str) -> Option<Entity> {
    let needle = raw_name.trim();
    if needle.is_empty() {
        return None;
    }
    state.players.iter().find_map(|(entity, player)| {
        player
            .username
            .eq_ignore_ascii_case(needle)
            .then_some(*entity)
    })
}

fn parse_xyz_args(args: &[String]) -> Result<[f32; 3], String> {
    if args.len() < 3 {
        return Err("Usage: /tp <x> <y> <z>".to_string());
    }
    parse_xyz_slice(&args[0], &args[1], &args[2])
}

fn parse_xyz_slice(x: &str, y: &str, z: &str) -> Result<[f32; 3], String> {
    let x = x
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid x coordinate '{}'.", x.trim()))?;
    let y = y
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid y coordinate '{}'.", y.trim()))?;
    let z = z
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Invalid z coordinate '{}'.", z.trim()))?;
    Ok([x, y, z])
}

fn teleport_player_to(
    state: &mut ServerState,
    q_remote_id: &Query<&RemoteId>,
    multi_sender: &mut ServerMultiMessageSender,
    server: &Server,
    target_entity: Entity,
    target_translation: [f32; 3],
    teleported_by: Option<&str>,
) -> Result<(), String> {
    let (player_id, yaw, pitch, player_name) = {
        let Some(player) = state.players.get_mut(&target_entity) else {
            return Err("Player is not available.".to_string());
        };
        player.translation = target_translation;
        (
            player.player_id,
            player.yaw,
            player.pitch,
            player.username.clone(),
        )
    };

    let peer_id = q_remote_id
        .get(target_entity)
        .map(|remote| remote.0)
        .unwrap_or(PeerId::Entity(target_entity.to_bits()));

    let _ = multi_sender.send::<_, UnorderedReliable>(
        &ServerTeleport::new(player_id, target_translation),
        server,
        &NetworkTarget::Single(peer_id),
    );
    let _ = multi_sender.send::<_, UnorderedUnreliable>(
        &PlayerSnapshot::new(player_id, target_translation, yaw, pitch),
        server,
        &NetworkTarget::All,
    );

    if let Some(by) = teleported_by
        && !by.eq_ignore_ascii_case(player_name.as_str())
    {
        let _ = multi_sender.send::<_, UnorderedReliable>(
            &ServerChatMessage::new(
                CommandSender::System(SystemSender::Server {
                    level: SystemMessageLevel::Info,
                }),
                format!(
                    "You were teleported by {} to [{:.2}, {:.2}, {:.2}].",
                    by, target_translation[0], target_translation[1], target_translation[2]
                ),
            ),
            server,
            &NetworkTarget::Single(peer_id),
        );
    }

    Ok(())
}

/// Polls console input and applies supported server-side commands.
pub fn handle_console_commands(
    console_input: Res<ServerConsoleInput>,
    mut state: ResMut<ServerState>,
    mut multi_sender: ServerMultiMessageSender,
    server: Single<&Server>,
) {
    let mut pending_lines = Vec::<String>::new();
    if let Ok(receiver) = console_input.0.lock() {
        while let Ok(line) = receiver.try_recv() {
            pending_lines.push(line);
        }
    }

    for line in pending_lines {
        run_console_command(line.as_str(), &mut state, &mut multi_sender, *server);
    }
}

fn run_console_command(
    line: &str,
    state: &mut ServerState,
    multi_sender: &mut ServerMultiMessageSender,
    server: &Server,
) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }

    let body = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut parts = body.split_whitespace();
    let Some(command) = parts.next() else {
        return;
    };

    match command.to_ascii_lowercase().as_str() {
        "help" => {
            log::info!("Console commands: help, gamemode <mode> <player_name>");
        }
        "gamemode" | "gm" => {
            let Some(raw_mode) = parts.next() else {
                log::warn!("Usage: gamemode <survival|creative|spectator> <player_name>");
                return;
            };
            let Some(mode) = GameModeKind::parse(raw_mode) else {
                log::warn!(
                    "Unknown mode '{}'. Use survival, creative, or spectator.",
                    raw_mode
                );
                return;
            };

            let raw_player_name = parts.collect::<Vec<_>>().join(" ");
            if raw_player_name.trim().is_empty() {
                log::warn!("Usage: gamemode <survival|creative|spectator> <player_name>");
                return;
            }
            let target_name = normalize_console_player_name(raw_player_name.as_str());

            let mut changed = None::<(u64, String)>;
            for player in state.players.values_mut() {
                if player.username.eq_ignore_ascii_case(target_name.as_str()) {
                    player.game_mode = mode;
                    changed = Some((player.player_id, player.username.clone()));
                    break;
                }
            }

            let Some((player_id, player_name)) = changed else {
                log::warn!("Player '{}' not found.", target_name);
                return;
            };

            let _ = multi_sender.send::<_, OrderedReliable>(
                &ServerGameModeChanged::new(player_id, mode),
                server,
                &NetworkTarget::All,
            );
            let _ = multi_sender.send::<_, UnorderedReliable>(
                &ServerChatMessage::new(
                    CommandSender::System(SystemSender::Server {
                        level: SystemMessageLevel::Info,
                    }),
                    format!(
                        "Server changed game mode of {} to {}.",
                        player_name,
                        mode.as_str()
                    ),
                ),
                server,
                &NetworkTarget::All,
            );
            log::info!(
                "Console: changed game mode of '{}' to '{}'.",
                player_name,
                mode.as_str()
            );
        }
        unknown => {
            log::warn!("Unknown console command '{}'. Try: help", unknown);
        }
    }
}

fn normalize_console_player_name(input: &str) -> String {
    input
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim()
        .to_string()
}

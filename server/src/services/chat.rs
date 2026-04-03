use crate::state::ServerState;
use api::core::commands::{
    CommandRegistry, CommandSender, EntitySender, GameModeKind, SystemMessageLevel, SystemSender,
    default_chat_command_registry, parse_chat_command,
};
use api::core::network::protocols::{
    ClientChatMessage, OrderedReliable, ServerChatMessage, ServerGameModeChanged, UnorderedReliable,
};
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use std::io::BufRead;
use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Instant;

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
) {
    let mut buffered = Vec::<(Entity, ClientChatMessage)>::new();
    for (entity, mut receiver) in q.iter_mut() {
        for message in receiver.receive() {
            buffered.push((entity, message));
        }
    }

    for (entity, payload) in buffered {
        let Some(player) = state.players.get_mut(&entity) else {
            continue;
        };
        player.last_seen = Instant::now();

        let text = payload.text.trim();
        if text.is_empty() {
            continue;
        }

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

                    player.game_mode = mode;
                    let player_id = player.player_id;
                    let player_name = player.username.clone();

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
                    player_id: player.player_id,
                    player_name: player.username.clone(),
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

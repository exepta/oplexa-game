mod auth;
mod blocks;
mod chat;
mod chunks;
mod drops;
pub mod mobs;
mod players;

pub use auth::*;
pub use blocks::*;
pub use chat::*;
pub use chunks::*;
pub use drops::*;
pub use players::*;

use bevy::prelude::*;
use lightyear::prelude::*;

// ── Channel marker types ──────────────────────────────────────────────────────

/// Reliable and ordered: block interactions, drops
pub struct OrderedReliable;
/// Reliable but unordered: player join/leave, welcome, chunk data, chunk interest
pub struct UnorderedReliable;
/// Unreliable and unordered: position snapshots, keep-alives
pub struct UnorderedUnreliable;

// ── Protocol registration plugin ─────────────────────────────────────────────

/// Represents protocol plugin used by the `core::network::protocols` module.
pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    /// Builds this component for the `core::network::protocols` module.
    fn build(&self, app: &mut App) {
        // Channels
        app.add_channel::<OrderedReliable>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.add_channel::<UnorderedReliable>(ChannelSettings {
            mode: ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        app.add_channel::<UnorderedUnreliable>(ChannelSettings {
            mode: ChannelMode::UnorderedUnreliable,
            ..default()
        })
        .add_direction(NetworkDirection::Bidirectional);

        // Messages – auth
        app.register_message::<Auth>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ServerWelcome>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ServerAuthRejected>()
            .add_direction(NetworkDirection::ServerToClient);

        // Messages – players
        app.register_message::<PlayerJoined>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<PlayerLeft>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<PlayerMove>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ClientKeepAlive>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<PlayerSnapshot>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ServerTeleport>()
            .add_direction(NetworkDirection::ServerToClient);

        // Messages – chunks
        app.register_message::<ClientChunkInterest>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ServerChunkData>()
            .add_direction(NetworkDirection::ServerToClient);

        // Messages – blocks
        app.register_message::<ClientBlockBreak>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ClientBlockPlace>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ServerBlockBreak>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ServerBlockPlace>()
            .add_direction(NetworkDirection::ServerToClient);

        // Messages – drops
        app.register_message::<ServerDropSpawn>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ClientDropItem>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ClientDropPickup>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ServerDropPicked>()
            .add_direction(NetworkDirection::ServerToClient);

        // Messages - chat + commands
        app.register_message::<ClientChatMessage>()
            .add_direction(NetworkDirection::ClientToServer);
        app.register_message::<ServerChatMessage>()
            .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<ServerGameModeChanged>()
            .add_direction(NetworkDirection::ServerToClient);
    }
}

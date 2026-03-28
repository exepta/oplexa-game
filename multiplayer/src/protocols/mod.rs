mod auth;
mod blocks;
mod drops;
mod players;
pub mod mobs;

pub use auth::*;
pub use blocks::*;
pub use drops::*;
pub use players::*;

use naia_shared::{LinkConditionerConfig, Protocol};
use std::time::Duration;

pub fn protocol() -> Protocol {
    Protocol::builder()
        .tick_interval(Duration::from_millis(50))
        .link_condition(LinkConditionerConfig::good_condition())
        .add_default_channels()
        .add_message::<Auth>()
        .add_message::<ServerWelcome>()
        .add_message::<PlayerJoined>()
        .add_message::<PlayerLeft>()
        .add_message::<PlayerMove>()
        .add_message::<PlayerSnapshot>()
        .add_message::<ClientBlockBreak>()
        .add_message::<ClientBlockPlace>()
        .add_message::<ServerBlockBreak>()
        .add_message::<ServerBlockPlace>()
        .add_message::<ServerDropSpawn>()
        .add_message::<ClientDropItem>()
        .add_message::<ClientDropPickup>()
        .add_message::<ServerDropPicked>()
        .build()
}

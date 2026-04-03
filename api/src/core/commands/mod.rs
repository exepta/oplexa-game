//! Shared command primitives used by client and server side chat handling.
//!
//! This module intentionally keeps command concerns decoupled from gameplay systems:
//! - sender modeling (`sender`)
//! - gameplay modes referenced by commands (`mode`)
//! - parser and command-token model (`parsed`)
//! - registry and autocomplete helpers (`registry`)

pub mod mode;
pub mod parsed;
pub mod registry;
pub mod sender;

pub use mode::GameModeKind;
pub use parsed::{ParsedCommand, is_command_input, parse_chat_command};
pub use registry::{CommandDescriptor, CommandRegistry, default_chat_command_registry};
pub use sender::{CommandSender, EntitySender, SystemMessageLevel, SystemSender};

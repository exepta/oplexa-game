use crate::core::commands::{CommandSender, EntitySender, SystemSender};
use bevy::prelude::*;
use std::collections::VecDeque;

const DEFAULT_CHAT_HISTORY_LIMIT: usize = 140;

/// One chat line in the timeline.
#[derive(Clone, Debug)]
pub struct ChatLine {
    pub sender: CommandSender,
    pub message: String,
}

impl ChatLine {
    /// Creates a new chat line.
    pub fn new(sender: CommandSender, message: impl Into<String>) -> Self {
        Self {
            sender,
            message: message.into(),
        }
    }

    /// Formats this line for UI rendering.
    ///
    /// Player: `[name] > text`
    /// System: `[INFO] - text`
    pub fn formatted(&self) -> String {
        match &self.sender {
            CommandSender::Entity(sender) => match sender {
                EntitySender::Player { player_name, .. } => {
                    format!("[{}] > {}", player_name, self.message)
                }
                EntitySender::Other { display_name, .. } => {
                    format!("[{}] > {}", display_name, self.message)
                }
            },
            CommandSender::System(sender) => {
                let level = match sender {
                    SystemSender::Server { level } => level.as_tag(),
                    SystemSender::Plugin { level, .. } => level.as_tag(),
                };
                format!("[{}] - {}", level, self.message)
            }
        }
    }
}

/// Shared chat history resource consumed by UI and networking systems.
#[derive(Resource, Debug)]
pub struct ChatLog {
    lines: VecDeque<ChatLine>,
    max_lines: usize,
}

impl Default for ChatLog {
    /// Runs the `default` routine for default in the `core::chat` module.
    fn default() -> Self {
        Self {
            lines: VecDeque::with_capacity(DEFAULT_CHAT_HISTORY_LIMIT),
            max_lines: DEFAULT_CHAT_HISTORY_LIMIT,
        }
    }
}

impl ChatLog {
    /// Sets the maximum number of retained chat lines.
    pub fn set_max_lines(&mut self, max_lines: usize) {
        self.max_lines = max_lines.max(1);
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    /// Pushes one line while preserving the fixed maximum history.
    pub fn push(&mut self, line: ChatLine) {
        self.lines.push_back(line);
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    /// Clears the full chat timeline.
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Returns all lines in insertion order.
    pub fn lines(&self) -> &VecDeque<ChatLine> {
        &self.lines
    }
}

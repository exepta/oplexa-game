use serde::{Deserialize, Serialize};

/// Canonical gameplay modes used by command handlers and network sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameModeKind {
    Survival,
    Creative,
    Spectator,
}

impl GameModeKind {
    /// Returns the canonical lowercase command token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Spectator => "spectator",
        }
    }

    /// Parses a user supplied mode token.
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim().to_ascii_lowercase().as_str() {
            "survival" | "s" | "0" => Some(Self::Survival),
            "creative" | "c" | "1" => Some(Self::Creative),
            "spectator" | "sp" | "3" => Some(Self::Spectator),
            _ => None,
        }
    }
}

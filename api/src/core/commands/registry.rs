use std::collections::HashMap;

/// Static metadata for one chat command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub name: String,
    pub aliases: Vec<String>,
    pub usage: String,
    pub description: String,
}

impl CommandDescriptor {
    /// Creates a command descriptor with canonical lowercase keying.
    pub fn new(
        name: impl Into<String>,
        aliases: impl IntoIterator<Item = impl Into<String>>,
        usage: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into().trim().to_ascii_lowercase(),
            aliases: aliases
                .into_iter()
                .map(|alias| alias.into().trim().to_ascii_lowercase())
                .filter(|alias| !alias.is_empty())
                .collect(),
            usage: usage.into(),
            description: description.into(),
        }
    }
}

/// Extendable command registry with lookup and autocomplete support.
#[derive(Clone, Debug, Default)]
pub struct CommandRegistry {
    commands: Vec<CommandDescriptor>,
    by_name: HashMap<String, usize>,
}

impl CommandRegistry {
    /// Registers one command descriptor.
    ///
    /// Existing names/aliases are replaced to keep the most recent registration.
    pub fn register(&mut self, descriptor: CommandDescriptor) {
        let index = self.commands.len();
        self.by_name.insert(descriptor.name.clone(), index);
        for alias in &descriptor.aliases {
            self.by_name.insert(alias.clone(), index);
        }
        self.commands.push(descriptor);
    }

    /// Finds the descriptor for a command token (name or alias).
    pub fn find(&self, token: &str) -> Option<&CommandDescriptor> {
        let key = token.trim().to_ascii_lowercase();
        let idx = self.by_name.get(&key)?;
        self.commands.get(*idx)
    }

    /// Returns descriptors sorted by canonical command name.
    pub fn sorted_descriptors(&self) -> Vec<&CommandDescriptor> {
        let mut commands = self.commands.iter().collect::<Vec<_>>();
        commands.sort_by(|left, right| left.name.cmp(&right.name));
        commands
    }

    /// Returns slash-prefixed command suggestions for a user input prefix.
    ///
    /// `input` may include the leading slash. Suggestions are canonical names only.
    pub fn autocomplete(&self, input: &str) -> Vec<String> {
        let trimmed = input.trim();
        let prefix = trimmed
            .strip_prefix('/')
            .unwrap_or(trimmed)
            .to_ascii_lowercase();
        if prefix.is_empty() {
            return self
                .sorted_descriptors()
                .into_iter()
                .map(|entry| format!("/{}", entry.name))
                .collect();
        }

        let mut hits = self
            .commands
            .iter()
            .filter(|entry| {
                entry.name.starts_with(&prefix)
                    || entry.aliases.iter().any(|alias| alias.starts_with(&prefix))
            })
            .map(|entry| format!("/{}", entry.name))
            .collect::<Vec<_>>();
        hits.sort();
        hits.dedup();
        hits
    }
}

/// Creates the default chat command registry used by base gameplay.
pub fn default_chat_command_registry() -> CommandRegistry {
    let mut registry = CommandRegistry::default();
    registry.register(CommandDescriptor::new(
        "help",
        ["h"],
        "/help",
        "Lists available chat commands.",
    ));
    registry.register(CommandDescriptor::new(
        "gamemode",
        ["gm"],
        "/gamemode <survival|creative|spectator>",
        "Changes your current game mode.",
    ));
    registry
}

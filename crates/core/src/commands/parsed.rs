/// Parsed command token stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCommand {
    /// Full input as entered by the user, including the leading slash.
    pub raw_input: String,
    /// Canonical lowercase command token without slash.
    pub name: String,
    /// Positional arguments.
    pub args: Vec<String>,
}

/// Returns true when the supplied chat line starts a command.
pub fn is_command_input(input: &str) -> bool {
    input.trim_start().starts_with('/')
}

/// Parses a chat input into a `ParsedCommand`.
///
/// Returns `None` if the line is not a command or if the command token is empty.
pub fn parse_chat_command(input: &str) -> Option<ParsedCommand> {
    let raw = input.trim();
    if !raw.starts_with('/') {
        return None;
    }

    let body = raw[1..].trim();
    if body.is_empty() {
        return None;
    }

    let tokens = split_tokens(body);
    let (name, args) = tokens.split_first()?;
    Some(ParsedCommand {
        raw_input: raw.to_string(),
        name: name.to_ascii_lowercase(),
        args: args.to_vec(),
    })
}

/// Splits a command body into tokens while preserving quoted values.
///
/// Example:
/// `/say "hello world" now` -> `["say", "hello world", "now"]`
fn split_tokens(body: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in body.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

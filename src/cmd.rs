//! `/`-style commands entered from the prompt box.

/// A parsed command line, e.g. `/clear`, `/model llama3.2`, `/export path`.
#[derive(Debug, Clone)]
pub enum Command {
    /// Wipe the conversation (resets Ollama context).
    Clear,
    /// Switch the active model.
    Model(String),
    /// Export the current chat as markdown to `path`.
    Export(String),
    /// Unknown command.
    Unknown(String),
}

/// Parse a prompt starting with `/`.
pub fn parse(line: &str) -> Option<Command> {
    if !line.starts_with('/') {
        return None;
    }
    let mut parts = line[1..].splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest = parts.next().unwrap_or("").trim().to_string();
    let cmd = match name.as_str() {
        "clear" => Command::Clear,
        "model" => Command::Model(rest),
        "export" => Command::Export(rest),
        "help" => Command::Unknown("help: /clear /model <name> /export <path>".into()),
        _ => Command::Unknown(name),
    };
    Some(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_clear() {
        assert!(matches!(parse("/clear"), Some(Command::Clear)));
        assert!(matches!(parse("  /clear"), None)); // non-leading slash
    }

    #[test]
    fn parses_model_with_arg() {
        match parse("/model llama3.2") {
            Some(Command::Model(m)) => assert_eq!(m, "llama3.2"),
            _ => panic!("expected Model"),
        }
    }

    #[test]
    fn ignores_plain_text() {
        assert!(parse("hello world").is_none());
    }
}
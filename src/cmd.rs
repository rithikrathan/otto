//! `/`-style commands entered from the prompt box.

/// A parsed command line, e.g. `/clear`, `/model llama3.2`, `/export path`.
#[derive(Debug, Clone)]
pub enum Command {
    /// Wipe the conversation (resets Ollama context).
    Clear,
    /// Switch the active model. Empty string = open the model picker.
    Model(String),
    /// Export the current chat as markdown to `path`.
    Export(String),
    /// Open the settings window.
    Settings,
    /// Quit the app.
    Quit,
    /// Unknown command.
    Unknown(String),
}

/// Parse a prompt starting with `/` (or a `:q`-style quit shortcut).
pub fn parse(line: &str) -> Option<Command> {
    // `:q` / `:quit` vi-style quit shortcut.
    let trimmed = line.trim();
    if trimmed == ":q" || trimmed == ":quit" || trimmed == ":wq" {
        return Some(Command::Quit);
    }
    if !line.starts_with('/') {
        return None;
    }
    let mut parts = line[1..].splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest = parts.next().unwrap_or("").trim().to_string();
    let cmd = match name.as_str() {
        "clear" => Command::Clear,
        "model" => Command::Model(rest),
        "settings" | "config" => Command::Settings,
        "export" => Command::Export(rest),
        "exit" | "quit" | "q" => Command::Quit,
        "help" => Command::Unknown(
            "help: /clear /model [name] /settings /export <path> /exit :q".into(),
        ),
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

    #[test]
    fn parses_exit_and_quit_shortcuts() {
        assert!(matches!(parse("/exit"), Some(Command::Quit)));
        assert!(matches!(parse("/q"), Some(Command::Quit)));
        assert!(matches!(parse(":q"), Some(Command::Quit)));
        assert!(matches!(parse(":quit"), Some(Command::Quit)));
    }

    #[test]
    fn parses_settings_and_model_picker() {
        assert!(matches!(parse("/settings"), Some(Command::Settings)));
        assert!(matches!(parse("/config"), Some(Command::Settings)));
        assert!(matches!(parse("/model"), Some(Command::Model(_))));
    }
}
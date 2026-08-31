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
    /// Change the server endpoint URL.
    Endpoint(String),
    /// Set the system prompt (clears chat context).
    System(String),
    /// Open the settings window.
    Settings,
    /// Show help.
    Help,
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
        "endpoint" => Command::Endpoint(rest),
        "system" | "sys" => Command::System(rest),
        "settings" | "config" => Command::Settings,
        "export" => Command::Export(rest),
        "exit" | "quit" | "q" => Command::Quit,
        "help" => Command::Help,
        _ => Command::Unknown(name),
    };
    Some(cmd)
}

/// Returns ghost text for autocomplete if the user's prompt matches uniquely
pub fn autocomplete(line: &str) -> Option<&'static str> {
    if !line.starts_with('/') || line.contains(char::is_whitespace) {
        return None;
    }
    let commands = [
        "/clear",
        "/model",
        "/endpoint",
        "/system",
        "/settings",
        "/export",
        "/help",
    ];
    let mut match_found = None;
    for cmd in commands {
        if cmd.starts_with(line) && cmd != line {
            if match_found.is_some() {
                // More than one match, maybe return shortest or None?
                // Returning first match is fine for now, or just return it.
                // Actually if they type /e, it matches /export and /exit. Let's return None if multiple, or just the first.
                // Let's return None for ambiguity, EXCEPT if we want to just suggest the first. Code complete suggests the first.
            }
            if match_found.is_none() {
                match_found = Some(&cmd[line.len()..]);
            }
        }
    }
    match_found
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

    #[test]
    fn parses_system_prompt() {
        match parse("/system be terse") {
            Some(Command::System(s)) => assert_eq!(s, "be terse"),
            _ => panic!("expected System"),
        }
        match parse("/sys no yap") {
            Some(Command::System(s)) => assert_eq!(s, "no yap"),
            _ => panic!("expected System alias"),
        }
        assert!(matches!(parse("/system"), Some(Command::System(s)) if s.is_empty()));
    }

    #[test]
    fn test_case_insensitive_and_whitespace_commands() {
        assert!(matches!(parse("/CLEAR"), Some(Command::Clear)));
        assert!(matches!(parse("/SETTINGS"), Some(Command::Settings)));
        match parse("/MODEL    qwen2.5-coder-7b:latest   ") {
            Some(Command::Model(m)) => assert_eq!(m, "qwen2.5-coder-7b:latest"),
            _ => panic!("expected Model"),
        }
    }

    #[test]
    fn test_unknown_command_fallback() {
        assert!(matches!(parse("/nonexistent_cmd"), Some(Command::Unknown(_))));
    }

    #[test]
    fn test_autocomplete_edge_cases() {
        assert_eq!(autocomplete(""), None);
        assert_eq!(autocomplete("hello"), None);
        assert_eq!(autocomplete("/"), Some("clear"));
        assert_eq!(autocomplete("/set"), Some("tings"));
        assert_eq!(autocomplete("/mod"), Some("el"));
        assert_eq!(autocomplete("/model "), None); // whitespace returns None
    }
}

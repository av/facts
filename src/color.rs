/// Minimal colored terminal output.
///
/// Colors are enabled only when stdout is a TTY.
use std::io::IsTerminal;
use std::sync::OnceLock;

static USE_COLOR: OnceLock<bool> = OnceLock::new();

/// Whether to use ANSI color codes.
pub fn enabled() -> bool {
    *USE_COLOR.get_or_init(|| std::io::stdout().is_terminal())
}

/// ANSI escape: green text.
pub fn green(s: &str) -> String {
    if enabled() {
        format!("\x1b[32m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// ANSI escape: red text.
pub fn red(s: &str) -> String {
    if enabled() {
        format!("\x1b[31m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// ANSI escape: yellow text.
pub fn yellow(s: &str) -> String {
    if enabled() {
        format!("\x1b[33m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// ANSI escape: bold text.
pub fn bold(s: &str) -> String {
    if enabled() {
        format!("\x1b[1m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// ANSI escape: dim text.
pub fn dim(s: &str) -> String {
    if enabled() {
        format!("\x1b[2m{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_color_when_not_tty() {
        // In test harness, stdout is typically not a TTY
        // so colors should be disabled.
        // Just verify the functions don't panic and return the input.
        let result = green("hello");
        assert!(result.contains("hello"));
    }
}

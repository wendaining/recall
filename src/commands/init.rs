use anyhow::Result;

use crate::cli::InitArgs;
use crate::config::Config;

const SEARCH_KEY_PLACEHOLDER: &str = "@RECALL_SEARCH_KEY@";

pub fn run(args: InitArgs) -> Result<()> {
    let script = match args.shell.as_str() {
        "zsh" => include_str!("../../shell/recall.zsh"),
        "bash" => include_str!("../../shell/recall.bash"),
        "fish" => include_str!("../../shell/recall.fish"),
        _ => unreachable!("clap restricts the shell value"),
    };
    let config = Config::load().unwrap_or_default();
    let key = search_key_for(&config.ui.search_key, &args.shell);
    print!("{}", script.replace(SEARCH_KEY_PLACEHOLDER, &key));
    Ok(())
}

fn search_key_for(spec: &str, shell: &str) -> String {
    match parse_key_sequence(spec) {
        Some(keys) => encode_key_sequence(&keys, shell),
        None => spec.to_string(),
    }
}

#[derive(Debug, PartialEq)]
enum Key {
    Alt(char),
    Ctrl(char),
}

fn parse_key_sequence(spec: &str) -> Option<Vec<Key>> {
    let mut keys = Vec::new();
    for token in spec.split_whitespace() {
        let (modifier, rest) = token.split_once('-')?;
        let mut chars = rest.chars();
        let ch = chars.next()?;
        if chars.next().is_some() || !ch.is_ascii_alphabetic() {
            return None;
        }
        keys.push(match modifier.to_ascii_lowercase().as_str() {
            "alt" | "meta" | "option" => Key::Alt(ch),
            "ctrl" | "control" => Key::Ctrl(ch),
            _ => return None,
        });
    }
    (!keys.is_empty()).then_some(keys)
}

fn encode_key_sequence(keys: &[Key], shell: &str) -> String {
    let mut out = String::new();
    for key in keys {
        match (shell, key) {
            ("zsh", Key::Alt(ch)) => {
                out.push_str("^[");
                out.push(ch.to_ascii_lowercase());
            }
            ("zsh", Key::Ctrl(ch)) => {
                out.push('^');
                out.push(ch.to_ascii_uppercase());
            }
            ("bash", Key::Alt(ch)) | ("fish", Key::Alt(ch)) => {
                out.push_str("\\e");
                out.push(ch.to_ascii_lowercase());
            }
            ("bash", Key::Ctrl(ch)) => {
                out.push_str("\\C-");
                out.push(ch.to_ascii_lowercase());
            }
            ("fish", Key::Ctrl(ch)) => {
                out.push_str("\\c");
                out.push(ch.to_ascii_lowercase());
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_keys_per_shell() {
        assert_eq!(search_key_for("alt-r", "zsh"), "^[r");
        assert_eq!(search_key_for("alt-r", "bash"), "\\er");
        assert_eq!(search_key_for("alt-r", "fish"), "\\er");
        assert_eq!(search_key_for("ctrl-t", "zsh"), "^T");
        assert_eq!(search_key_for("ctrl-t", "bash"), "\\C-t");
        assert_eq!(search_key_for("ctrl-t", "fish"), "\\ct");
        assert_eq!(search_key_for("ctrl-x ctrl-r", "zsh"), "^X^R");
        assert_eq!(search_key_for("ctrl-x ctrl-r", "bash"), "\\C-x\\C-r");
        assert_eq!(search_key_for("ctrl-x ctrl-r", "fish"), "\\cx\\cr");
    }

    #[test]
    fn accepts_alt_aliases_case_insensitively() {
        assert_eq!(search_key_for("Option-R", "zsh"), "^[r");
        assert_eq!(search_key_for("META-r", "zsh"), "^[r");
    }

    #[test]
    fn unknown_spec_is_passed_through() {
        assert_eq!(search_key_for("not-a-key", "zsh"), "not-a-key");
        assert_eq!(search_key_for("", "zsh"), "");
    }
}

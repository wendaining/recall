use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Result, bail};

use crate::config::Config;
use crate::db::{Db, queries};
use crate::model::{Block, BlockKind};
use crate::shell::{HistoryFormat, Shell};
use crate::util;

pub(super) fn run(shell_name: String, path: Option<PathBuf>) -> Result<()> {
    let shell = Shell::from_name(&shell_name).expect("clap restricts history formats");
    let format = shell
        .history_format()
        .expect("clap restricts history formats");
    let path = match path.or_else(|| shell.history_path()) {
        Some(path) => path,
        None => bail!("cannot determine the default {shell_name} history path; pass --path"),
    };
    if !path.is_file() {
        bail!("{shell_name} history file not found at {}", path.display());
    }

    let bytes = std::fs::read(&path)?;
    let entries = parse(format, &String::from_utf8_lossy(&bytes));
    let modified = modified_ns(&path).unwrap_or_else(util::now_ns);
    let canonical = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    let source = format!("history:{}:{}", shell.name, canonical.display());
    let config = Config::load()?;
    let db = Db::open(&config.general.db_path)?;

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let total = entries.len();
    for (index, entry) in entries.into_iter().enumerate() {
        let started_at = entry.started_at.unwrap_or_else(|| {
            modified.saturating_sub((total.saturating_sub(index) as i64) * 1_000_000)
        });
        let external_id = entry_id(index, &entry);
        let block = Block {
            id: crate::commands::new_id(),
            session: None,
            hostname: util::resolved_hostname(&config),
            shell: Some(shell.name.to_string()),
            command: entry.command,
            cwd: None,
            started_at,
            duration_ns: entry.duration_ns,
            exit_code: None,
            output: None,
            output_bytes: 0,
            output_lines: 0,
            output_truncated: false,
            kind: BlockKind::Unavailable,
            created_at: util::now_ns(),
        };
        if queries::insert_imported(&db.conn, &block, &source, &external_id)? {
            imported += 1;
        } else {
            skipped += 1;
        }
    }

    println!(
        "imported {imported} block(s) from {} ({skipped} already present)",
        path.display()
    );
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct HistoryEntry {
    command: String,
    started_at: Option<i64>,
    duration_ns: Option<i64>,
}

fn parse(format: HistoryFormat, text: &str) -> Vec<HistoryEntry> {
    match format {
        HistoryFormat::Bash => parse_bash(text),
        HistoryFormat::Zsh => parse_zsh(text),
        HistoryFormat::Fish => parse_fish(text),
        HistoryFormat::PowerShell => parse_powershell(text),
    }
}

fn parse_bash(text: &str) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    let mut timestamp = None;
    for line in text.lines().map(trim_cr) {
        if let Some(seconds) = line.strip_prefix('#').and_then(parse_seconds) {
            timestamp = Some(seconds);
        } else if !line.is_empty() {
            entries.push(HistoryEntry {
                command: line.to_string(),
                started_at: timestamp.take(),
                duration_ns: None,
            });
        }
    }
    entries
}

fn parse_zsh(text: &str) -> Vec<HistoryEntry> {
    text.lines()
        .map(trim_cr)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let Some(rest) = line.strip_prefix(": ") else {
                return plain_entry(line);
            };
            let Some((seconds, rest)) = rest.split_once(':') else {
                return plain_entry(line);
            };
            let Some((duration, command)) = rest.split_once(';') else {
                return plain_entry(line);
            };
            let Some(started_at) = parse_seconds(seconds) else {
                return plain_entry(line);
            };
            HistoryEntry {
                command: command.to_string(),
                started_at: Some(started_at),
                duration_ns: duration
                    .parse::<i64>()
                    .ok()
                    .and_then(|value| value.checked_mul(1_000_000_000)),
            }
        })
        .collect()
}

fn parse_fish(text: &str) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    let mut current: Option<HistoryEntry> = None;
    for line in text.lines().map(trim_cr) {
        if let Some(command) = line.strip_prefix("- cmd: ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(HistoryEntry {
                command: unescape_fish(command),
                started_at: None,
                duration_ns: None,
            });
        } else if let Some(seconds) = line
            .trim_start()
            .strip_prefix("when: ")
            .and_then(parse_seconds)
            && let Some(entry) = current.as_mut()
        {
            entry.started_at = Some(seconds);
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    entries
}

fn parse_powershell(text: &str) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    let mut command = String::new();
    for line in text.lines().map(trim_cr) {
        if command.is_empty() && line.is_empty() {
            continue;
        }
        if !command.is_empty() {
            command.push('\n');
        }
        command.push_str(line);
        if !line.ends_with('`') {
            entries.push(HistoryEntry {
                command: std::mem::take(&mut command),
                started_at: None,
                duration_ns: None,
            });
        }
    }
    if !command.is_empty() {
        entries.push(HistoryEntry {
            command,
            started_at: None,
            duration_ns: None,
        });
    }
    entries
}

fn plain_entry(command: &str) -> HistoryEntry {
    HistoryEntry {
        command: command.to_string(),
        started_at: None,
        duration_ns: None,
    }
}

fn parse_seconds(value: &str) -> Option<i64> {
    value
        .parse::<i64>()
        .ok()
        .and_then(|seconds| seconds.checked_mul(1_000_000_000))
}

fn trim_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

fn unescape_fish(command: &str) -> String {
    let mut result = String::with_capacity(command.len());
    let mut chars = command.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('\\') => result.push('\\'),
            Some(next) => {
                result.push('\\');
                result.push(next);
            }
            None => result.push('\\'),
        }
    }
    result
}

fn modified_ns(path: &Path) -> Option<i64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos() as i64)
}

fn entry_id(index: usize, entry: &HistoryEntry) -> String {
    let mut hasher = StableHasher::default();
    hasher.write(&(index as u64).to_le_bytes());
    hasher.write(entry.command.as_bytes());
    hasher.write(&[entry.started_at.is_some() as u8]);
    hasher.write(&entry.started_at.unwrap_or_default().to_le_bytes());
    hasher.write(&[entry.duration_ns.is_some() as u8]);
    hasher.write(&entry.duration_ns.unwrap_or_default().to_le_bytes());
    format!("{index}:{:016x}", hasher.finish())
}

struct StableHasher(u64);

impl Default for StableHasher {
    fn default() -> Self {
        Self(0xcbf29ce484222325)
    }
}

impl Hasher for StableHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bash_timestamps_and_plain_entries() {
        assert_eq!(
            parse_bash("#1700000000\necho one\nprintf two\n"),
            vec![
                HistoryEntry {
                    command: "echo one".to_string(),
                    started_at: Some(1_700_000_000_000_000_000),
                    duration_ns: None,
                },
                plain_entry("printf two"),
            ]
        );
    }

    #[test]
    fn parses_zsh_extended_history() {
        assert_eq!(
            parse_zsh(": 1700000000:3;echo one\nplain command\n"),
            vec![
                HistoryEntry {
                    command: "echo one".to_string(),
                    started_at: Some(1_700_000_000_000_000_000),
                    duration_ns: Some(3_000_000_000),
                },
                plain_entry("plain command"),
            ]
        );
    }

    #[test]
    fn parses_fish_history_metadata() {
        assert_eq!(
            parse_fish("- cmd: echo one\\ntwo\n  when: 1700000000\n- cmd: pwd\n"),
            vec![
                HistoryEntry {
                    command: "echo one\ntwo".to_string(),
                    started_at: Some(1_700_000_000_000_000_000),
                    duration_ns: None,
                },
                plain_entry("pwd"),
            ]
        );
    }

    #[test]
    fn joins_powershell_continuation_lines() {
        assert_eq!(
            parse_powershell("Write-Output `\r\n  hello\r\nGet-Location\r\n"),
            vec![
                plain_entry("Write-Output `\n  hello"),
                plain_entry("Get-Location"),
            ]
        );
    }

    #[test]
    fn entry_ids_are_stable_and_keep_duplicate_executions() {
        let entry = plain_entry("echo repeated");
        assert_eq!(entry_id(0, &entry), entry_id(0, &entry));
        assert_ne!(entry_id(0, &entry), entry_id(1, &entry));
    }
}

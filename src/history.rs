use crate::command::CommandTree;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY_FILE: &str = ".history.jsonl";

/// One line of `.commands/.history.jsonl`
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// A `las <cmd>` invocation
    Run {
        ts: u64,
        command: String,
        args: Vec<String>,
        exit: i32,
        duration_ms: u64,
    },
    /// A raw shell command observed via `--observe` (e.g. from an agent hook)
    External { ts: u64, command: String },
}

pub fn history_path(commands_dir: &Path) -> PathBuf {
    commands_dir.join(HISTORY_FILE)
}

/// Record a command invocation. Never fails: history must not break a run.
pub fn record_run(
    commands_dir: &Path,
    cmd_path: &str,
    args: &[String],
    exit: i32,
    duration_ms: u64,
) {
    append(
        commands_dir,
        &Entry::Run {
            ts: now(),
            command: cmd_path.to_string(),
            args: args.to_vec(),
            exit,
            duration_ms,
        },
    );
}

/// Record an external shell command for later `--suggest` analysis.
/// Commands invoking this CLI itself are skipped (they're already recorded as runs).
pub fn record_external(commands_dir: &Path, name: &str, command: &str) {
    let trimmed = command.trim();
    if trimmed.is_empty() || trimmed.split_whitespace().next() == Some(name) {
        return;
    }
    append(
        commands_dir,
        &Entry::External {
            ts: now(),
            command: trimmed.to_string(),
        },
    );
}

/// Extract the shell command from `--observe` input: either a Claude Code
/// PostToolUse hook payload (JSON with .tool_input.command) or a raw line.
pub fn parse_observed(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
        && let Some(cmd) = value
            .get("tool_input")
            .and_then(|t| t.get("command"))
            .and_then(|c| c.as_str())
    {
        return Some(cmd.to_string());
    }
    Some(trimmed.to_string())
}

/// Build the `--suggest` report: which commands earn their keep, which raw
/// shell commands repeat often enough to deserve extraction.
pub fn generate_suggest(tree: &CommandTree, commands_dir: &Path, name: &str) -> String {
    let entries = read_entries(commands_dir);
    let mut output = String::new();

    if entries.is_empty() {
        output.push_str("No history yet.\n\n");
        output.push_str(&format!(
            "History accrues in {} as commands run.\n",
            history_path(commands_dir).display()
        ));
        output.push_str(&format!(
            "To also track raw shell commands (the extraction candidates), pipe them to `{} --observe` —\n\
             e.g. a Claude Code PostToolUse hook on Bash: {{\"type\": \"command\", \"command\": \"{} --observe\"}}\n",
            name, name
        ));
        return output;
    }

    // Aggregate las runs per command
    let mut runs: BTreeMap<String, (u64, u64)> = BTreeMap::new(); // command -> (count, failures)
    let mut externals: BTreeMap<String, u64> = BTreeMap::new(); // normalized command -> count
    for entry in &entries {
        match entry {
            Entry::Run { command, exit, .. } => {
                let stat = runs.entry(command.clone()).or_default();
                stat.0 += 1;
                if *exit != 0 {
                    stat.1 += 1;
                }
            }
            Entry::External { command, .. } => {
                let normalized = command.split_whitespace().collect::<Vec<_>>().join(" ");
                *externals.entry(normalized).or_default() += 1;
            }
        }
    }

    // Section 1: command usage
    if !runs.is_empty() {
        output.push_str("COMMAND USAGE\n");
        let mut by_count: Vec<(&String, &(u64, u64))> = runs.iter().collect();
        by_count.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
        for (command, (count, failures)) in by_count {
            if *failures > 0 {
                output.push_str(&format!(
                    "  {:<16}{} runs, {} failed\n",
                    command, count, failures
                ));
            } else {
                output.push_str(&format!("  {:<16}{} runs\n", command, count));
            }
        }

        // Commands that exist but never appear in history
        let mut all_paths = Vec::new();
        collect_paths(tree, "", &mut all_paths);
        let unused: Vec<&String> = all_paths
            .iter()
            .filter(|path| !runs.contains_key(*path))
            .collect();
        if !unused.is_empty() {
            output.push_str(&format!(
                "  never used:     {}\n",
                unused
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        output.push('\n');
    }

    // Section 2: extraction candidates — repeated multi-word raw commands
    let mut candidates: Vec<(&String, &u64)> = externals
        .iter()
        .filter(|(command, count)| **count >= 3 && command.contains(' '))
        .collect();
    candidates.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    candidates.truncate(10);

    if candidates.is_empty() {
        output.push_str("EXTRACTION CANDIDATES\n");
        output.push_str("  none yet — repeated raw shell commands (3+ exact runs) show up here.\n");
        if externals.is_empty() {
            output.push_str(&format!(
                "  (no raw shell commands observed; hook `{} --observe` into your agent to feed this)\n",
                name
            ));
        }
    } else {
        output.push_str("EXTRACTION CANDIDATES (repeated raw shell commands)\n");
        for (command, count) in candidates {
            output.push_str(&format!("  {:>3}x  {}\n", count, command));
        }
        output.push('\n');
        output.push_str(&format!(
            "Save one as a command: {} --new <name>, then paste the shell line into the script.\n",
            name
        ));
    }

    output
}

fn collect_paths(tree: &CommandTree, prefix: &str, out: &mut Vec<String>) {
    if let Some(children) = tree.children() {
        for (child_name, child) in children {
            let path = if prefix.is_empty() {
                child_name.clone()
            } else {
                format!("{} {}", prefix, child_name)
            };
            match child {
                CommandTree::Leaf(_) => out.push(path),
                CommandTree::Group { .. } => collect_paths(child, &path, out),
            }
        }
    }
}

fn read_entries(commands_dir: &Path) -> Vec<Entry> {
    let Ok(content) = fs::read_to_string(history_path(commands_dir)) else {
        return Vec::new();
    };
    content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Append one entry, creating the file (and a .gitignore guard) on first write.
/// All errors are swallowed: history is best-effort.
fn append(commands_dir: &Path, entry: &Entry) {
    let Ok(json) = serde_json::to_string(entry) else {
        return;
    };
    ensure_gitignore(commands_dir);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path(commands_dir))
    {
        let _ = writeln!(file, "{}", json);
    }
}

/// Keep the history file out of version control without touching the user's
/// root .gitignore: maintain a .commands/.gitignore that covers it.
fn ensure_gitignore(commands_dir: &Path) {
    let gitignore = commands_dir.join(".gitignore");
    match fs::read_to_string(&gitignore) {
        Ok(content) => {
            if !content.lines().any(|line| line.trim() == HISTORY_FILE)
                && let Ok(mut file) = OpenOptions::new().append(true).open(&gitignore)
            {
                let _ = writeln!(file, "{}", HISTORY_FILE);
            }
        }
        Err(_) => {
            let _ = fs::write(&gitignore, format!("{}\n", HISTORY_FILE));
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandDef;
    use tempfile::TempDir;

    fn commands_dir() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join(".commands");
        fs::create_dir(&dir).unwrap();
        (temp, dir)
    }

    fn make_tree() -> CommandTree {
        let mut tree = CommandTree::new_group("root");
        for name in ["shot", "test"] {
            tree.insert(
                name,
                CommandTree::Leaf(CommandDef {
                    name: name.into(),
                    ..Default::default()
                }),
            );
        }
        tree
    }

    #[test]
    fn record_run_appends_jsonl_and_gitignore() {
        let (_temp, dir) = commands_dir();

        record_run(&dir, "shot", &["--fast".to_string()], 0, 120);
        record_run(&dir, "shot", &[], 1, 80);

        let content = fs::read_to_string(history_path(&dir)).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: Entry = serde_json::from_str(lines[0]).unwrap();
        match first {
            Entry::Run {
                command,
                args,
                exit,
                duration_ms,
                ..
            } => {
                assert_eq!(command, "shot");
                assert_eq!(args, vec!["--fast"]);
                assert_eq!(exit, 0);
                assert_eq!(duration_ms, 120);
            }
            _ => panic!("expected run entry"),
        }

        let gitignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(gitignore.contains(".history.jsonl"));
    }

    #[test]
    fn gitignore_appended_not_clobbered() {
        let (_temp, dir) = commands_dir();
        fs::write(dir.join(".gitignore"), "something-else\n").unwrap();

        record_run(&dir, "test", &[], 0, 5);

        let gitignore = fs::read_to_string(dir.join(".gitignore")).unwrap();
        assert!(gitignore.contains("something-else"));
        assert!(gitignore.contains(".history.jsonl"));
    }

    #[test]
    fn record_external_skips_own_cli_and_blanks() {
        let (_temp, dir) = commands_dir();

        record_external(&dir, "las", "las shot");
        record_external(&dir, "las", "   ");
        record_external(&dir, "las", "git status -sb");

        let content = fs::read_to_string(history_path(&dir)).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("git status -sb"));
    }

    #[test]
    fn parse_observed_handles_hook_json_and_raw() {
        let hook = r#"{"tool_name":"Bash","tool_input":{"command":"cargo test","description":"Run tests"}}"#;
        assert_eq!(parse_observed(hook).as_deref(), Some("cargo test"));

        assert_eq!(
            parse_observed("  rg TODO scripts/  ").as_deref(),
            Some("rg TODO scripts/")
        );

        assert_eq!(parse_observed(""), None);

        // JSON without a tool_input.command falls back to the raw text
        let other_json = r#"{"foo": 1}"#;
        assert_eq!(parse_observed(other_json).as_deref(), Some(other_json));
    }

    #[test]
    fn suggest_reports_usage_and_candidates() {
        let (_temp, dir) = commands_dir();

        for _ in 0..3 {
            record_run(&dir, "shot", &[], 0, 50);
        }
        record_run(&dir, "shot", &[], 1, 50);
        for _ in 0..4 {
            record_external(&dir, "las", "pkill -f godot_server");
        }
        record_external(&dir, "las", "echo once");

        let report = generate_suggest(&make_tree(), &dir, "las");

        assert!(report.contains("COMMAND USAGE"));
        assert!(report.contains("shot"));
        assert!(report.contains("4 runs, 1 failed"));
        assert!(report.contains("never used:     test"));
        assert!(report.contains("EXTRACTION CANDIDATES"));
        assert!(report.contains("4x  pkill -f godot_server"));
        // a single occurrence is not a candidate
        assert!(!report.contains("echo once"));
    }

    #[test]
    fn suggest_ignores_single_word_commands() {
        let (_temp, dir) = commands_dir();
        for _ in 0..5 {
            record_external(&dir, "las", "ls");
        }

        let report = generate_suggest(&make_tree(), &dir, "las");
        assert!(!report.contains("5x  ls"));
    }

    #[test]
    fn suggest_with_no_history_explains_setup() {
        let (_temp, dir) = commands_dir();
        let report = generate_suggest(&make_tree(), &dir, "las");
        assert!(report.contains("No history yet"));
        assert!(report.contains("--observe"));
    }

    #[test]
    fn read_entries_skips_malformed_lines() {
        let (_temp, dir) = commands_dir();
        record_run(&dir, "shot", &[], 0, 1);
        let mut file = OpenOptions::new()
            .append(true)
            .open(history_path(&dir))
            .unwrap();
        writeln!(file, "not json").unwrap();
        record_run(&dir, "shot", &[], 0, 1);

        assert_eq!(read_entries(&dir).len(), 2);
    }
}

use crate::command::CommandTree;
use crate::skill::generate_skill;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Write the agent skill document to `.claude/skills/<name>/SKILL.md` next to
/// the `.commands/` directory, so agents discover the CLI without spending a
/// turn on `--help`. Idempotent: regenerates the file on every call.
pub fn sync_skill(tree: &CommandTree, commands_dir: &Path, name: &str) -> Result<PathBuf> {
    let project_root = commands_dir
        .parent()
        .context("commands directory has no parent")?;

    let skill_name = sanitize_skill_name(name);
    let skill_dir = project_root
        .join(".claude")
        .join("skills")
        .join(&skill_name);
    fs::create_dir_all(&skill_dir)
        .with_context(|| format!("failed to create {}", skill_dir.display()))?;

    let skill_path = skill_dir.join("SKILL.md");
    let content = format!(
        "{}{}",
        skill_frontmatter(tree, &skill_name, name),
        generate_skill(tree, commands_dir, name)
    );
    fs::write(&skill_path, content)
        .with_context(|| format!("failed to write {}", skill_path.display()))?;

    Ok(skill_path)
}

/// Build the YAML frontmatter agent harnesses use to decide when to load the skill
fn skill_frontmatter(tree: &CommandTree, skill_name: &str, name: &str) -> String {
    let mut top_level: Vec<String> = tree
        .children()
        .map(|children| children.keys().cloned().collect())
        .unwrap_or_default();

    // Keep the description bounded for harness limits
    const MAX_LISTED: usize = 12;
    let elided = top_level.len().saturating_sub(MAX_LISTED);
    top_level.truncate(MAX_LISTED);
    let mut summary = top_level.join(", ");
    if elided > 0 {
        summary.push_str(&format!(", … {} more", elided));
    }

    format!(
        "---\nname: {}\ndescription: Project task runner `{}` ({}). Use when running project tasks, tests, or builds, before hand-rolling shell commands for anything a listed command covers, and when saving a repeated shell workflow as a new command.\n---\n\n",
        skill_name, name, summary
    )
}

/// Skill names must be filesystem- and harness-friendly: lowercase alphanumerics and dashes
fn sanitize_skill_name(name: &str) -> String {
    let sanitized: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "las".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandDef;
    use tempfile::TempDir;

    fn make_tree() -> CommandTree {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "test",
            CommandTree::Leaf(CommandDef {
                name: "test".into(),
                description: Some("Run the test suite".into()),
                ..Default::default()
            }),
        );
        tree
    }

    #[test]
    fn sync_writes_skill_with_frontmatter() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let tree = make_tree();
        let path = sync_skill(&tree, &commands_dir, "las").unwrap();

        assert_eq!(
            path,
            temp.path()
                .join(".claude")
                .join("skills")
                .join("las")
                .join("SKILL.md")
        );

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("---\nname: las\n"));
        assert!(content.contains("description: Project task runner"));
        assert!(content.contains("# las — CLI Skill Document"));
        assert!(content.contains("Run the test suite"));
    }

    #[test]
    fn sync_is_idempotent_and_overwrites() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let tree = make_tree();
        let path = sync_skill(&tree, &commands_dir, "las").unwrap();
        let first = fs::read_to_string(&path).unwrap();

        // Re-sync after the doc went stale
        fs::write(&path, "stale").unwrap();
        sync_skill(&tree, &commands_dir, "las").unwrap();
        let second = fs::read_to_string(&path).unwrap();

        assert_eq!(first, second);
    }

    #[test]
    fn sync_description_lists_commands() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let tree = make_tree();
        let path = sync_skill(&tree, &commands_dir, "las").unwrap();
        let content = fs::read_to_string(&path).unwrap();

        // The frontmatter block (between the first two --- lines) summarizes commands
        let frontmatter: String = content
            .lines()
            .take_while(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(frontmatter.contains("test"));
    }

    #[test]
    fn sanitize_skill_name_handles_odd_names() {
        assert_eq!(sanitize_skill_name("las"), "las");
        assert_eq!(sanitize_skill_name("My Project!"), "my-project");
        assert_eq!(sanitize_skill_name("___"), "las");
    }
}

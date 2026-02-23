use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use walkdir::WalkDir;

use crate::command::CommandTree;
use crate::frontmatter::command_from_script;

#[derive(Debug, Deserialize, Default)]
struct GroupConfig {
    description: Option<String>,
    order: Option<Vec<String>>,
}

/// Walk up from `start` looking for a `.commands/` directory.
/// Returns the path to `.commands/` if found, None otherwise.
pub fn find_commands_dir(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();

    loop {
        let candidate = current.join(".commands");
        if candidate.is_dir() {
            return Some(candidate);
        }

        if !current.pop() {
            // Reached filesystem root
            return None;
        }
    }
}

/// Discover all commands in the `.commands/` directory and build a CommandTree.
pub fn discover(commands_dir: &Path) -> Result<CommandTree> {
    if !commands_dir.is_dir() {
        anyhow::bail!("{} is not a directory", commands_dir.display());
    }

    let mut root = CommandTree::new_group("root");

    // Walk the directory
    for entry in WalkDir::new(commands_dir)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Skip hidden files and directories
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.')
        })
    {
        let entry = entry.context("failed to read directory entry")?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy();

        // Skip files starting with underscore (except special files we handle separately)
        if file_name.starts_with('_') && file_name != "_group.yml" {
            continue;
        }

        if path.is_file() && file_name.ends_with(".sh") {
            // Check if executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let metadata = fs::metadata(path)
                    .with_context(|| format!("failed to read metadata for {}", path.display()))?;
                let perms = metadata.permissions();
                if perms.mode() & 0o111 == 0 {
                    eprintln!(
                        "warning: {} is not executable, but will be included",
                        path.display()
                    );
                }
            }

            // Read script content
            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;

            // Extract command name (file name without .sh extension)
            let cmd_name = file_name.trim_end_matches(".sh").to_string();

            // Parse command
            let cmd = command_from_script(&cmd_name, path.to_path_buf(), &content)
                .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", path.display(), e))?;

            // Build path from commands_dir to this file
            let rel_path = path
                .strip_prefix(commands_dir)
                .context("path should be within commands_dir")?;

            // Insert into tree at the appropriate location
            insert_command(&mut root, rel_path, CommandTree::Leaf(cmd))?;
        }
    }

    // Now load _group.yml files
    for entry in WalkDir::new(commands_dir)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !name.starts_with('.') || name == "_group.yml"
        })
    {
        let entry = entry.context("failed to read directory entry")?;
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy();

        if path.is_file() && file_name == "_group.yml" {
            let content = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let config: GroupConfig = serde_yaml::from_str(&content)
                .with_context(|| format!("failed to parse {}", path.display()))?;

            // Get the group path (parent directory)
            let group_dir = path.parent().context("_group.yml should have parent")?;
            let rel_path = group_dir
                .strip_prefix(commands_dir)
                .context("path should be within commands_dir")?;

            // Apply config to the group
            apply_group_config(&mut root, rel_path, config)?;
        }
    }

    Ok(root)
}

/// Insert a command into the tree at the path specified by rel_path.
/// Creates intermediate groups as needed.
fn insert_command(tree: &mut CommandTree, rel_path: &Path, cmd: CommandTree) -> Result<()> {
    let components: Vec<_> = rel_path.components().collect();

    if components.is_empty() {
        anyhow::bail!("empty path");
    }

    // If only one component (file at root), insert directly
    if components.len() == 1 {
        let name = components[0]
            .as_os_str()
            .to_string_lossy()
            .trim_end_matches(".sh")
            .to_string();
        tree.insert(name, cmd);
        return Ok(());
    }

    // Otherwise, navigate/create groups
    let mut current = tree;
    for (i, component) in components.iter().enumerate() {
        let name = component.as_os_str().to_string_lossy();

        // Last component is the command file
        if i == components.len() - 1 {
            let cmd_name = name.trim_end_matches(".sh").to_string();
            current.insert(cmd_name, cmd);
            return Ok(());
        }

        // Intermediate directories are groups
        let group_name = name.to_string();
        if let CommandTree::Group { children, .. } = current {
            current = children
                .entry(group_name.clone())
                .or_insert_with(|| CommandTree::new_group(&group_name));
        } else {
            anyhow::bail!("expected group, found leaf");
        }
    }

    Ok(())
}

/// Apply group configuration (description and order) to a group in the tree.
fn apply_group_config(tree: &mut CommandTree, rel_path: &Path, config: GroupConfig) -> Result<()> {
    let components: Vec<_> = rel_path.components().collect();

    if components.is_empty() {
        // Applying to root
        if let CommandTree::Group {
            description, order, ..
        } = tree
        {
            *description = config.description;
            *order = config.order;
        }
        return Ok(());
    }

    // Navigate to the group
    let mut current = tree;
    for component in components {
        let name = component.as_os_str().to_string_lossy();
        if let CommandTree::Group { children, .. } = current {
            current = children
                .get_mut(name.as_ref())
                .with_context(|| format!("group {} not found", name))?;
        } else {
            anyhow::bail!("expected group, found leaf");
        }
    }

    // Apply config
    if let CommandTree::Group {
        description, order, ..
    } = current
    {
        *description = config.description;
        *order = config.order;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn create_executable(path: &Path, content: &str) -> Result<()> {
        fs::write(path, content)?;
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms)?;
        }
        Ok(())
    }

    #[test]
    fn find_commands_dir_at_current() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let found = find_commands_dir(temp.path());
        assert_eq!(found, Some(commands_dir));
    }

    #[test]
    fn find_commands_dir_walks_up() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let subdir = temp.path().join("a").join("b").join("c");
        fs::create_dir_all(&subdir).unwrap();

        let found = find_commands_dir(&subdir);
        assert_eq!(found, Some(commands_dir));
    }

    #[test]
    fn find_commands_dir_not_found() {
        let temp = TempDir::new().unwrap();
        let found = find_commands_dir(temp.path());
        assert_eq!(found, None);
    }

    #[test]
    fn discover_basic_scripts() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        // Create two scripts at root level
        create_executable(
            &commands_dir.join("hello.sh"),
            "#!/bin/bash\necho hello\n",
        )
        .unwrap();
        create_executable(
            &commands_dir.join("world.sh"),
            "#!/bin/bash\necho world\n",
        )
        .unwrap();

        let tree = discover(&commands_dir).unwrap();

        // Check children
        let children = tree.children().unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.contains_key("hello"));
        assert!(children.contains_key("world"));

        // Check they're leaves
        match children.get("hello").unwrap() {
            CommandTree::Leaf(cmd) => assert_eq!(cmd.name, "hello"),
            _ => panic!("expected leaf"),
        }
    }

    #[test]
    fn discover_nested_groups() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        // Create db/ directory with migrate.sh
        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\necho migrate\n").unwrap();

        let tree = discover(&commands_dir).unwrap();

        // Check db group exists
        let db = tree.get("db").unwrap();
        match db {
            CommandTree::Group { name, children, .. } => {
                assert_eq!(name, "db");
                assert!(children.contains_key("migrate"));
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    fn discover_skips_hidden_files() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(
            &commands_dir.join("visible.sh"),
            "#!/bin/bash\necho visible\n",
        )
        .unwrap();
        create_executable(
            &commands_dir.join(".hidden.sh"),
            "#!/bin/bash\necho hidden\n",
        )
        .unwrap();

        let tree = discover(&commands_dir).unwrap();

        let children = tree.children().unwrap();
        assert_eq!(children.len(), 1);
        assert!(children.contains_key("visible"));
        assert!(!children.contains_key(".hidden"));
    }

    #[test]
    fn discover_skips_underscore_prefixed() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(
            &commands_dir.join("normal.sh"),
            "#!/bin/bash\necho normal\n",
        )
        .unwrap();
        create_executable(
            &commands_dir.join("_helper.sh"),
            "#!/bin/bash\necho helper\n",
        )
        .unwrap();

        let tree = discover(&commands_dir).unwrap();

        let children = tree.children().unwrap();
        assert_eq!(children.len(), 1);
        assert!(children.contains_key("normal"));
        assert!(!children.contains_key("_helper"));
    }

    #[test]
    fn discover_with_group_yml() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\necho migrate\n").unwrap();
        create_executable(&db_dir.join("seed.sh"), "#!/bin/bash\necho seed\n").unwrap();

        // Create _group.yml
        fs::write(
            db_dir.join("_group.yml"),
            "description: Database management commands\norder: [migrate, seed]\n",
        )
        .unwrap();

        let tree = discover(&commands_dir).unwrap();

        let db = tree.get("db").unwrap();
        match db {
            CommandTree::Group {
                description, order, ..
            } => {
                assert_eq!(
                    description.as_deref(),
                    Some("Database management commands")
                );
                assert_eq!(order.as_ref().unwrap(), &vec!["migrate", "seed"]);
            }
            _ => panic!("expected group"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn discover_warns_on_non_executable() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        // Create non-executable script
        let script_path = commands_dir.join("test.sh");
        fs::write(&script_path, "#!/bin/bash\necho test\n").unwrap();

        // Don't make it executable
        let tree = discover(&commands_dir).unwrap();

        // Should still be discovered
        let children = tree.children().unwrap();
        assert!(children.contains_key("test"));
    }

    #[test]
    fn discover_deeply_nested() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        // Create a/b/c/deep.sh
        let path = commands_dir.join("a").join("b").join("c");
        fs::create_dir_all(&path).unwrap();
        create_executable(&path.join("deep.sh"), "#!/bin/bash\necho deep\n").unwrap();

        let tree = discover(&commands_dir).unwrap();

        // Navigate tree
        let a = tree.get("a").unwrap();
        let b = a.get("b").unwrap();
        let c = b.get("c").unwrap();
        let deep = c.get("deep").unwrap();

        match deep {
            CommandTree::Leaf(cmd) => assert_eq!(cmd.name, "deep"),
            _ => panic!("expected leaf"),
        }
    }

    #[test]
    fn discover_with_frontmatter() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let script_content = r#"#!/bin/bash
#---
# description: Deploy the app
# args:
#   env:
#     description: Target environment
#     required: true
#---
echo "deploying to $ARG_ENV"
"#;
        create_executable(&commands_dir.join("deploy.sh"), script_content).unwrap();

        let tree = discover(&commands_dir).unwrap();

        let deploy = tree.get("deploy").unwrap();
        match deploy {
            CommandTree::Leaf(cmd) => {
                assert_eq!(cmd.description.as_deref(), Some("Deploy the app"));
                assert_eq!(cmd.args.len(), 1);
                assert_eq!(cmd.args[0].name, "env");
                assert!(cmd.args[0].required);
            }
            _ => panic!("expected leaf"),
        }
    }
}

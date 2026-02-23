use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Create a new command script from a template
///
/// # Arguments
/// * `commands_dir` - The .commands directory
/// * `cmd_path` - The command path (e.g., "deploy/rollback")
/// * `template` - Optional custom template from _config.yml
///
/// # Returns
/// The path to the created file
pub fn create_command(
    commands_dir: &Path,
    cmd_path: &str,
    template: Option<&str>,
) -> Result<PathBuf> {
    // Parse the command path
    let segments: Vec<&str> = cmd_path.split('/').collect();

    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        bail!("invalid command path: {}", cmd_path);
    }

    // Build the target path
    let cmd_name = segments.last().unwrap();
    let mut target_path = commands_dir.to_path_buf();

    // Add intermediate directories
    for segment in &segments[..segments.len() - 1] {
        target_path.push(segment);
    }

    // Add the script file name
    target_path.push(format!("{}.sh", cmd_name));

    // Check if file already exists
    if target_path.exists() {
        bail!("command already exists: {}", target_path.display());
    }

    // Create intermediate directories if needed
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)
            .context("failed to create intermediate directories")?;
    }

    // Get the template content
    let content = match template {
        Some(custom_template) => custom_template.to_string(),
        None => default_template(cmd_name),
    };

    // Write the template to the file
    fs::write(&target_path, content)
        .context("failed to write command file")?;

    // Make it executable (Unix only)
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&target_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target_path, perms)
            .context("failed to set executable permissions")?;
    }

    Ok(target_path)
}

/// Generate the default template for a new command
fn default_template(cmd_name: &str) -> String {
    format!(
        r#"#!/bin/bash
#---
# description: TODO describe this command
#---

echo "TODO: implement {}"
"#,
        cmd_name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_simple_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let result = create_command(&commands_dir, "test", None).unwrap();

        // Check file exists
        assert!(result.exists());
        assert_eq!(result, commands_dir.join("test.sh"));

        // Check file is executable (Unix only)
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&result).unwrap();
            let permissions = metadata.permissions();
            assert!(permissions.mode() & 0o111 != 0); // At least one execute bit set
        }

        // Check content
        let content = fs::read_to_string(&result).unwrap();
        assert!(content.contains("#!/bin/bash"));
        assert!(content.contains("description: TODO describe this command"));
        assert!(content.contains("echo \"TODO: implement test\""));
    }

    #[test]
    fn test_create_nested_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let result = create_command(&commands_dir, "db/reset", None).unwrap();

        // Check file exists in nested directory
        assert!(result.exists());
        assert_eq!(result, commands_dir.join("db").join("reset.sh"));

        // Check intermediate directory was created
        assert!(commands_dir.join("db").is_dir());

        // Check content has correct command name
        let content = fs::read_to_string(&result).unwrap();
        assert!(content.contains("echo \"TODO: implement reset\""));
    }

    #[test]
    fn test_create_with_custom_template() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let custom_template = r#"#!/bin/zsh
# Custom template
echo "custom"
"#;

        let result = create_command(&commands_dir, "custom", Some(custom_template)).unwrap();

        // Check file exists
        assert!(result.exists());

        // Check content matches custom template
        let content = fs::read_to_string(&result).unwrap();
        assert_eq!(content, custom_template);
    }

    #[test]
    fn test_refuse_to_overwrite_existing() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        // Create the command first
        let existing = commands_dir.join("existing.sh");
        fs::write(&existing, "existing content").unwrap();

        // Try to create it again
        let result = create_command(&commands_dir, "existing", None);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));

        // Check original content wasn't overwritten
        let content = fs::read_to_string(&existing).unwrap();
        assert_eq!(content, "existing content");
    }

    #[test]
    fn test_default_template_contains_command_name() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let result = create_command(&commands_dir, "deploy/production", None).unwrap();

        let content = fs::read_to_string(&result).unwrap();
        // The command name in the TODO should be the last segment
        assert!(content.contains("echo \"TODO: implement production\""));
    }
}

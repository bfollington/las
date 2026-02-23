use crate::args::ParsedArgs;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Collect all _hooks.sh files from root to the command's directory
///
/// Given the commands dir and the command's script path relative to it,
/// returns paths to _hooks.sh files in order from root to deepest.
///
/// Example:
///   commands_dir: /project/.commands
///   cmd_relative_path: db/migrate.sh
///   Returns: [/project/.commands/_hooks.sh, /project/.commands/db/_hooks.sh]
///            (only if those files exist)
pub fn collect_hooks(commands_dir: &Path, cmd_relative_path: &Path) -> Vec<PathBuf> {
    let mut hooks = Vec::new();

    // Start with root _hooks.sh
    let root_hooks = commands_dir.join("_hooks.sh");
    if root_hooks.exists() {
        hooks.push(root_hooks);
    }

    // Walk up the command's directory hierarchy
    if let Some(parent) = cmd_relative_path.parent() {
        let mut current = PathBuf::new();
        for component in parent.components() {
            current.push(component);
            let hooks_path = commands_dir.join(&current).join("_hooks.sh");
            if hooks_path.exists() {
                hooks.push(hooks_path);
            }
        }
    }

    hooks
}

/// Run before hooks for a command
///
/// For each hooks file (in order), source it and call before().
/// If any before() exits nonzero, return Some(exit_code) to signal
/// the command should be skipped. If all succeed, return None.
pub fn run_before_hooks(
    hooks: &[PathBuf],
    parsed: &ParsedArgs,
    shell: &str,
) -> Result<Option<i32>> {
    for hooks_path in hooks {
        let mut cmd = Command::new(shell);
        cmd.arg("-c")
            .arg(format!(
                "source '{}' && before",
                hooks_path.display()
            ));

        // Set the same environment variables as the command would receive
        for (key, value) in &parsed.env_args {
            cmd.env(key, value);
        }
        for (key, value) in &parsed.env_flags {
            cmd.env(key, value);
        }

        let status = cmd.status()?;

        if let Some(code) = status.code() {
            if code != 0 {
                // Before hook failed, skip the command
                return Ok(Some(code));
            }
        } else {
            // Process was killed by signal
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt;
                if let Some(signal) = status.signal() {
                    return Ok(Some(128 + signal));
                }
            }
            return Ok(Some(1));
        }
    }

    // All before hooks succeeded
    Ok(None)
}

/// Run after hooks for a command
///
/// For each hooks file (in order), source it and call after() with the
/// command's exit code. After hooks run regardless of command exit code.
/// Their exit codes are ignored.
pub fn run_after_hooks(
    hooks: &[PathBuf],
    parsed: &ParsedArgs,
    shell: &str,
    exit_code: i32,
) -> Result<()> {
    for hooks_path in hooks {
        let mut cmd = Command::new(shell);
        cmd.arg("-c")
            .arg(format!(
                "source '{}' && after {}",
                hooks_path.display(),
                exit_code
            ));

        // Set the same environment variables as the command received
        for (key, value) in &parsed.env_args {
            cmd.env(key, value);
        }
        for (key, value) in &parsed.env_flags {
            cmd.env(key, value);
        }

        // Run the hook, but ignore its exit code
        let _ = cmd.status();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn create_hooks_file(path: &Path, content: &str) {
        fs::write(path, content).unwrap();
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).unwrap();
        }
    }

    #[test]
    fn test_collect_hooks_finds_root() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let root_hooks = commands_dir.join("_hooks.sh");
        create_hooks_file(&root_hooks, "#!/bin/bash\nbefore() { exit 0; }\n");

        let cmd_path = Path::new("test.sh");
        let hooks = collect_hooks(&commands_dir, cmd_path);

        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0], root_hooks);
    }

    #[test]
    fn test_collect_hooks_finds_nested() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let root_hooks = commands_dir.join("_hooks.sh");
        create_hooks_file(&root_hooks, "#!/bin/bash\nbefore() { exit 0; }\n");

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        let db_hooks = db_dir.join("_hooks.sh");
        create_hooks_file(&db_hooks, "#!/bin/bash\nbefore() { exit 0; }\n");

        let cmd_path = Path::new("db/migrate.sh");
        let hooks = collect_hooks(&commands_dir, cmd_path);

        assert_eq!(hooks.len(), 2);
        assert_eq!(hooks[0], root_hooks);
        assert_eq!(hooks[1], db_hooks);
    }

    #[test]
    fn test_collect_hooks_empty_when_none() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let cmd_path = Path::new("test.sh");
        let hooks = collect_hooks(&commands_dir, cmd_path);

        assert_eq!(hooks.len(), 0);
    }

    #[test]
    fn test_before_hook_success() {
        let temp = TempDir::new().unwrap();
        let hooks_path = temp.path().join("_hooks.sh");
        create_hooks_file(
            &hooks_path,
            "#!/bin/bash\nbefore() { exit 0; }\nafter() { exit 0; }\n",
        );

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec![],
        };

        let result = run_before_hooks(&[hooks_path], &parsed, "bash").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_before_hook_failure_skips_command() {
        let temp = TempDir::new().unwrap();
        let hooks_path = temp.path().join("_hooks.sh");
        create_hooks_file(
            &hooks_path,
            "#!/bin/bash\nbefore() { exit 1; }\nafter() { exit 0; }\n",
        );

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec![],
        };

        let result = run_before_hooks(&[hooks_path], &parsed, "bash").unwrap();
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_after_hook_runs_with_exit_code() {
        let temp = TempDir::new().unwrap();
        let hooks_path = temp.path().join("_hooks.sh");
        create_hooks_file(
            &hooks_path,
            "#!/bin/bash\nbefore() { exit 0; }\nafter() { exit 0; }\n",
        );

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec![],
        };

        // Should not error even if we pass a non-zero exit code
        let result = run_after_hooks(&[hooks_path], &parsed, "bash", 42);
        assert!(result.is_ok());
    }

    #[test]
    fn test_hooks_receive_env_vars() {
        let temp = TempDir::new().unwrap();
        let test_file = temp.path().join("test_output.txt");
        let hooks_path = temp.path().join("_hooks.sh");

        // Hook that writes ARG_NAME to a file
        create_hooks_file(
            &hooks_path,
            &format!(
                "#!/bin/bash\nbefore() {{ echo \"$ARG_NAME\" > '{}'; exit 0; }}\nafter() {{ exit 0; }}\n",
                test_file.display()
            ),
        );

        let mut env_args = HashMap::new();
        env_args.insert("ARG_NAME".to_string(), "test_value".to_string());

        let parsed = ParsedArgs {
            env_args,
            env_flags: HashMap::new(),
            positional: vec![],
        };

        run_before_hooks(&[hooks_path], &parsed, "bash").unwrap();

        // Verify the env var was passed
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content.trim(), "test_value");
    }

    #[test]
    fn test_multiple_before_hooks_in_order() {
        let temp = TempDir::new().unwrap();
        let output_file = temp.path().join("output.txt");

        let hooks1 = temp.path().join("_hooks1.sh");
        create_hooks_file(
            &hooks1,
            &format!(
                "#!/bin/bash\nbefore() {{ echo 'first' >> '{}'; exit 0; }}\nafter() {{ exit 0; }}\n",
                output_file.display()
            ),
        );

        let hooks2 = temp.path().join("_hooks2.sh");
        create_hooks_file(
            &hooks2,
            &format!(
                "#!/bin/bash\nbefore() {{ echo 'second' >> '{}'; exit 0; }}\nafter() {{ exit 0; }}\n",
                output_file.display()
            ),
        );

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec![],
        };

        run_before_hooks(&[hooks1, hooks2], &parsed, "bash").unwrap();

        let content = fs::read_to_string(&output_file).unwrap();
        assert_eq!(content, "first\nsecond\n");
    }

    #[test]
    fn test_first_before_hook_failure_stops_chain() {
        let temp = TempDir::new().unwrap();
        let output_file = temp.path().join("output.txt");

        let hooks1 = temp.path().join("_hooks1.sh");
        create_hooks_file(
            &hooks1,
            &format!(
                "#!/bin/bash\nbefore() {{ echo 'first' >> '{}'; exit 1; }}\nafter() {{ exit 0; }}\n",
                output_file.display()
            ),
        );

        let hooks2 = temp.path().join("_hooks2.sh");
        create_hooks_file(
            &hooks2,
            &format!(
                "#!/bin/bash\nbefore() {{ echo 'second' >> '{}'; exit 0; }}\nafter() {{ exit 0; }}\n",
                output_file.display()
            ),
        );

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec![],
        };

        let result = run_before_hooks(&[hooks1, hooks2], &parsed, "bash").unwrap();
        assert_eq!(result, Some(1));

        // Only first hook should have run
        let content = fs::read_to_string(&output_file).unwrap();
        assert_eq!(content, "first\n");
    }
}

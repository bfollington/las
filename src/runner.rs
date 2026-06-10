use crate::args::ParsedArgs;
use crate::command::CommandDef;
use anyhow::{Context, Result};
use std::process::{Command, Stdio};

/// Run a script command with the given parsed arguments
///
/// Returns the exit code of the script:
/// - 0-255: normal exit codes
/// - 128 + signal_number: if killed by signal (Unix)
/// - 1: if killed by signal but signal number unavailable
pub fn run(cmd: &CommandDef, parsed: &ParsedArgs, shell: &str) -> Result<i32> {
    let mut process = Command::new(shell);

    // Add the script path as the first argument
    process.arg(&cmd.script_path);

    // Add positional arguments (they become $1, $2, etc. in the script)
    for arg in &parsed.positional {
        process.arg(arg);
    }

    // Set environment variables for named arguments (ARG_*)
    for (key, value) in &parsed.env_args {
        process.env(key, value);
    }

    // Set environment variables for flags (FLAG_*)
    for (key, value) in &parsed.env_flags {
        process.env(key, value);
    }

    // Inherit stdin, stdout, stderr for transparent passthrough
    process.stdin(Stdio::inherit());
    process.stdout(Stdio::inherit());
    process.stderr(Stdio::inherit());

    // Spawn and wait for the process
    let status = process.status().context("Failed to execute script")?;

    // Return the exit code
    if let Some(code) = status.code() {
        Ok(code)
    } else {
        // Process was killed by a signal (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                Ok(128 + signal)
            } else {
                Ok(1)
            }
        }
        #[cfg(not(unix))]
        {
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{ArgDef, FlagDef};
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn setup_test_script(dir: &TempDir, script_content: &str) -> PathBuf {
        let script_path = dir.path().join("test_script.sh");
        fs::write(&script_path, script_content).unwrap();

        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }

        script_path
    }

    #[test]
    fn test_basic_execution_success() {
        let temp_dir = TempDir::new().unwrap();
        let script_content = "#!/bin/bash\nexit 0";
        let script_path = setup_test_script(&temp_dir, script_content);

        let cmd = CommandDef {
            name: "test".into(),
            description: None,
            script_path,
            args: vec![],
            flags: vec![],
            ..Default::default()
        };

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec![],
        };

        let exit_code = run(&cmd, &parsed, "bash").unwrap();
        assert_eq!(exit_code, 0);
    }

    #[test]
    fn test_exit_code_forwarding() {
        let temp_dir = TempDir::new().unwrap();
        let script_content = "#!/bin/bash\nexit 42";
        let script_path = setup_test_script(&temp_dir, script_content);

        let cmd = CommandDef {
            name: "test".into(),
            description: None,
            script_path,
            args: vec![],
            flags: vec![],
            ..Default::default()
        };

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec![],
        };

        let exit_code = run(&cmd, &parsed, "bash").unwrap();
        assert_eq!(exit_code, 42);
    }

    #[test]
    fn test_env_args_passed_to_script() {
        let temp_dir = TempDir::new().unwrap();
        // Script exits 0 if ARG_NAME is set to "test_value", otherwise exits 1
        let script_content = r#"#!/bin/bash
if [ "$ARG_NAME" = "test_value" ]; then
    exit 0
else
    exit 1
fi
"#;
        let script_path = setup_test_script(&temp_dir, script_content);

        let cmd = CommandDef {
            name: "test".into(),
            description: None,
            script_path,
            args: vec![ArgDef {
                name: "name".into(),
                description: None,
                required: true,
                variadic: false,
                default: None,
                choices: None,
            }],
            flags: vec![],
            ..Default::default()
        };

        let mut env_args = HashMap::new();
        env_args.insert("ARG_NAME".to_string(), "test_value".to_string());

        let parsed = ParsedArgs {
            env_args,
            env_flags: HashMap::new(),
            positional: vec!["test_value".to_string()],
        };

        let exit_code = run(&cmd, &parsed, "bash").unwrap();
        assert_eq!(exit_code, 0);
    }

    #[test]
    fn test_env_flags_passed_to_script() {
        let temp_dir = TempDir::new().unwrap();
        // Script exits 0 if FLAG_VERBOSE is "true", otherwise exits 1
        let script_content = r#"#!/bin/bash
if [ "$FLAG_VERBOSE" = "true" ]; then
    exit 0
else
    exit 1
fi
"#;
        let script_path = setup_test_script(&temp_dir, script_content);

        let cmd = CommandDef {
            name: "test".into(),
            description: None,
            script_path,
            args: vec![],
            flags: vec![FlagDef {
                name: "verbose".into(),
                description: None,
                short: Some('v'),
                value: None,
                default: None,
                choices: None,
            }],
            ..Default::default()
        };

        let mut env_flags = HashMap::new();
        env_flags.insert("FLAG_VERBOSE".to_string(), "true".to_string());

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags,
            positional: vec![],
        };

        let exit_code = run(&cmd, &parsed, "bash").unwrap();
        assert_eq!(exit_code, 0);
    }

    #[test]
    fn test_positional_args_as_script_params() {
        let temp_dir = TempDir::new().unwrap();
        // Script checks if $1 = "first" and $2 = "second"
        let script_content = r#"#!/bin/bash
if [ "$1" = "first" ] && [ "$2" = "second" ]; then
    exit 0
else
    exit 1
fi
"#;
        let script_path = setup_test_script(&temp_dir, script_content);

        let cmd = CommandDef {
            name: "test".into(),
            description: None,
            script_path,
            args: vec![],
            flags: vec![],
            ..Default::default()
        };

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags: HashMap::new(),
            positional: vec!["first".to_string(), "second".to_string()],
        };

        let exit_code = run(&cmd, &parsed, "bash").unwrap();
        assert_eq!(exit_code, 0);
    }

    #[test]
    fn test_boolean_flag_false() {
        let temp_dir = TempDir::new().unwrap();
        // Script exits 0 if FLAG_VERBOSE is "false", otherwise exits 1
        let script_content = r#"#!/bin/bash
if [ "$FLAG_VERBOSE" = "false" ]; then
    exit 0
else
    exit 1
fi
"#;
        let script_path = setup_test_script(&temp_dir, script_content);

        let cmd = CommandDef {
            name: "test".into(),
            description: None,
            script_path,
            args: vec![],
            flags: vec![FlagDef {
                name: "verbose".into(),
                description: None,
                short: Some('v'),
                value: None,
                default: None,
                choices: None,
            }],
            ..Default::default()
        };

        let mut env_flags = HashMap::new();
        env_flags.insert("FLAG_VERBOSE".to_string(), "false".to_string());

        let parsed = ParsedArgs {
            env_args: HashMap::new(),
            env_flags,
            positional: vec![],
        };

        let exit_code = run(&cmd, &parsed, "bash").unwrap();
        assert_eq!(exit_code, 0);
    }
}

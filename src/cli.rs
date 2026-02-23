use crate::args::{parse_args, ArgsError};
use crate::command::CommandTree;
use crate::discovery::{discover, find_commands_dir};
use crate::help;
use crate::hooks;
use crate::runner;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
struct Config {
    shell: Option<String>,
    name: Option<String>,
    template: Option<String>,
}

/// Main CLI entry point (reads from std::env::args)
pub fn run() -> Result<i32> {
    let args: Vec<String> = env::args().skip(1).collect();
    run_with_args(&args)
}

/// Run with explicit args (for testing)
pub fn run_with_args(args: &[String]) -> Result<i32> {
    // Find the .commands directory
    let commands_dir = find_commands_dir(&env::current_dir()?)
        .context("No .commands directory found. Run this from a project with a .commands/ directory.")?;

    run_with_context(args, &commands_dir)
}

/// Run with explicit args and commands directory (for testing)
pub fn run_with_context(args: &[String], commands_dir: &Path) -> Result<i32> {
    // Load config
    let config = load_config(commands_dir)?;
    let shell = config.shell.as_deref().unwrap_or("/bin/bash");

    // If no args, show top-level help
    if args.is_empty() {
        let tree = discover(commands_dir)?;
        let name = config.name.as_deref().unwrap_or("las");
        help::print_top_level_help(&tree, commands_dir, name);
        return Ok(0);
    }

    let first_arg = &args[0];

    // Handle meta-commands (flags that start with --)
    if first_arg.starts_with("--") {
        return handle_meta_command(first_arg, &args[1..], commands_dir);
    }

    // Otherwise, resolve command name(s) by walking the CommandTree
    let tree = discover(commands_dir)?;

    // Consume args from left to resolve command path
    let mut current_tree = &tree;
    let mut consumed = 0;
    let mut cmd_path_parts = Vec::new();

    for (idx, arg) in args.iter().enumerate() {
        // Check if this looks like a flag or we've hit the help flag
        if arg.starts_with("-") {
            break;
        }

        // Try to resolve this as a subcommand
        if let Some(child) = current_tree.get(arg) {
            current_tree = child;
            cmd_path_parts.push(arg.clone());
            consumed = idx + 1;
        } else {
            // Not a valid subcommand, stop here
            break;
        }
    }

    // Get remaining args after command name(s)
    let remaining: Vec<String> = args[consumed..].to_vec();

    // If we didn't consume any args, command not found
    if consumed == 0 {
        eprintln!("las: command not found: {}", first_arg);
        return Ok(127);
    }

    let cmd_path = cmd_path_parts.join(" ");
    let name = config.name.as_deref().unwrap_or("las");

    // Check what we resolved to
    match current_tree {
        CommandTree::Group { name: _group_name, description, children, order } => {
            // If remaining args contain --help, show group help
            if remaining.iter().any(|a| a == "--help") {
                help::print_group_help(name, &cmd_path, description.as_deref(), children, order.as_deref());
                return Ok(0);
            }

            // Otherwise, show group help (user didn't specify a subcommand)
            help::print_group_help(name, &cmd_path, description.as_deref(), children, order.as_deref());
            Ok(0)
        }
        CommandTree::Leaf(cmd) => {
            // Check if --help is in remaining args
            if remaining.iter().any(|a| a == "--help") {
                help::print_command_help(cmd, &cmd_path, name);
                return Ok(0);
            }

            // Parse args and run command
            match parse_args(cmd, &remaining) {
                Ok(parsed) => {
                    // Collect hooks
                    let rel_path = cmd.script_path.strip_prefix(commands_dir).unwrap_or(&cmd.script_path);
                    let hook_files = hooks::collect_hooks(commands_dir, rel_path);

                    // Run before hooks
                    if let Some(code) = hooks::run_before_hooks(&hook_files, &parsed, shell)? {
                        return Ok(code);
                    }

                    // Run command
                    let exit_code = runner::run(cmd, &parsed, shell)?;

                    // Run after hooks
                    hooks::run_after_hooks(&hook_files, &parsed, shell, exit_code)?;

                    Ok(exit_code)
                }
                Err(e) => {
                    handle_args_error(e, cmd, name);
                    Ok(2)
                }
            }
        }
    }
}

/// Load configuration from .commands/_config.yml
fn load_config(commands_dir: &Path) -> Result<Config> {
    let config_path = commands_dir.join("_config.yml");

    if !config_path.exists() {
        return Ok(Config::default());
    }

    let content = fs::read_to_string(&config_path)
        .context("failed to read _config.yml")?;

    let config: Config = serde_yaml::from_str(&content)
        .context("failed to parse _config.yml")?;

    Ok(config)
}

/// Handle meta-commands like --help, --version, etc.
fn handle_meta_command(flag: &str, remaining: &[String], commands_dir: &Path) -> Result<i32> {
    let config = load_config(commands_dir)?;
    let name = config.name.as_deref().unwrap_or("las");

    match flag {
        "--help" => {
            let tree = discover(commands_dir)?;
            help::print_top_level_help(&tree, commands_dir, name);
            Ok(0)
        }
        "--version" => {
            println!("las {}", env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
        "--list" => {
            let tree = discover(commands_dir)?;
            print_tree(&tree, 0);
            Ok(0)
        }
        "--which" => {
            if remaining.is_empty() {
                eprintln!("las: --which requires a command name");
                return Ok(2);
            }
            let tree = discover(commands_dir)?;
            match resolve_command(&tree, remaining) {
                Some(CommandTree::Leaf(cmd)) => {
                    println!("{}", cmd.script_path.display());
                    Ok(0)
                }
                Some(CommandTree::Group { name, .. }) => {
                    eprintln!("las: {} is a group, not a command", name);
                    Ok(2)
                }
                None => {
                    eprintln!("las: command not found: {}", remaining.join(" "));
                    Ok(127)
                }
            }
        }
        "--edit" => {
            if remaining.is_empty() {
                eprintln!("las: --edit requires a command name");
                return Ok(2);
            }
            let tree = discover(commands_dir)?;
            match resolve_command(&tree, remaining) {
                Some(CommandTree::Leaf(cmd)) => {
                    let editor = env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
                    let status = std::process::Command::new(&editor)
                        .arg(&cmd.script_path)
                        .status()
                        .context(format!("failed to launch editor: {}", editor))?;
                    Ok(status.code().unwrap_or(1))
                }
                Some(CommandTree::Group { name, .. }) => {
                    eprintln!("las: {} is a group, not a command", name);
                    Ok(2)
                }
                None => {
                    eprintln!("las: command not found: {}", remaining.join(" "));
                    Ok(127)
                }
            }
        }
        "--new" => {
            if remaining.is_empty() {
                eprintln!("las: --new requires a command name");
                return Ok(2);
            }

            let cmd_path = &remaining[0];
            let config = load_config(commands_dir)?;

            match crate::new::create_command(commands_dir, cmd_path, config.template.as_deref()) {
                Ok(created_path) => {
                    // Calculate the relative path from commands_dir
                    let relative = created_path.strip_prefix(commands_dir)
                        .unwrap_or(&created_path);
                    println!("Created {} (chmod +x)", relative.display());
                    Ok(0)
                }
                Err(e) => {
                    eprintln!("las: {}", e);
                    Ok(2)
                }
            }
        }
        "--skill" => {
            let tree = discover(commands_dir)?;
            let skill_doc = crate::skill::generate_skill(&tree, commands_dir, name);
            print!("{}", skill_doc);
            Ok(0)
        }
        "--completions" => {
            if remaining.is_empty() {
                eprintln!("las: --completions requires a shell name (bash, zsh, fish)");
                return Ok(2);
            }
            let tree = discover(commands_dir)?;
            match crate::completions::generate_completions(&remaining[0], &tree, name) {
                Ok(script) => {
                    print!("{}", script);
                    Ok(0)
                }
                Err(e) => {
                    eprintln!("las: {}", e);
                    Ok(2)
                }
            }
        }
        "--config" => {
            let config = load_config(commands_dir)?;
            let shell = config.shell.as_deref().unwrap_or("/bin/bash");
            let name = config.name.as_deref().unwrap_or("las");
            let tree = discover(commands_dir)?;
            let num_commands = count_commands(&tree);

            println!("Commands directory: {}", commands_dir.display());
            println!("Shell: {}", shell);
            println!("Name: {}", name);
            println!("Commands: {}", num_commands);
            Ok(0)
        }
        _ => {
            eprintln!("las: unknown flag: {}", flag);
            Ok(2)
        }
    }
}

/// Recursively print the command tree with indentation
fn print_tree(tree: &CommandTree, indent: usize) {
    if let Some(children) = tree.children() {
        for (name, child) in children {
            let description = match child {
                CommandTree::Leaf(cmd) => cmd.description.as_deref().unwrap_or(""),
                CommandTree::Group { description, .. } => description.as_deref().unwrap_or(""),
            };
            let spaces = "  ".repeat(indent);
            println!("{}{:<16}{}", spaces, name, description);

            // Recursively print children of groups
            if let CommandTree::Group { .. } = child {
                print_tree(child, indent + 1);
            }
        }
    }
}

/// Resolve a command path through the tree
fn resolve_command<'a>(tree: &'a CommandTree, path: &[String]) -> Option<&'a CommandTree> {
    let mut current = tree;

    for segment in path {
        match current.get(segment) {
            Some(child) => current = child,
            None => return None,
        }
    }

    Some(current)
}

/// Count the total number of leaf commands in the tree
fn count_commands(tree: &CommandTree) -> usize {
    match tree {
        CommandTree::Leaf(_) => 1,
        CommandTree::Group { children, .. } => {
            children.values().map(count_commands).sum()
        }
    }
}

/// Handle argument parsing errors
fn handle_args_error(err: ArgsError, cmd: &crate::command::CommandDef, name: &str) {
    eprintln!("{}: {}", name, err);
    eprintln!();
    eprintln!("Try '{} {} --help' for more information.", name, cmd.name);
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
    fn test_no_args_shows_help() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(
            &commands_dir.join("test.sh"),
            "#!/bin/bash\necho test\n",
        ).unwrap();

        let args = vec![];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_command_not_found() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let args = vec!["nonexistent".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 127);
    }

    #[test]
    fn test_command_help_flag() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let script_content = r#"#!/bin/bash
#---
# description: Test command
# args:
#   name:
#     description: A name
#     required: true
#---
echo "hello $ARG_NAME"
"#;
        create_executable(&commands_dir.join("greet.sh"), script_content).unwrap();

        let args = vec!["greet".to_string(), "--help".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_run_simple_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(
            &commands_dir.join("test.sh"),
            "#!/bin/bash\nexit 0\n",
        ).unwrap();

        let args = vec!["test".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_run_nested_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();

        create_executable(
            &db_dir.join("migrate.sh"),
            "#!/bin/bash\nexit 0\n",
        ).unwrap();

        let args = vec!["db".to_string(), "migrate".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_group_shows_help() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();

        create_executable(
            &db_dir.join("migrate.sh"),
            "#!/bin/bash\necho migrate\n",
        ).unwrap();

        // Just "db" without subcommand should show group help
        let args = vec!["db".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_missing_required_arg() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let script_content = r#"#!/bin/bash
#---
# args:
#   name:
#     required: true
#---
echo "hello $ARG_NAME"
"#;
        create_executable(&commands_dir.join("greet.sh"), script_content).unwrap();

        // Run without required arg
        let args = vec!["greet".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 2); // ArgsError exit code
    }

    #[test]
    fn test_top_level_help_flag() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(
            &commands_dir.join("test.sh"),
            "#!/bin/bash\necho test\n",
        ).unwrap();

        let args = vec!["--help".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_version_flag() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let args = vec!["--version".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_load_config_with_shell() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let config_content = "shell: /bin/zsh\nname: mycli\n";
        fs::write(commands_dir.join("_config.yml"), config_content).unwrap();

        let config = load_config(&commands_dir).unwrap();
        assert_eq!(config.shell.as_deref(), Some("/bin/zsh"));
        assert_eq!(config.name.as_deref(), Some("mycli"));
    }

    #[test]
    fn test_load_config_default() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        // No _config.yml file
        let config = load_config(&commands_dir).unwrap();
        assert!(config.shell.is_none());
        assert!(config.name.is_none());
    }

    #[test]
    fn test_list_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        // Create some commands
        create_executable(&commands_dir.join("test.sh"), "#!/bin/bash\necho test\n").unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\necho migrate\n").unwrap();

        let args = vec!["--list".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_which_simple_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(&commands_dir.join("deploy.sh"), "#!/bin/bash\necho deploy\n").unwrap();

        let args = vec!["--which".to_string(), "deploy".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_which_nested_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\necho migrate\n").unwrap();

        let args = vec!["--which".to_string(), "db".to_string(), "migrate".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_which_nonexistent() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let args = vec!["--which".to_string(), "nonexistent".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 127);
    }

    #[test]
    fn test_which_group_not_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\necho migrate\n").unwrap();

        // Try to --which on a group
        let args = vec!["--which".to_string(), "db".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 2);
    }

    #[test]
    fn test_config_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(&commands_dir.join("test.sh"), "#!/bin/bash\necho test\n").unwrap();

        let args = vec!["--config".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_resolve_command_helper() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\necho migrate\n").unwrap();

        let tree = discover(&commands_dir).unwrap();

        // Test resolving nested command
        let path = vec!["db".to_string(), "migrate".to_string()];
        let resolved = resolve_command(&tree, &path);
        assert!(resolved.is_some());

        match resolved.unwrap() {
            CommandTree::Leaf(cmd) => assert_eq!(cmd.name, "migrate"),
            _ => panic!("expected leaf"),
        }

        // Test resolving group
        let path = vec!["db".to_string()];
        let resolved = resolve_command(&tree, &path);
        assert!(resolved.is_some());

        match resolved.unwrap() {
            CommandTree::Group { name, .. } => assert_eq!(name, "db"),
            _ => panic!("expected group"),
        }

        // Test non-existent path
        let path = vec!["nonexistent".to_string()];
        let resolved = resolve_command(&tree, &path);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_count_commands_helper() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        create_executable(&commands_dir.join("test1.sh"), "#!/bin/bash\necho test1\n").unwrap();
        create_executable(&commands_dir.join("test2.sh"), "#!/bin/bash\necho test2\n").unwrap();

        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\necho migrate\n").unwrap();

        let tree = discover(&commands_dir).unwrap();
        let count = count_commands(&tree);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_before_hook_runs_and_passes_env_vars() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let marker = temp.path().join("before_marker.txt");

        // Create a hooks file that writes ARG_NAME to a marker file
        let hooks_content = format!(
            r#"#!/bin/bash
before() {{
  echo "$ARG_NAME" > '{}'
  exit 0
}}
after() {{
  exit 0
}}
"#,
            marker.display()
        );
        create_executable(&commands_dir.join("_hooks.sh"), &hooks_content).unwrap();

        // Create a command that takes an arg
        let script_content = r#"#!/bin/bash
#---
# args:
#   name:
#     required: true
#---
exit 0
"#;
        create_executable(&commands_dir.join("test.sh"), script_content).unwrap();

        // Run the command
        let args = vec!["test".to_string(), "myvalue".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);

        // Check that the hook ran and received the env var
        let content = fs::read_to_string(&marker).unwrap();
        assert_eq!(content.trim(), "myvalue");
    }

    #[test]
    fn test_before_hook_failure_skips_command() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let command_marker = temp.path().join("command_ran.txt");

        // Create a hooks file that exits with code 1
        let hooks_content = r#"#!/bin/bash
before() {
  exit 1
}
after() {
  exit 0
}
"#;
        create_executable(&commands_dir.join("_hooks.sh"), hooks_content).unwrap();

        // Create a command that writes a marker file
        let script_content = format!(
            r#"#!/bin/bash
echo "command ran" > '{}'
exit 0
"#,
            command_marker.display()
        );
        create_executable(&commands_dir.join("test.sh"), &script_content).unwrap();

        // Run the command
        let args = vec!["test".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 1); // Hook exit code

        // Command should not have run
        assert!(!command_marker.exists());
    }

    #[test]
    fn test_after_hook_runs_with_command_exit_code() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let marker = temp.path().join("after_marker.txt");

        // Create a hooks file that writes the command exit code to a marker file
        let hooks_content = format!(
            r#"#!/bin/bash
before() {{
  exit 0
}}
after() {{
  echo "$1" > '{}'
  exit 0
}}
"#,
            marker.display()
        );
        create_executable(&commands_dir.join("_hooks.sh"), &hooks_content).unwrap();

        // Create a command that exits with code 42
        create_executable(&commands_dir.join("test.sh"), "#!/bin/bash\nexit 42\n").unwrap();

        // Run the command
        let args = vec!["test".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 42);

        // Check that the after hook received the exit code
        let content = fs::read_to_string(&marker).unwrap();
        assert_eq!(content.trim(), "42");
    }

    #[test]
    fn test_nested_hooks_run_in_order() {
        let temp = TempDir::new().unwrap();
        let commands_dir = temp.path().join(".commands");
        fs::create_dir(&commands_dir).unwrap();

        let marker = temp.path().join("hooks_order.txt");

        // Root hooks file
        let root_hooks = format!(
            r#"#!/bin/bash
before() {{
  echo "root" >> '{}'
  exit 0
}}
after() {{
  exit 0
}}
"#,
            marker.display()
        );
        create_executable(&commands_dir.join("_hooks.sh"), &root_hooks).unwrap();

        // Nested db/ directory with hooks
        let db_dir = commands_dir.join("db");
        fs::create_dir(&db_dir).unwrap();
        let db_hooks = format!(
            r#"#!/bin/bash
before() {{
  echo "db" >> '{}'
  exit 0
}}
after() {{
  exit 0
}}
"#,
            marker.display()
        );
        create_executable(&db_dir.join("_hooks.sh"), &db_hooks).unwrap();

        // Command in db/ directory
        create_executable(&db_dir.join("migrate.sh"), "#!/bin/bash\nexit 0\n").unwrap();

        // Run the nested command
        let args = vec!["db".to_string(), "migrate".to_string()];
        let result = run_with_context(&args, &commands_dir).unwrap();
        assert_eq!(result, 0);

        // Check that both hooks ran in order
        let content = fs::read_to_string(&marker).unwrap();
        assert_eq!(content, "root\ndb\n");
    }
}

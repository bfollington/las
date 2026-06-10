use crate::command::{CommandDef, CommandTree};
use std::collections::BTreeMap;
use std::path::Path;

/// Format top-level help output
pub fn format_top_level_help(tree: &CommandTree, commands_dir: &Path, name: &str) -> String {
    let mut output = String::new();

    output.push_str(&format!("{} — project commands\n\n", name));

    // COMMANDS section
    if let Some(children) = tree.children()
        && !children.is_empty()
    {
        output.push_str("COMMANDS\n");
        format_command_list(&mut output, children, 2);
        output.push('\n');
    }

    // META section
    output.push_str("META\n");
    output.push_str("  --help              Show this help\n");
    output.push_str("  --list              List all commands (tree)\n");
    output.push_str("  --which <cmd>       Show file path for a command\n");
    output.push_str("  --edit <cmd>        Open command in $EDITOR\n");
    output.push_str("  --new <cmd>         Create a new command\n");
    output.push_str("  --skill             Print agent skill document\n");
    output.push_str("  --completions <sh>  Generate shell completions\n");
    output.push_str("  --config            Show configuration\n");
    output.push('\n');

    // Footer with helpful hints
    output.push_str("This CLI is extensible. Commands live in:\n");
    output.push_str(&format!("  {}\n\n", commands_dir.display()));
    output.push_str(&format!("Create new commands: {} --new <name>\n", name));
    output.push_str(&format!("Agent skill document: {} --skill\n", name));

    output
}

/// Print top-level help
pub fn print_top_level_help(tree: &CommandTree, commands_dir: &Path, name: &str) {
    print!("{}", format_top_level_help(tree, commands_dir, name));
}

/// Recursively format command list with groups and their children
fn format_command_list(
    output: &mut String,
    children: &BTreeMap<String, CommandTree>,
    indent: usize,
) {
    // Calculate max name width for alignment
    let max_width = children.keys().map(|k| k.len()).max().unwrap_or(16).max(16);

    for (name, child) in children {
        let indent_str = " ".repeat(indent);

        match child {
            CommandTree::Leaf(cmd) => {
                let description = cmd.description.as_deref().unwrap_or("");
                output.push_str(&format!(
                    "{}{:<width$}{}\n",
                    indent_str,
                    name,
                    description,
                    width = max_width
                ));
            }
            CommandTree::Group {
                description,
                children: group_children,
                ..
            } => {
                let desc = description.as_deref().unwrap_or("");
                output.push_str(&format!(
                    "{}{:<width$}{}\n",
                    indent_str,
                    name,
                    desc,
                    width = max_width
                ));

                // Recursively format children with more indentation
                if !group_children.is_empty() {
                    format_command_list(output, group_children, indent + 2);
                }
            }
        }
    }
}

/// Format command help output
pub fn format_command_help(cmd: &CommandDef, cmd_path: &str, name: &str) -> String {
    let mut output = String::new();

    // Title
    let description = cmd.description.as_deref().unwrap_or("");
    output.push_str(&format!("{} {} — {}\n\n", name, cmd_path, description));

    // USAGE section
    output.push_str("USAGE\n");
    output.push_str(&format!("  {} {}", name, cmd_path));

    // Add args to usage line
    for arg in &cmd.args {
        let ellipsis = if arg.variadic { "..." } else { "" };
        if arg.required {
            output.push_str(&format!(" <{}>{}", arg.name, ellipsis));
        } else {
            output.push_str(&format!(" [{}]{}", arg.name, ellipsis));
        }
    }

    // Add flags indicator if there are flags
    if !cmd.flags.is_empty() {
        output.push_str(" [flags]");
    }

    output.push('\n');

    // Free-form usage text from frontmatter (examples, caveats)
    if let Some(usage) = &cmd.usage {
        output.push('\n');
        for line in usage.trim_end().lines() {
            if line.trim().is_empty() {
                output.push('\n');
            } else {
                output.push_str(&format!("  {}\n", line));
            }
        }
    }

    output.push('\n');

    // ARGUMENTS section
    if !cmd.args.is_empty() {
        output.push_str("ARGUMENTS\n");
        for arg in &cmd.args {
            let desc = arg.description.as_deref().unwrap_or("");
            let required = if arg.required { " (required)" } else { "" };
            output.push_str(&format!("  {:<16}{}{}\n", arg.name, desc, required));

            // Show choices on a second line
            if let Some(choices) = &arg.choices {
                output.push_str(&format!(
                    "                  choices: {}\n",
                    choices.join(", ")
                ));
            }

            // Show default on a second line
            if let Some(default) = &arg.default {
                output.push_str(&format!("                  default: {}\n", default));
            }
        }
        output.push('\n');
    }

    // FLAGS section
    if !cmd.flags.is_empty() {
        output.push_str("FLAGS\n");
        for flag in &cmd.flags {
            let short = if let Some(s) = flag.short {
                format!("-{}, ", s)
            } else {
                "    ".to_string()
            };
            let desc = flag.description.as_deref().unwrap_or("");
            output.push_str(&format!("  {}--{:<12}{}\n", short, flag.name, desc));

            // Show choices for value flags
            if flag.value.is_some() {
                if let Some(choices) = &flag.choices {
                    output.push_str(&format!(
                        "                  choices: {}\n",
                        choices.join(", ")
                    ));
                }

                // Show default for value flags
                if let Some(default) = &flag.default {
                    output.push_str(&format!("                  default: {}\n", default));
                }
            }
        }
        output.push('\n');
    }

    // ARTIFACTS section
    if !cmd.artifacts.is_empty() {
        output.push_str("ARTIFACTS\n");
        for artifact in &cmd.artifacts {
            output.push_str(&format!("  {}\n", artifact));
        }
        output.push('\n');
    }

    // SOURCE section
    output.push_str("SOURCE\n");
    output.push_str(&format!("  {}\n", cmd.script_path.display()));

    output
}

/// Format the available commands as an indented list (used by command-not-found output)
pub fn format_available_commands(tree: &CommandTree) -> String {
    let mut output = String::new();
    if let Some(children) = tree.children()
        && !children.is_empty()
    {
        format_command_list(&mut output, children, 2);
    }
    output
}

/// Print command help
pub fn print_command_help(cmd: &CommandDef, cmd_path: &str, name: &str) {
    print!("{}", format_command_help(cmd, cmd_path, name));
}

/// Format group help output
pub fn format_group_help(
    name: &str,
    group_name: &str,
    description: Option<&str>,
    children: &BTreeMap<String, CommandTree>,
    order: Option<&[String]>,
) -> String {
    let mut output = String::new();

    // Title
    let desc = description.unwrap_or("");
    output.push_str(&format!("{} {} — {}\n\n", name, group_name, desc));

    // COMMANDS section
    if !children.is_empty() {
        output.push_str("COMMANDS\n");

        // Calculate max name width for alignment
        let max_width = children.keys().map(|k| k.len()).max().unwrap_or(16).max(16);

        // Build list respecting order if provided
        let mut ordered_keys = Vec::new();
        let mut remaining_keys: Vec<_> = children.keys().collect();

        if let Some(order_list) = order {
            for ordered_name in order_list {
                if children.contains_key(ordered_name) {
                    ordered_keys.push(ordered_name);
                    remaining_keys.retain(|k| *k != ordered_name);
                }
            }
        }

        // Append remaining keys alphabetically (they're already in BTreeMap order)
        for key in remaining_keys {
            ordered_keys.push(key);
        }

        // Print commands in order
        for child_name in ordered_keys {
            if let Some(child) = children.get(child_name) {
                let desc = match child {
                    CommandTree::Leaf(cmd) => cmd.description.as_deref().unwrap_or(""),
                    CommandTree::Group { description, .. } => description.as_deref().unwrap_or(""),
                };
                output.push_str(&format!(
                    "  {:<width$}{}\n",
                    child_name,
                    desc,
                    width = max_width
                ));
            }
        }
    }

    output
}

/// Print group help
pub fn print_group_help(
    name: &str,
    group_name: &str,
    description: Option<&str>,
    children: &BTreeMap<String, CommandTree>,
    order: Option<&[String]>,
) {
    print!(
        "{}",
        format_group_help(name, group_name, description, children, order)
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{ArgDef, CommandDef, CommandTree, FlagDef};
    use std::path::PathBuf;

    #[test]
    fn test_format_top_level_help_includes_commands() {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "deploy",
            CommandTree::Leaf(CommandDef {
                name: "deploy".into(),
                description: Some("Deploy the app".into()),
                script_path: PathBuf::from(".commands/deploy.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );

        let output = format_top_level_help(&tree, Path::new(".commands"), "las");

        assert!(output.contains("las — project commands"));
        assert!(output.contains("COMMANDS"));
        assert!(output.contains("deploy"));
        assert!(output.contains("Deploy the app"));
        assert!(output.contains("META"));
        assert!(output.contains("--help"));
        assert!(output.contains(".commands"));
    }

    #[test]
    fn test_format_top_level_help_shows_nested_groups() {
        let mut tree = CommandTree::new_group("root");
        let mut db_group = CommandTree::Group {
            name: "db".into(),
            description: Some("Database commands".into()),
            order: None,
            children: BTreeMap::new(),
        };

        if let CommandTree::Group { children, .. } = &mut db_group {
            children.insert(
                "migrate".into(),
                CommandTree::Leaf(CommandDef {
                    name: "migrate".into(),
                    description: Some("Run migrations".into()),
                    script_path: PathBuf::from(".commands/db/migrate.sh"),
                    args: vec![],
                    flags: vec![],
                    ..Default::default()
                }),
            );
        }

        tree.insert("db", db_group);

        let output = format_top_level_help(&tree, Path::new(".commands"), "las");

        assert!(output.contains("db"));
        assert!(output.contains("Database commands"));
        assert!(output.contains("migrate"));
        assert!(output.contains("Run migrations"));
    }

    #[test]
    fn test_format_command_help_with_args_and_flags() {
        let cmd = CommandDef {
            name: "deploy".into(),
            description: Some("Deploy the app".into()),
            script_path: PathBuf::from(".commands/deploy.sh"),
            args: vec![
                ArgDef {
                    name: "env".into(),
                    description: Some("Target environment".into()),
                    required: true,
                    variadic: false,
                    default: None,
                    choices: Some(vec!["staging".into(), "production".into()]),
                },
                ArgDef {
                    name: "version".into(),
                    description: Some("Version to deploy".into()),
                    required: false,
                    variadic: false,
                    default: Some("latest".into()),
                    choices: None,
                },
            ],
            flags: vec![
                FlagDef {
                    name: "dry-run".into(),
                    description: Some("Show what would happen".into()),
                    short: Some('n'),
                    value: None,
                    default: None,
                    choices: None,
                },
                FlagDef {
                    name: "verbose".into(),
                    description: Some("Verbose output".into()),
                    short: Some('v'),
                    value: None,
                    default: None,
                    choices: None,
                },
            ],
            ..Default::default()
        };

        let output = format_command_help(&cmd, "deploy", "las");

        assert!(output.contains("las deploy — Deploy the app"));
        assert!(output.contains("USAGE"));
        assert!(output.contains("las deploy <env> [version] [flags]"));
        assert!(output.contains("ARGUMENTS"));
        assert!(output.contains("env"));
        assert!(output.contains("(required)"));
        assert!(output.contains("choices: staging, production"));
        assert!(output.contains("version"));
        assert!(output.contains("default: latest"));
        assert!(output.contains("FLAGS"));
        assert!(output.contains("-n, --dry-run"));
        assert!(output.contains("-v, --verbose"));
        assert!(output.contains("SOURCE"));
        assert!(output.contains(".commands/deploy.sh"));
    }

    #[test]
    fn test_format_command_help_value_flag_with_choices() {
        let cmd = CommandDef {
            name: "test".into(),
            description: Some("Run tests".into()),
            script_path: PathBuf::from(".commands/test.sh"),
            args: vec![],
            flags: vec![FlagDef {
                name: "format".into(),
                description: Some("Output format".into()),
                short: Some('f'),
                value: Some("format".into()),
                default: Some("json".into()),
                choices: Some(vec!["json".into(), "yaml".into(), "text".into()]),
            }],
            ..Default::default()
        };

        let output = format_command_help(&cmd, "test", "las");

        assert!(output.contains("--format"));
        assert!(output.contains("choices: json, yaml, text"));
        assert!(output.contains("default: json"));
    }

    #[test]
    fn test_format_group_help() {
        let mut children = BTreeMap::new();
        children.insert(
            "migrate".into(),
            CommandTree::Leaf(CommandDef {
                name: "migrate".into(),
                description: Some("Run migrations".into()),
                script_path: PathBuf::from(".commands/db/migrate.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );
        children.insert(
            "seed".into(),
            CommandTree::Leaf(CommandDef {
                name: "seed".into(),
                description: Some("Load seed data".into()),
                script_path: PathBuf::from(".commands/db/seed.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );

        let output = format_group_help("las", "db", Some("Database commands"), &children, None);

        assert!(output.contains("las db — Database commands"));
        assert!(output.contains("COMMANDS"));
        assert!(output.contains("migrate"));
        assert!(output.contains("Run migrations"));
        assert!(output.contains("seed"));
        assert!(output.contains("Load seed data"));
    }

    #[test]
    fn test_format_group_help_respects_order() {
        let mut children = BTreeMap::new();
        children.insert(
            "alpha".into(),
            CommandTree::Leaf(CommandDef {
                name: "alpha".into(),
                description: Some("First alphabetically".into()),
                script_path: PathBuf::from(".commands/alpha.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );
        children.insert(
            "zebra".into(),
            CommandTree::Leaf(CommandDef {
                name: "zebra".into(),
                description: Some("Last alphabetically".into()),
                script_path: PathBuf::from(".commands/zebra.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );

        // Order should put zebra first
        let order = vec!["zebra".to_string()];
        let output = format_group_help("las", "group", Some("Test group"), &children, Some(&order));

        // zebra should appear before alpha in output
        let zebra_pos = output.find("zebra").unwrap();
        let alpha_pos = output.find("alpha").unwrap();
        assert!(zebra_pos < alpha_pos);
    }

    #[test]
    fn test_format_command_help_renders_usage_text() {
        let cmd = CommandDef {
            name: "gconsole".into(),
            description: Some("Run a console command".into()),
            usage: Some(
                "Settle defaults to 2.5s.\n\nlas gconsole switch_biome ice   # then Read the PNG\n"
                    .into(),
            ),
            ..Default::default()
        };

        let output = format_command_help(&cmd, "gconsole", "las");

        assert!(output.contains("USAGE"));
        assert!(output.contains("  Settle defaults to 2.5s."));
        assert!(output.contains("  las gconsole switch_biome ice   # then Read the PNG"));
    }

    #[test]
    fn test_format_command_help_variadic_usage_line() {
        let cmd = CommandDef {
            name: "gconsole".into(),
            args: vec![ArgDef {
                name: "command".into(),
                required: true,
                variadic: true,
                ..Default::default()
            }],
            ..Default::default()
        };

        let output = format_command_help(&cmd, "gconsole", "las");
        assert!(output.contains("las gconsole <command>..."));
    }

    #[test]
    fn test_format_command_help_artifacts_section() {
        let cmd = CommandDef {
            name: "shot".into(),
            artifacts: vec!["/tmp/godot_screenshot.png".into()],
            ..Default::default()
        };

        let output = format_command_help(&cmd, "shot", "las");
        assert!(output.contains("ARTIFACTS"));
        assert!(output.contains("  /tmp/godot_screenshot.png"));
    }

    #[test]
    fn test_format_available_commands() {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "deploy",
            CommandTree::Leaf(CommandDef {
                name: "deploy".into(),
                description: Some("Deploy the app".into()),
                ..Default::default()
            }),
        );

        let output = format_available_commands(&tree);
        assert!(output.contains("deploy"));
        assert!(output.contains("Deploy the app"));
    }

    #[test]
    fn test_format_command_help_no_short_flag() {
        let cmd = CommandDef {
            name: "test".into(),
            description: Some("Test command".into()),
            script_path: PathBuf::from(".commands/test.sh"),
            args: vec![],
            flags: vec![FlagDef {
                name: "verbose".into(),
                description: Some("Verbose output".into()),
                short: None,
                value: None,
                default: None,
                choices: None,
            }],
            ..Default::default()
        };

        let output = format_command_help(&cmd, "test", "las");

        // Should have proper spacing even without short flag
        assert!(output.contains("    --verbose"));
    }
}

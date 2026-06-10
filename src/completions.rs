use crate::command::{CommandDef, CommandTree};
use anyhow::{Result, anyhow};

pub fn generate_completions(shell: &str, tree: &CommandTree, name: &str) -> Result<String> {
    match shell {
        "bash" => Ok(generate_bash(tree, name)),
        "zsh" => Ok(generate_zsh(tree, name)),
        "fish" => Ok(generate_fish(tree, name)),
        _ => Err(anyhow!(
            "unsupported shell: {}. Supported: bash, zsh, fish",
            shell
        )),
    }
}

fn generate_bash(tree: &CommandTree, name: &str) -> String {
    let mut script = String::new();

    script.push_str(&format!("_{}_completions() {{\n", name));
    script.push_str("    local cur prev words cword\n");
    script.push_str("    _init_completion || return\n\n");

    // Collect top-level commands
    let mut commands = Vec::new();
    if let Some(children) = tree.children() {
        for key in children.keys() {
            commands.push(key.clone());
        }
    }

    script.push_str(&format!(
        "    local commands=\"{}\"\n\n",
        commands.join(" ")
    ));

    // Generate case statement for each command
    script.push_str("    case \"${COMP_WORDS[1]}\" in\n");

    if let Some(children) = tree.children() {
        for (cmd_name, child) in children {
            script.push_str(&format!("        {})\n", cmd_name));

            match child {
                CommandTree::Leaf(cmd) => {
                    script.push_str(&generate_bash_command_completion(cmd));
                }
                CommandTree::Group {
                    children: subcommands,
                    ..
                } => {
                    // For groups, complete subcommand names
                    let subcommand_names: Vec<String> = subcommands.keys().cloned().collect();
                    script.push_str(&format!(
                        "            local subcommands=\"{}\"\n",
                        subcommand_names.join(" ")
                    ));
                    script.push_str("            if [[ $cword -eq 2 ]]; then\n");
                    script.push_str(
                        "                COMPREPLY=($(compgen -W \"$subcommands\" -- \"$cur\"))\n",
                    );
                    script.push_str("            fi\n");
                }
            }

            script.push_str("            ;;\n");
        }
    }

    script.push_str("        *)\n");
    script.push_str("            if [[ $cword -eq 1 ]]; then\n");
    script.push_str("                COMPREPLY=($(compgen -W \"$commands\" -- \"$cur\"))\n");
    script.push_str("            fi\n");
    script.push_str("            ;;\n");
    script.push_str("    esac\n");
    script.push_str("}\n");
    script.push_str(&format!("complete -F _{}_completions {}\n", name, name));

    script
}

fn generate_bash_command_completion(cmd: &CommandDef) -> String {
    let mut completion = String::new();

    // Collect flags
    let mut flags = Vec::new();
    for flag in &cmd.flags {
        flags.push(format!("--{}", flag.name));
        if let Some(short) = flag.short {
            flags.push(format!("-{}", short));
        }
    }

    if !flags.is_empty() {
        completion.push_str(&format!(
            "            local flags=\"{}\"\n",
            flags.join(" ")
        ));
    }

    // If first arg has choices, include them
    if let Some(first_arg) = cmd.args.first().and_then(|arg| arg.choices.as_ref()) {
        completion.push_str(&format!(
            "            local arg_choices=\"{}\"\n",
            first_arg.join(" ")
        ));
    }

    // Complete flags or first arg
    completion.push_str("            if [[ \"$cur\" == -* ]]; then\n");
    if !flags.is_empty() {
        completion.push_str("                COMPREPLY=($(compgen -W \"$flags\" -- \"$cur\"))\n");
    }
    completion.push_str("            elif [[ $cword -eq 2 ]]; then\n");
    if cmd.args.first().is_some_and(|arg| arg.choices.is_some()) {
        completion
            .push_str("                COMPREPLY=($(compgen -W \"$arg_choices\" -- \"$cur\"))\n");
    }
    completion.push_str("            fi\n");

    completion
}

fn generate_zsh(tree: &CommandTree, name: &str) -> String {
    let mut script = String::new();

    script.push_str(&format!("#compdef {}\n\n", name));
    script.push_str(&format!("_{name}() {{\n"));
    script.push_str("    local -a commands\n");
    script.push_str("    commands=(\n");

    // Collect top-level commands with descriptions
    if let Some(children) = tree.children() {
        for (cmd_name, child) in children {
            let description = match child {
                CommandTree::Leaf(cmd) => cmd.description.as_deref().unwrap_or(""),
                CommandTree::Group { description, .. } => description.as_deref().unwrap_or(""),
            };
            let escaped_desc = description.replace(':', "\\:");
            script.push_str(&format!("        '{}:{}'\n", cmd_name, escaped_desc));
        }
    }

    script.push_str("    )\n\n");
    script.push_str("    _arguments -C \\\n");
    script.push_str("        '1:command:->cmd' \\\n");
    script.push_str("        '*::arg:->args'\n\n");
    script.push_str("    case \"$state\" in\n");
    script.push_str("        cmd)\n");
    script.push_str("            _describe 'command' commands\n");
    script.push_str("            ;;\n");
    script.push_str("        args)\n");
    script.push_str("            case \"${words[1]}\" in\n");

    // Generate completion for each command
    if let Some(children) = tree.children() {
        for (cmd_name, child) in children {
            script.push_str(&format!("                {})\n", cmd_name));

            match child {
                CommandTree::Leaf(cmd) => {
                    script.push_str(&generate_zsh_command_completion(cmd));
                }
                CommandTree::Group {
                    children: subcommands,
                    ..
                } => {
                    script.push_str("                    local -a subcommands\n");
                    script.push_str("                    subcommands=(\n");
                    for (subcmd_name, subcmd) in subcommands {
                        let description = match subcmd {
                            CommandTree::Leaf(cmd) => cmd.description.as_deref().unwrap_or(""),
                            CommandTree::Group { description, .. } => {
                                description.as_deref().unwrap_or("")
                            }
                        };
                        let escaped_desc = description.replace(':', "\\:");
                        script.push_str(&format!(
                            "                        '{}:{}'\n",
                            subcmd_name, escaped_desc
                        ));
                    }
                    script.push_str("                    )\n");
                    script.push_str("                    _describe 'subcommand' subcommands\n");
                }
            }

            script.push_str("                    ;;\n");
        }
    }

    script.push_str("            esac\n");
    script.push_str("            ;;\n");
    script.push_str("    esac\n");
    script.push_str("}\n\n");
    script.push_str(&format!("_{}\n", name));

    script
}

fn generate_zsh_command_completion(cmd: &CommandDef) -> String {
    let mut completion = String::new();

    completion.push_str("                    _arguments \\\n");

    // Add positional args
    for (idx, arg) in cmd.args.iter().enumerate() {
        let position = idx + 1;
        let arg_name = &arg.name;

        if let Some(choices) = &arg.choices {
            completion.push_str(&format!(
                "                        '{}:{}:({})'",
                position,
                arg_name,
                choices.join(" ")
            ));
        } else {
            completion.push_str(&format!(
                "                        '{}:{}:'",
                position, arg_name
            ));
        }

        if idx < cmd.args.len() - 1 || !cmd.flags.is_empty() {
            completion.push_str(" \\\n");
        } else {
            completion.push('\n');
        }
    }

    // Add flags
    for (idx, flag) in cmd.flags.iter().enumerate() {
        let flag_name = &flag.name;
        let description = flag.description.as_deref().unwrap_or("");
        let escaped_desc = description.replace('[', "\\[").replace(']', "\\]");

        if flag.value.is_some() {
            // Value flag
            if let Some(choices) = &flag.choices {
                completion.push_str(&format!(
                    "                        '--{}[{}]:value:({})'",
                    flag_name,
                    escaped_desc,
                    choices.join(" ")
                ));
            } else {
                completion.push_str(&format!(
                    "                        '--{}[{}]:value:'",
                    flag_name, escaped_desc
                ));
            }
        } else {
            // Boolean flag
            completion.push_str(&format!(
                "                        '--{}[{}]'",
                flag_name, escaped_desc
            ));
        }

        if idx < cmd.flags.len() - 1 {
            completion.push_str(" \\\n");
        } else {
            completion.push('\n');
        }
    }

    completion
}

fn generate_fish(tree: &CommandTree, name: &str) -> String {
    let mut script = String::new();

    // Add top-level commands
    if let Some(children) = tree.children() {
        for (cmd_name, child) in children {
            let description = match child {
                CommandTree::Leaf(cmd) => cmd.description.as_deref().unwrap_or(""),
                CommandTree::Group { description, .. } => description.as_deref().unwrap_or(""),
            };

            script.push_str(&format!(
                "complete -c {} -n '__fish_use_subcommand' -a '{}' -d '{}'\n",
                name,
                cmd_name,
                description.replace('\'', "\\'")
            ));
        }
    }

    script.push('\n');

    // Add subcommands and flags for each command
    if let Some(children) = tree.children() {
        for (cmd_name, child) in children {
            match child {
                CommandTree::Leaf(cmd) => {
                    script.push_str(&generate_fish_command_completion(name, cmd_name, cmd));
                }
                CommandTree::Group {
                    children: subcommands,
                    ..
                } => {
                    // Add subcommands
                    for (subcmd_name, subcmd) in subcommands {
                        let description = match subcmd {
                            CommandTree::Leaf(cmd) => cmd.description.as_deref().unwrap_or(""),
                            CommandTree::Group { description, .. } => {
                                description.as_deref().unwrap_or("")
                            }
                        };

                        script.push_str(&format!(
                            "complete -c {} -n '__fish_seen_subcommand_from {}' -a '{}' -d '{}'\n",
                            name,
                            cmd_name,
                            subcmd_name,
                            description.replace('\'', "\\'")
                        ));
                    }
                    script.push('\n');
                }
            }
        }
    }

    script
}

fn generate_fish_command_completion(name: &str, cmd_name: &str, cmd: &CommandDef) -> String {
    let mut completion = String::new();

    // Add flags
    for flag in &cmd.flags {
        let description = flag.description.as_deref().unwrap_or("");

        completion.push_str(&format!(
            "complete -c {} -n '__fish_seen_subcommand_from {}' -l '{}' -d '{}'\n",
            name,
            cmd_name,
            flag.name,
            description.replace('\'', "\\'")
        ));

        // Add short flag if present
        if let Some(short) = flag.short {
            completion.push_str(&format!(
                "complete -c {} -n '__fish_seen_subcommand_from {}' -s '{}' -d '{}'\n",
                name,
                cmd_name,
                short,
                description.replace('\'', "\\'")
            ));
        }
    }

    if !cmd.flags.is_empty() {
        completion.push('\n');
    }

    completion
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{ArgDef, FlagDef};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn create_test_tree() -> CommandTree {
        let mut root = CommandTree::new_group("root");

        // Add a simple command with flags and choices
        let deploy_cmd = CommandDef {
            name: "deploy".to_string(),
            description: Some("Deploy the app".to_string()),
            script_path: PathBuf::from(".commands/deploy.sh"),
            args: vec![ArgDef {
                name: "env".to_string(),
                description: Some("Environment".to_string()),
                required: true,
                variadic: false,
                default: None,
                choices: Some(vec!["staging".to_string(), "production".to_string()]),
            }],
            flags: vec![FlagDef {
                name: "dry-run".to_string(),
                description: Some("Show what would happen".to_string()),
                short: Some('n'),
                value: None,
                default: None,
                choices: None,
            }],
            ..Default::default()
        };

        root.insert("deploy", CommandTree::Leaf(deploy_cmd));

        // Add a group with subcommands
        let mut db_group = CommandTree::Group {
            name: "db".to_string(),
            description: Some("Database management".to_string()),
            order: None,
            children: BTreeMap::new(),
        };

        let migrate_cmd = CommandDef {
            name: "migrate".to_string(),
            description: Some("Run migrations".to_string()),
            script_path: PathBuf::from(".commands/db/migrate.sh"),
            args: vec![],
            flags: vec![],
            ..Default::default()
        };

        if let CommandTree::Group { children, .. } = &mut db_group {
            children.insert("migrate".to_string(), CommandTree::Leaf(migrate_cmd));
        }

        root.insert("db", db_group);

        root
    }

    #[test]
    fn test_bash_completions_contain_command_names() {
        let tree = create_test_tree();
        let script = generate_bash(&tree, "las");

        assert!(script.contains("deploy"));
        assert!(script.contains("db"));
        assert!(script.contains("local commands="));
    }

    #[test]
    fn test_bash_completions_contain_choices() {
        let tree = create_test_tree();
        let script = generate_bash(&tree, "las");

        assert!(script.contains("staging"));
        assert!(script.contains("production"));
    }

    #[test]
    fn test_zsh_completions_contain_descriptions() {
        let tree = create_test_tree();
        let script = generate_zsh(&tree, "las");

        assert!(script.contains("Deploy the app"));
        assert!(script.contains("Database management"));
        assert!(script.contains("_describe 'command' commands"));
    }

    #[test]
    fn test_fish_completions_contain_flag_names() {
        let tree = create_test_tree();
        let script = generate_fish(&tree, "las");

        // Fish uses -l flag_name (without --)
        assert!(script.contains("dry-run"));
        assert!(script.contains("Show what would happen"));
    }

    #[test]
    fn test_unknown_shell_returns_error() {
        let tree = create_test_tree();
        let result = generate_completions("powershell", &tree, "las");

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unsupported shell")
        );
    }

    #[test]
    fn test_completions_for_all_shells() {
        let tree = create_test_tree();

        // All should succeed
        assert!(generate_completions("bash", &tree, "las").is_ok());
        assert!(generate_completions("zsh", &tree, "las").is_ok());
        assert!(generate_completions("fish", &tree, "las").is_ok());
    }

    #[test]
    fn test_bash_completions_structure() {
        let tree = create_test_tree();
        let script = generate_bash(&tree, "las");

        // Check for key structural elements
        assert!(script.contains("_las_completions()"));
        assert!(script.contains("complete -F _las_completions las"));
        assert!(script.contains("case \"${COMP_WORDS[1]}\" in"));
    }

    #[test]
    fn test_zsh_completions_structure() {
        let tree = create_test_tree();
        let script = generate_zsh(&tree, "las");

        // Check for key structural elements
        assert!(script.contains("#compdef las"));
        assert!(script.contains("_las()"));
        assert!(script.contains("_arguments -C"));
    }

    #[test]
    fn test_fish_completions_structure() {
        let tree = create_test_tree();
        let script = generate_fish(&tree, "las");

        // Check for key structural elements
        assert!(script.contains("complete -c las"));
        assert!(script.contains("__fish_use_subcommand"));
    }
}

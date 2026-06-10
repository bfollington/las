use crate::command::{ArgDef, CommandTree, FlagDef};
use std::path::Path;

/// Generate a markdown skill document for AI agents
pub fn generate_skill(tree: &CommandTree, commands_dir: &Path, name: &str) -> String {
    let mut output = String::new();

    // Header
    output.push_str(&format!("# {} — CLI Skill Document\n\n", name));

    // Overview
    output.push_str("## Overview\n\n");
    output.push_str(&format!(
        "`{}` is a project-specific CLI. Commands are shell scripts in a `.commands/` directory.\n\n",
        name
    ));
    output.push_str(&format!(
        "Commands directory: `{}`\n\n",
        commands_dir.display()
    ));

    // Available Commands
    output.push_str("## Available Commands\n\n");
    format_commands(tree, "", name, &mut output);

    // Creating Commands
    output.push_str("## Creating Commands\n\n");
    output.push_str("Create a new command:\n");
    output.push_str(&format!("```\n{} --new <command-name>\n```\n\n", name));

    // Frontmatter Format
    output.push_str("### Frontmatter Format\n\n");
    output.push_str("Commands can include YAML frontmatter in comments:\n\n");
    output.push_str("```bash\n");
    output.push_str("#!/bin/bash\n");
    output.push_str("#---\n");
    output.push_str("# description: What this command does\n");
    output.push_str("# usage: |\n");
    output
        .push_str("#   las cmd foo --bar    # copy-pasteable examples; shown in --help and here\n");
    output.push_str("# on-failure: Hint printed when the command exits nonzero (how to recover)\n");
    output.push_str("# artifacts:\n");
    output.push_str(
        "#   - /tmp/output.png    # files the command produces; pointers printed on success\n",
    );
    output.push_str("# args:\n");
    output.push_str("#   arg_name:\n");
    output.push_str("#     description: What this argument is\n");
    output.push_str("#     required: true\n");
    output.push_str("#     variadic: true       # last arg only: collects all remaining words\n");
    output.push_str("#     default: value\n");
    output.push_str("#     choices: [a, b, c]\n");
    output.push_str("# flags:\n");
    output.push_str("#   flag-name:\n");
    output.push_str("#     description: What this flag does\n");
    output.push_str("#     short: f\n");
    output.push_str("#     value: description   # if flag takes a value\n");
    output.push_str("#     default: value\n");
    output.push_str("#     choices: [x, y, z]\n");
    output.push_str("#---\n");
    output.push_str("```\n\n");

    // How Arguments Are Passed
    output.push_str("### How Arguments Are Passed\n\n");
    output.push_str("Arguments and flags are passed as environment variables:\n");
    output.push_str("- Positional args: `ARG_<NAME>` (UPPER_SNAKE_CASE)\n");
    output.push_str("- Flags: `FLAG_<NAME>` (UPPER_SNAKE_CASE, dashes become underscores)\n");
    output.push_str("- Boolean flags: `\"true\"` or `\"false\"`\n");
    output.push_str("- Positional args also available as `$1`, `$2`, etc.\n\n");

    // Editing Commands
    output.push_str("### Editing Commands\n\n");
    output.push_str(&format!("- `{} --edit <cmd>` — Open in $EDITOR\n", name));
    output.push_str(&format!("- `{} --which <cmd>` — Print file path\n\n", name));

    // Best Practices
    output.push_str("## Best Practices\n\n");
    output.push_str("- Prefer small, composable commands\n");
    output.push_str("- Use descriptive names that form a natural vocabulary\n");
    output.push_str("- Group related commands in subdirectories\n");
    output.push_str(
        "- Always include a description and a copy-pasteable `usage:` example in frontmatter\n",
    );
    output.push_str("- Add `on-failure:` hints so failures explain how to recover\n");
    output.push_str(
        "- Declare `artifacts:` for files a command produces, so callers know what to read next\n",
    );
    output.push_str("- Use `set -euo pipefail` for robust scripts\n");

    output
}

/// Recursively format commands in the tree
fn format_commands(tree: &CommandTree, path: &str, name: &str, output: &mut String) {
    match tree {
        CommandTree::Leaf(cmd) => {
            // Format this command's documentation
            // Note: path already contains the full command path including the command name
            output.push_str(&format!("### {} {}\n\n", name, path));

            // Description
            if let Some(desc) = &cmd.description {
                output.push_str(&format!("{}\n\n", desc));
            } else {
                output.push_str("No description\n\n");
            }

            // Usage examples / caveats
            if let Some(usage) = &cmd.usage {
                output.push_str("**Usage:**\n```\n");
                output.push_str(usage.trim_end());
                output.push_str("\n```\n\n");
            }

            // Arguments
            if !cmd.args.is_empty() {
                output.push_str("**Arguments:**\n");
                for arg in &cmd.args {
                    format_arg(arg, output);
                }
                output.push('\n');
            }

            // Flags
            if !cmd.flags.is_empty() {
                output.push_str("**Flags:**\n");
                for flag in &cmd.flags {
                    format_flag(flag, output);
                }
                output.push('\n');
            }

            // Artifacts
            if !cmd.artifacts.is_empty() {
                output.push_str("**Artifacts** (read these after a successful run):\n");
                for artifact in &cmd.artifacts {
                    output.push_str(&format!("- `{}`\n", artifact));
                }
                output.push('\n');
            }

            output.push_str("---\n\n");
        }
        CommandTree::Group { children, .. } => {
            for (child_name, child) in children {
                let child_path = if path.is_empty() {
                    child_name.clone()
                } else {
                    format!("{} {}", path, child_name)
                };
                format_commands(child, &child_path, name, output);
            }
        }
    }
}

/// Format an argument definition
fn format_arg(arg: &ArgDef, output: &mut String) {
    output.push_str(&format!("- `{}` — ", arg.name));

    if let Some(desc) = &arg.description {
        output.push_str(desc);
    } else {
        output.push_str("No description");
    }

    // Required/optional
    if arg.required {
        output.push_str(" (required");
    } else {
        output.push_str(" (optional");
    }

    // Default
    if let Some(default) = &arg.default {
        output.push_str(&format!(", default: {}", default));
    }

    // Choices
    if let Some(choices) = &arg.choices {
        output.push_str(&format!(", choices: [{}]", choices.join(", ")));
    }

    output.push_str(")\n");
}

/// Format a flag definition
fn format_flag(flag: &FlagDef, output: &mut String) {
    output.push_str("- `--");
    output.push_str(&flag.name);
    output.push('`');

    if let Some(short) = flag.short {
        output.push_str(&format!(" (`-{}`)", short));
    }

    output.push_str(" — ");

    if let Some(desc) = &flag.description {
        output.push_str(desc);
    } else {
        output.push_str("No description");
    }

    // Value flags
    if let Some(value) = &flag.value {
        output.push_str(&format!(" (takes value: {})", value));

        // Default for value flags
        if let Some(default) = &flag.default {
            output.push_str(&format!(", default: {}", default));
        }

        // Choices for value flags
        if let Some(choices) = &flag.choices {
            output.push_str(&format!(", choices: [{}]", choices.join(", ")));
        }
    }

    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{ArgDef, CommandDef, FlagDef};
    use std::path::PathBuf;

    fn make_simple_tree() -> CommandTree {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "deploy",
            CommandTree::Leaf(CommandDef {
                name: "deploy".into(),
                description: Some("Deploy the app".into()),
                script_path: PathBuf::from(".commands/deploy.sh"),
                args: vec![ArgDef {
                    name: "env".into(),
                    description: Some("Target environment".into()),
                    required: true,
                    variadic: false,
                    default: None,
                    choices: Some(vec!["staging".into(), "production".into()]),
                }],
                flags: vec![FlagDef {
                    name: "dry-run".into(),
                    description: Some("Show what would happen".into()),
                    short: Some('n'),
                    value: None,
                    default: None,
                    choices: None,
                }],
                ..Default::default()
            }),
        );
        tree
    }

    #[test]
    fn skill_doc_contains_command_names() {
        let tree = make_simple_tree();
        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("deploy"));
    }

    #[test]
    fn skill_doc_contains_arg_descriptions() {
        let tree = make_simple_tree();
        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("env"));
        assert!(skill.contains("Target environment"));
        assert!(skill.contains("required"));
        assert!(skill.contains("staging, production"));
    }

    #[test]
    fn skill_doc_contains_flag_descriptions() {
        let tree = make_simple_tree();
        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("dry-run"));
        assert!(skill.contains("Show what would happen"));
        assert!(skill.contains("-n"));
    }

    #[test]
    fn skill_doc_contains_frontmatter_format() {
        let tree = make_simple_tree();
        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("Frontmatter Format"));
        assert!(skill.contains("#---"));
        assert!(skill.contains("description:"));
        assert!(skill.contains("args:"));
        assert!(skill.contains("flags:"));
    }

    #[test]
    fn skill_doc_contains_commands_dir() {
        let tree = make_simple_tree();
        let commands_dir = Path::new("/test/.commands");
        let skill = generate_skill(&tree, commands_dir, "las");

        assert!(skill.contains("/test/.commands"));
    }

    #[test]
    fn skill_doc_contains_best_practices() {
        let tree = make_simple_tree();
        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("Best Practices"));
        assert!(skill.contains("Prefer small, composable commands"));
        assert!(skill.contains("Use descriptive names"));
    }

    #[test]
    fn skill_doc_with_nested_commands() {
        let mut tree = CommandTree::new_group("root");

        // Create db group with migrate command
        let mut db_group = CommandTree::new_group("db");
        db_group.insert(
            "migrate",
            CommandTree::Leaf(CommandDef {
                name: "migrate".into(),
                description: Some("Run database migrations".into()),
                script_path: PathBuf::from(".commands/db/migrate.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );
        tree.insert("db", db_group);

        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("las db migrate"));
        assert!(skill.contains("Run database migrations"));
    }

    #[test]
    fn skill_doc_with_no_description() {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "test",
            CommandTree::Leaf(CommandDef {
                name: "test".into(),
                description: None,
                script_path: PathBuf::from(".commands/test.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );

        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("No description"));
    }

    #[test]
    fn skill_doc_with_value_flag() {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "build",
            CommandTree::Leaf(CommandDef {
                name: "build".into(),
                description: Some("Build the project".into()),
                script_path: PathBuf::from(".commands/build.sh"),
                args: vec![],
                flags: vec![FlagDef {
                    name: "output".into(),
                    description: Some("Output file".into()),
                    short: Some('o'),
                    value: Some("file".into()),
                    default: Some("dist/output".into()),
                    choices: None,
                }],
                ..Default::default()
            }),
        );

        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("output"));
        assert!(skill.contains("Output file"));
        assert!(skill.contains("takes value: file"));
        assert!(skill.contains("default: dist/output"));
    }

    #[test]
    fn skill_doc_contains_usage_and_artifacts() {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "shot",
            CommandTree::Leaf(CommandDef {
                name: "shot".into(),
                description: Some("Capture a screenshot".into()),
                usage: Some("las shot   # then Read the PNG\n".into()),
                artifacts: vec!["/tmp/godot_screenshot.png".into()],
                ..Default::default()
            }),
        );

        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("**Usage:**"));
        assert!(skill.contains("las shot   # then Read the PNG"));
        assert!(skill.contains("**Artifacts**"));
        assert!(skill.contains("/tmp/godot_screenshot.png"));
    }

    #[test]
    fn skill_doc_documents_new_frontmatter_fields() {
        let tree = make_simple_tree();
        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("usage: |"));
        assert!(skill.contains("on-failure:"));
        assert!(skill.contains("artifacts:"));
        assert!(skill.contains("variadic: true"));
    }

    #[test]
    fn skill_doc_environment_variables() {
        let tree = make_simple_tree();
        let skill = generate_skill(&tree, Path::new("/test/.commands"), "las");

        assert!(skill.contains("ARG_<NAME>"));
        assert!(skill.contains("FLAG_<NAME>"));
        assert!(skill.contains("UPPER_SNAKE_CASE"));
    }
}

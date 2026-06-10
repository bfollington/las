use crate::command::{ArgDef, CommandTree, FlagDef};
use serde::Serialize;
use std::path::Path;

/// One command in the machine-readable dump
#[derive(Serialize)]
struct JsonCommand<'a> {
    /// Command path segments, e.g. ["db", "migrate"]
    path: Vec<String>,
    /// Full invocation, e.g. "las db migrate"
    invocation: String,
    description: Option<&'a str>,
    usage: Option<&'a str>,
    on_failure: Option<&'a str>,
    artifacts: &'a [String],
    args: &'a [ArgDef],
    flags: &'a [FlagDef],
    script: String,
}

#[derive(Serialize)]
struct JsonDump<'a> {
    name: &'a str,
    commands_dir: String,
    commands: Vec<JsonCommand<'a>>,
}

/// Generate a machine-readable JSON dump of all command metadata
pub fn generate_json(tree: &CommandTree, commands_dir: &Path, name: &str) -> String {
    let mut commands = Vec::new();
    collect(tree, &mut Vec::new(), name, &mut commands);

    let dump = JsonDump {
        name,
        commands_dir: commands_dir.display().to_string(),
        commands,
    };

    // Serialization of plain strings/lists cannot fail
    serde_json::to_string_pretty(&dump).expect("JSON serialization failed")
}

fn collect<'a>(
    tree: &'a CommandTree,
    path: &mut Vec<String>,
    name: &str,
    out: &mut Vec<JsonCommand<'a>>,
) {
    match tree {
        CommandTree::Leaf(cmd) => {
            out.push(JsonCommand {
                path: path.clone(),
                invocation: format!("{} {}", name, path.join(" ")),
                description: cmd.description.as_deref(),
                usage: cmd.usage.as_deref(),
                on_failure: cmd.on_failure.as_deref(),
                artifacts: &cmd.artifacts,
                args: &cmd.args,
                flags: &cmd.flags,
                script: cmd.script_path.display().to_string(),
            });
        }
        CommandTree::Group { children, .. } => {
            for (child_name, child) in children {
                path.push(child_name.clone());
                collect(child, path, name, out);
                path.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandDef;
    use std::path::PathBuf;

    fn make_tree() -> CommandTree {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "shot",
            CommandTree::Leaf(CommandDef {
                name: "shot".into(),
                description: Some("Capture a screenshot".into()),
                usage: Some("las shot\n".into()),
                on_failure: Some("Is the game running?".into()),
                artifacts: vec!["/tmp/shot.png".into()],
                script_path: PathBuf::from(".commands/shot.sh"),
                ..Default::default()
            }),
        );
        let mut db = CommandTree::new_group("db");
        db.insert(
            "migrate",
            CommandTree::Leaf(CommandDef {
                name: "migrate".into(),
                description: Some("Run migrations".into()),
                script_path: PathBuf::from(".commands/db/migrate.sh"),
                ..Default::default()
            }),
        );
        tree.insert("db", db);
        tree
    }

    #[test]
    fn json_is_valid_and_complete() {
        let tree = make_tree();
        let json = generate_json(&tree, Path::new("/test/.commands"), "las");

        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["name"], "las");
        assert_eq!(parsed["commands_dir"], "/test/.commands");

        let commands = parsed["commands"].as_array().unwrap();
        assert_eq!(commands.len(), 2);

        // BTreeMap ordering: "db" before "shot"
        assert_eq!(commands[0]["invocation"], "las db migrate");
        assert_eq!(commands[0]["path"], serde_json::json!(["db", "migrate"]));

        assert_eq!(commands[1]["invocation"], "las shot");
        assert_eq!(commands[1]["description"], "Capture a screenshot");
        assert_eq!(commands[1]["on_failure"], "Is the game running?");
        assert_eq!(
            commands[1]["artifacts"],
            serde_json::json!(["/tmp/shot.png"])
        );
        assert_eq!(commands[1]["script"], ".commands/shot.sh");
    }

    #[test]
    fn json_includes_arg_and_flag_metadata() {
        let mut tree = CommandTree::new_group("root");
        tree.insert(
            "deploy",
            CommandTree::Leaf(CommandDef {
                name: "deploy".into(),
                args: vec![ArgDef {
                    name: "env".into(),
                    required: true,
                    choices: Some(vec!["staging".into(), "production".into()]),
                    ..Default::default()
                }],
                flags: vec![FlagDef {
                    name: "dry-run".into(),
                    short: Some('n'),
                    ..Default::default()
                }],
                ..Default::default()
            }),
        );

        let json = generate_json(&tree, Path::new("/test/.commands"), "las");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let cmd = &parsed["commands"][0];

        assert_eq!(cmd["args"][0]["name"], "env");
        assert_eq!(cmd["args"][0]["required"], true);
        assert_eq!(cmd["args"][0]["variadic"], false);
        assert_eq!(
            cmd["args"][0]["choices"],
            serde_json::json!(["staging", "production"])
        );
        assert_eq!(cmd["flags"][0]["name"], "dry-run");
        assert_eq!(cmd["flags"][0]["short"], "n");
    }
}

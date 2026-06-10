use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ArgDef {
    pub name: String,
    pub description: Option<String>,
    pub required: bool,
    /// If true, this (last) argument collects all remaining positional words.
    pub variadic: bool,
    pub default: Option<String>,
    pub choices: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FlagDef {
    pub name: String,
    pub description: Option<String>,
    pub short: Option<char>,
    /// If Some, the flag takes a value (the string describes the value).
    /// If None, the flag is boolean.
    pub value: Option<String>,
    pub default: Option<String>,
    pub choices: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct CommandDef {
    pub name: String,
    pub description: Option<String>,
    /// Free-form usage text (examples, caveats) shown in help and the skill doc.
    pub usage: Option<String>,
    /// Remediation hint printed when the command exits nonzero.
    pub on_failure: Option<String>,
    /// Files the command produces; pointers are printed after a successful run.
    pub artifacts: Vec<String>,
    pub script_path: PathBuf,
    pub args: Vec<ArgDef>,
    pub flags: Vec<FlagDef>,
    pub stdin: Option<String>,
}

#[derive(Debug, Clone)]
pub enum CommandTree {
    Leaf(CommandDef),
    Group {
        name: String,
        description: Option<String>,
        order: Option<Vec<String>>,
        children: BTreeMap<String, CommandTree>,
    },
}

impl CommandTree {
    pub fn new_group(name: impl Into<String>) -> Self {
        CommandTree::Group {
            name: name.into(),
            description: None,
            order: None,
            children: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, child: CommandTree) {
        if let CommandTree::Group { children, .. } = self {
            children.insert(key.into(), child);
        }
    }

    pub fn get(&self, key: &str) -> Option<&CommandTree> {
        if let CommandTree::Group { children, .. } = self {
            children.get(key)
        } else {
            None
        }
    }

    pub fn children(&self) -> Option<&BTreeMap<String, CommandTree>> {
        if let CommandTree::Group { children, .. } = self {
            Some(children)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_def_basic() {
        let cmd = CommandDef {
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
        };
        assert_eq!(cmd.name, "deploy");
        assert_eq!(cmd.args.len(), 1);
        assert!(cmd.args[0].required);
        assert_eq!(cmd.flags[0].short, Some('n'));
    }

    #[test]
    fn tree_alphabetical_ordering() {
        let mut root = CommandTree::new_group("root");
        root.insert(
            "zebra",
            CommandTree::Leaf(CommandDef {
                name: "zebra".into(),
                description: None,
                script_path: PathBuf::from("zebra.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );
        root.insert(
            "alpha",
            CommandTree::Leaf(CommandDef {
                name: "alpha".into(),
                description: None,
                script_path: PathBuf::from("alpha.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );

        let keys: Vec<&String> = root.children().unwrap().keys().collect();
        assert_eq!(keys, vec!["alpha", "zebra"]);
    }

    #[test]
    fn tree_nesting() {
        let mut root = CommandTree::new_group("root");
        let mut db = CommandTree::new_group("db");
        db.insert(
            "migrate",
            CommandTree::Leaf(CommandDef {
                name: "migrate".into(),
                description: Some("Run migrations".into()),
                script_path: PathBuf::from(".commands/db/migrate.sh"),
                args: vec![],
                flags: vec![],
                ..Default::default()
            }),
        );
        root.insert("db", db);

        let db_node = root.get("db").unwrap();
        let migrate = db_node.get("migrate").unwrap();
        match migrate {
            CommandTree::Leaf(cmd) => assert_eq!(cmd.name, "migrate"),
            _ => panic!("expected leaf"),
        }
    }
}

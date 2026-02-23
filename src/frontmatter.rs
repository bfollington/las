use std::collections::BTreeMap;

use serde::Deserialize;

use crate::command::{ArgDef, CommandDef, FlagDef};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default)]
pub struct RawFrontmatter {
    pub description: Option<String>,
    pub args: Option<BTreeMap<String, RawArg>>,
    pub flags: Option<BTreeMap<String, RawFlag>>,
    pub stdin: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RawArg {
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    pub default: Option<String>,
    pub choices: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct RawFlag {
    pub description: Option<String>,
    pub short: Option<char>,
    pub value: Option<String>,
    pub default: Option<String>,
    pub choices: Option<Vec<String>>,
}

/// Extract frontmatter YAML from a script's content.
/// Returns None if no `#---` delimiters are found.
pub fn parse_frontmatter(content: &str) -> Result<Option<RawFrontmatter>, String> {
    let lines: Vec<&str> = content.lines().collect();

    let mut delimiters = vec![];
    for (i, line) in lines.iter().enumerate() {
        if line.trim() == "#---" {
            delimiters.push(i);
            if delimiters.len() == 2 {
                break;
            }
        }
    }

    if delimiters.len() < 2 {
        return Ok(None);
    }

    let yaml_lines: Vec<&str> = lines[delimiters[0] + 1..delimiters[1]]
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(stripped) = trimmed.strip_prefix("# ") {
                stripped
            } else if trimmed == "#" {
                ""
            } else {
                trimmed
            }
        })
        .collect();

    let yaml_str = yaml_lines.join("\n");
    let raw: RawFrontmatter =
        serde_yaml::from_str(&yaml_str).map_err(|e| format!("frontmatter YAML error: {e}"))?;
    Ok(Some(raw))
}

/// Build a CommandDef from a script file's content, name, and path.
pub fn command_from_script(
    name: &str,
    script_path: PathBuf,
    content: &str,
) -> Result<CommandDef, String> {
    let frontmatter = parse_frontmatter(content)?;

    let (description, args, flags, stdin) = match frontmatter {
        Some(fm) => {
            let args: Vec<ArgDef> = fm
                .args
                .unwrap_or_default()
                .into_iter()
                .map(|(name, raw)| ArgDef {
                    name,
                    description: raw.description,
                    required: raw.required,
                    default: raw.default,
                    choices: raw.choices,
                })
                .collect();

            let flags: Vec<FlagDef> = fm
                .flags
                .unwrap_or_default()
                .into_iter()
                .map(|(name, raw)| FlagDef {
                    name,
                    description: raw.description,
                    short: raw.short,
                    value: raw.value,
                    default: raw.default,
                    choices: raw.choices,
                })
                .collect();

            (fm.description, args, flags, fm.stdin)
        }
        None => (None, vec![], vec![], None),
    };

    Ok(CommandDef {
        name: name.to_string(),
        description,
        script_path,
        args,
        flags,
        stdin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_frontmatter() {
        let content = r#"#!/bin/bash
#---
# description: Deploy the app
# args:
#   env:
#     description: Target environment
#     required: true
#     choices: [staging, production]
#   version:
#     description: Version tag
#     default: latest
# flags:
#   dry-run:
#     description: Show what would happen
#     short: n
#   verbose:
#     description: Show detailed output
#     short: v
#---

echo "deploying"
"#;
        let fm = parse_frontmatter(content).unwrap().unwrap();
        assert_eq!(fm.description.as_deref(), Some("Deploy the app"));

        let args = fm.args.unwrap();
        assert_eq!(args.len(), 2);
        assert!(args["env"].required);
        assert_eq!(
            args["env"].choices.as_ref().unwrap(),
            &vec!["staging".to_string(), "production".to_string()]
        );
        assert_eq!(args["version"].default.as_deref(), Some("latest"));

        let flags = fm.flags.unwrap();
        assert_eq!(flags.len(), 2);
        assert_eq!(flags["dry-run"].short, Some('n'));
    }

    #[test]
    fn parse_bare_script() {
        let content = "#!/bin/bash\necho hello\n";
        let fm = parse_frontmatter(content).unwrap();
        assert!(fm.is_none());
    }

    #[test]
    fn parse_description_only() {
        let content = "#!/bin/bash\n#---\n# description: Just a description\n#---\necho hi\n";
        let fm = parse_frontmatter(content).unwrap().unwrap();
        assert_eq!(fm.description.as_deref(), Some("Just a description"));
        assert!(fm.args.is_none());
        assert!(fm.flags.is_none());
    }

    #[test]
    fn malformed_yaml_returns_error() {
        let content = "#!/bin/bash\n#---\n# [invalid yaml\n#---\necho hi\n";
        let result = parse_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn command_from_bare_script() {
        let cmd = command_from_script("hello", PathBuf::from("hello.sh"), "#!/bin/bash\necho hi\n")
            .unwrap();
        assert_eq!(cmd.name, "hello");
        assert!(cmd.description.is_none());
        assert!(cmd.args.is_empty());
        assert!(cmd.flags.is_empty());
    }

    #[test]
    fn command_from_full_script() {
        let content = r#"#!/bin/bash
#---
# description: Greet someone
# args:
#   name:
#     description: Who to greet
#     required: true
# flags:
#   loud:
#     description: Shout it
#     short: l
#---
echo "hello $ARG_NAME"
"#;
        let cmd = command_from_script("greet", PathBuf::from("greet.sh"), content).unwrap();
        assert_eq!(cmd.description.as_deref(), Some("Greet someone"));
        assert_eq!(cmd.args.len(), 1);
        assert_eq!(cmd.args[0].name, "name");
        assert!(cmd.args[0].required);
        assert_eq!(cmd.flags.len(), 1);
        assert_eq!(cmd.flags[0].short, Some('l'));
    }

    #[test]
    fn hash_only_line_treated_as_blank() {
        let content = "#!/bin/bash\n#---\n# description: Test\n#\n# args:\n#   foo:\n#     description: A foo\n#---\necho\n";
        let fm = parse_frontmatter(content).unwrap().unwrap();
        assert_eq!(fm.description.as_deref(), Some("Test"));
        assert!(fm.args.is_some());
    }

    #[test]
    fn value_flag() {
        let content = r#"#!/bin/bash
#---
# flags:
#   output:
#     description: Output file
#     value: path
#     default: out.txt
#---
echo hi
"#;
        let fm = parse_frontmatter(content).unwrap().unwrap();
        let flags = fm.flags.unwrap();
        assert_eq!(flags["output"].value.as_deref(), Some("path"));
        assert_eq!(flags["output"].default.as_deref(), Some("out.txt"));
    }
}

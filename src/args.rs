use crate::command::{CommandDef, FlagDef};
use std::collections::HashMap;
use std::fmt;

/// Parsed arguments ready to be passed to a script
#[derive(Debug, Clone)]
pub struct ParsedArgs {
    /// Environment variables for arguments (ARG_FOO=bar)
    pub env_args: HashMap<String, String>,
    /// Environment variables for flags (FLAG_DRY_RUN=true)
    pub env_flags: HashMap<String, String>,
    /// Positional arguments for $1, $2, etc.
    pub positional: Vec<String>,
}

/// Errors that can occur during argument parsing
#[derive(Debug)]
pub enum ArgsError {
    MissingRequired(String),
    InvalidChoice {
        name: String,
        value: String,
        choices: Vec<String>,
    },
    UnknownFlag(String),
    MissingFlagValue(String),
}

impl fmt::Display for ArgsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgsError::MissingRequired(name) => {
                write!(f, "Missing required argument: {}", name)
            }
            ArgsError::InvalidChoice {
                name,
                value,
                choices,
            } => {
                write!(
                    f,
                    "Invalid value '{}' for {}. Must be one of: {}",
                    value,
                    name,
                    choices.join(", ")
                )
            }
            ArgsError::UnknownFlag(flag) => {
                write!(f, "Unknown flag: {}", flag)
            }
            ArgsError::MissingFlagValue(flag) => {
                write!(f, "Flag {} requires a value", flag)
            }
        }
    }
}

impl std::error::Error for ArgsError {}

/// Convert a name to an environment variable name (UPPER_SNAKE_CASE, dashes to underscores)
fn to_env_name(prefix: &str, name: &str) -> String {
    format!(
        "{}_{}",
        prefix,
        name.to_uppercase().replace(['-', ' '], "_")
    )
}

/// Parse command-line arguments according to the command definition
pub fn parse_args(cmd: &CommandDef, raw: &[String]) -> Result<ParsedArgs, ArgsError> {
    let mut env_args = HashMap::new();
    let mut env_flags = HashMap::new();
    let mut positional = Vec::new();

    // Build flag lookup tables
    let mut flag_by_long: HashMap<&str, &FlagDef> = HashMap::new();
    let mut flag_by_short: HashMap<char, &FlagDef> = HashMap::new();

    for flag in &cmd.flags {
        flag_by_long.insert(&flag.name, flag);
        if let Some(short) = flag.short {
            flag_by_short.insert(short, flag);
        }
    }

    // Initialize all boolean flags to "false" by default
    for flag in &cmd.flags {
        if flag.value.is_none() {
            let env_name = to_env_name("FLAG", &flag.name);
            env_flags.insert(env_name, "false".to_string());
        }
    }

    let mut i = 0;
    let mut parsing_flags = true;

    while i < raw.len() {
        let token = &raw[i];

        if parsing_flags && token == "--" {
            // Stop flag parsing
            parsing_flags = false;
            i += 1;
            continue;
        }

        if parsing_flags && token.starts_with("--") {
            // Long flag
            let flag_str = &token[2..];

            if let Some(eq_pos) = flag_str.find('=') {
                // --flag=value format
                let flag_name = &flag_str[..eq_pos];
                let flag_value = &flag_str[eq_pos + 1..];

                let flag_def = flag_by_long
                    .get(flag_name)
                    .ok_or_else(|| ArgsError::UnknownFlag(format!("--{}", flag_name)))?;

                let env_name = to_env_name("FLAG", &flag_def.name);

                if flag_def.value.is_some() {
                    // Value flag
                    validate_choice(&flag_def.name, flag_value, &flag_def.choices)?;
                    env_flags.insert(env_name, flag_value.to_string());
                } else {
                    // Boolean flag with =value is treated as boolean, value ignored
                    env_flags.insert(env_name, "true".to_string());
                }
            } else {
                // --flag or --flag value format
                let flag_def = flag_by_long
                    .get(flag_str)
                    .ok_or_else(|| ArgsError::UnknownFlag(format!("--{}", flag_str)))?;

                let env_name = to_env_name("FLAG", &flag_def.name);

                if flag_def.value.is_some() {
                    // Value flag, next token is the value
                    i += 1;
                    if i >= raw.len() {
                        return Err(ArgsError::MissingFlagValue(format!("--{}", flag_str)));
                    }
                    let flag_value = &raw[i];
                    validate_choice(&flag_def.name, flag_value, &flag_def.choices)?;
                    env_flags.insert(env_name, flag_value.to_string());
                } else {
                    // Boolean flag
                    env_flags.insert(env_name, "true".to_string());
                }
            }
            i += 1;
        } else if parsing_flags && token.starts_with('-') && token.len() > 1 && token != "-" {
            // Short flag
            let short_char = token.chars().nth(1).unwrap();

            let flag_def = flag_by_short
                .get(&short_char)
                .ok_or_else(|| ArgsError::UnknownFlag(format!("-{}", short_char)))?;

            let env_name = to_env_name("FLAG", &flag_def.name);

            if flag_def.value.is_some() {
                // Value flag, next token is the value
                i += 1;
                if i >= raw.len() {
                    return Err(ArgsError::MissingFlagValue(format!("-{}", short_char)));
                }
                let flag_value = &raw[i];
                validate_choice(&flag_def.name, flag_value, &flag_def.choices)?;
                env_flags.insert(env_name, flag_value.to_string());
            } else {
                // Boolean flag
                env_flags.insert(env_name, "true".to_string());
            }
            i += 1;
        } else {
            // Positional argument
            positional.push(token.clone());
            i += 1;
        }
    }

    // Map positional arguments to declared arg names
    for (idx, arg_def) in cmd.args.iter().enumerate() {
        let env_name = to_env_name("ARG", &arg_def.name);

        if idx < positional.len() {
            // Use provided positional value
            let value = &positional[idx];
            validate_choice(&arg_def.name, value, &arg_def.choices)?;
            env_args.insert(env_name, value.clone());
        } else if let Some(default) = &arg_def.default {
            // Use default value
            env_args.insert(env_name, default.clone());
        } else if arg_def.required {
            // Missing required argument
            return Err(ArgsError::MissingRequired(arg_def.name.clone()));
        }
    }

    // Apply defaults for flags that weren't provided
    for flag in &cmd.flags {
        if flag.value.is_some() {
            let env_name = to_env_name("FLAG", &flag.name);
            if !env_flags.contains_key(&env_name)
                && let Some(default) = &flag.default {
                    env_flags.insert(env_name, default.clone());
                }
        }
    }

    Ok(ParsedArgs {
        env_args,
        env_flags,
        positional,
    })
}

/// Validate that a value is in the allowed choices (if choices are defined)
fn validate_choice(name: &str, value: &str, choices: &Option<Vec<String>>) -> Result<(), ArgsError> {
    if let Some(valid_choices) = choices
        && !valid_choices.contains(&value.to_string()) {
            return Err(ArgsError::InvalidChoice {
                name: name.to_string(),
                value: value.to_string(),
                choices: valid_choices.clone(),
            });
        }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::ArgDef;
    use std::path::PathBuf;

    fn make_test_command() -> CommandDef {
        CommandDef {
            name: "test".into(),
            description: None,
            script_path: PathBuf::from("test.sh"),
            args: vec![
                ArgDef {
                    name: "env".into(),
                    description: None,
                    required: true,
                    default: None,
                    choices: Some(vec!["dev".into(), "prod".into()]),
                },
                ArgDef {
                    name: "region".into(),
                    description: None,
                    required: false,
                    default: Some("us-west".into()),
                    choices: None,
                },
            ],
            flags: vec![
                FlagDef {
                    name: "dry-run".into(),
                    description: None,
                    short: Some('n'),
                    value: None,
                    default: None,
                    choices: None,
                },
                FlagDef {
                    name: "verbose".into(),
                    description: None,
                    short: Some('v'),
                    value: None,
                    default: None,
                    choices: None,
                },
                FlagDef {
                    name: "output".into(),
                    description: None,
                    short: Some('o'),
                    value: Some("file".into()),
                    default: Some("stdout".into()),
                    choices: None,
                },
                FlagDef {
                    name: "format".into(),
                    description: None,
                    short: None,
                    value: Some("type".into()),
                    default: None,
                    choices: Some(vec!["json".into(), "yaml".into()]),
                },
            ],
            stdin: None,
        }
    }

    #[test]
    fn test_basic_positional_args() {
        let cmd = make_test_command();
        let raw = vec!["prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        assert_eq!(parsed.env_args.get("ARG_ENV"), Some(&"prod".to_string()));
        assert_eq!(
            parsed.env_args.get("ARG_REGION"),
            Some(&"us-west".to_string())
        ); // default
        assert_eq!(parsed.positional, vec!["prod"]);
    }

    #[test]
    fn test_boolean_flag_long() {
        let cmd = make_test_command();
        let raw = vec!["--dry-run".to_string(), "prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        assert_eq!(
            parsed.env_flags.get("FLAG_DRY_RUN"),
            Some(&"true".to_string())
        );
        assert_eq!(
            parsed.env_flags.get("FLAG_VERBOSE"),
            Some(&"false".to_string())
        ); // not provided
    }

    #[test]
    fn test_boolean_flag_short() {
        let cmd = make_test_command();
        let raw = vec!["-v".to_string(), "prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        assert_eq!(
            parsed.env_flags.get("FLAG_VERBOSE"),
            Some(&"true".to_string())
        );
        assert_eq!(
            parsed.env_flags.get("FLAG_DRY_RUN"),
            Some(&"false".to_string())
        );
    }

    #[test]
    fn test_value_flag_long_space() {
        let cmd = make_test_command();
        let raw = vec!["--output".to_string(), "file.txt".to_string(), "prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        assert_eq!(
            parsed.env_flags.get("FLAG_OUTPUT"),
            Some(&"file.txt".to_string())
        );
    }

    #[test]
    fn test_value_flag_long_equals() {
        let cmd = make_test_command();
        let raw = vec!["--output=file.txt".to_string(), "prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        assert_eq!(
            parsed.env_flags.get("FLAG_OUTPUT"),
            Some(&"file.txt".to_string())
        );
    }

    #[test]
    fn test_value_flag_short() {
        let cmd = make_test_command();
        let raw = vec!["-o".to_string(), "file.txt".to_string(), "prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        assert_eq!(
            parsed.env_flags.get("FLAG_OUTPUT"),
            Some(&"file.txt".to_string())
        );
    }

    #[test]
    fn test_missing_required_arg() {
        let cmd = make_test_command();
        let raw = vec![];
        let result = parse_args(&cmd, &raw);

        assert!(matches!(result, Err(ArgsError::MissingRequired(_))));
        if let Err(ArgsError::MissingRequired(name)) = result {
            assert_eq!(name, "env");
        }
    }

    #[test]
    fn test_invalid_arg_choice() {
        let cmd = make_test_command();
        let raw = vec!["staging".to_string()];
        let result = parse_args(&cmd, &raw);

        assert!(matches!(result, Err(ArgsError::InvalidChoice { .. })));
    }

    #[test]
    fn test_invalid_flag_choice() {
        let cmd = make_test_command();
        let raw = vec!["--format".to_string(), "xml".to_string(), "prod".to_string()];
        let result = parse_args(&cmd, &raw);

        assert!(matches!(result, Err(ArgsError::InvalidChoice { .. })));
    }

    #[test]
    fn test_default_values() {
        let cmd = make_test_command();
        let raw = vec!["prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        // region should have default
        assert_eq!(
            parsed.env_args.get("ARG_REGION"),
            Some(&"us-west".to_string())
        );
        // output flag should have default
        assert_eq!(
            parsed.env_flags.get("FLAG_OUTPUT"),
            Some(&"stdout".to_string())
        );
    }

    #[test]
    fn test_double_dash_separator() {
        let cmd = make_test_command();
        let raw = vec![
            "prod".to_string(),
            "--".to_string(),
            "--not-a-flag".to_string(),
        ];
        let parsed = parse_args(&cmd, &raw).unwrap();

        // --not-a-flag should be treated as positional
        assert_eq!(parsed.positional.len(), 2);
        assert_eq!(parsed.positional[1], "--not-a-flag");
    }

    #[test]
    fn test_unknown_flag() {
        let cmd = make_test_command();
        let raw = vec!["--unknown".to_string(), "prod".to_string()];
        let result = parse_args(&cmd, &raw);

        assert!(matches!(result, Err(ArgsError::UnknownFlag(_))));
    }

    #[test]
    fn test_missing_flag_value() {
        let cmd = make_test_command();
        let raw = vec!["--output".to_string()];
        let result = parse_args(&cmd, &raw);

        assert!(matches!(result, Err(ArgsError::MissingFlagValue(_))));
    }

    #[test]
    fn test_extra_positional_args() {
        let cmd = make_test_command();
        let raw = vec!["prod".to_string(), "us-east".to_string(), "extra1".to_string(), "extra2".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        // All positional args should be preserved
        assert_eq!(parsed.positional.len(), 4);
        assert_eq!(parsed.positional[0], "prod");
        assert_eq!(parsed.positional[1], "us-east");
        assert_eq!(parsed.positional[2], "extra1");
        assert_eq!(parsed.positional[3], "extra2");

        // First two should map to env vars
        assert_eq!(parsed.env_args.get("ARG_ENV"), Some(&"prod".to_string()));
        assert_eq!(parsed.env_args.get("ARG_REGION"), Some(&"us-east".to_string()));
    }

    #[test]
    fn test_all_boolean_flags_present() {
        let cmd = make_test_command();
        let raw = vec!["prod".to_string()];
        let parsed = parse_args(&cmd, &raw).unwrap();

        // All boolean flags should be present in env_flags, even if not passed
        assert!(parsed.env_flags.contains_key("FLAG_DRY_RUN"));
        assert!(parsed.env_flags.contains_key("FLAG_VERBOSE"));
        assert_eq!(
            parsed.env_flags.get("FLAG_DRY_RUN"),
            Some(&"false".to_string())
        );
        assert_eq!(
            parsed.env_flags.get("FLAG_VERBOSE"),
            Some(&"false".to_string())
        );
    }

    #[test]
    fn test_env_name_conversion() {
        assert_eq!(to_env_name("ARG", "env"), "ARG_ENV");
        assert_eq!(to_env_name("FLAG", "dry-run"), "FLAG_DRY_RUN");
        assert_eq!(to_env_name("ARG", "my-long-arg"), "ARG_MY_LONG_ARG");
    }
}

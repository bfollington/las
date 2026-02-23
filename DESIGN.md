# las (ལས) — a CLI that grows with your project

**las** discovers scripts in a `.commands/` folder and exposes them as a typed, self-documenting CLI. No build step, no config file — just files on disk that you or an agent can add, move, and edit while using it.

The name means "action" and "karma" in Tibetan. Your commands accumulate over time, each one shaping the project's vocabulary.

**Implementation:** Rust. Single binary, ~3 MB, sub-millisecond startup.

---

## The core loop

```
you (or an agent) drop a script into .commands/
    ↓
las discovers it instantly
    ↓
las deploy --help  ← works immediately
    ↓
you use it, notice friction, refine it
    ↓
repeat
```

Every `.commands/` folder becomes a project-specific vocabulary — the verbs and nouns that matter in *this* context. The agent isn't just a user of the CLI, it's a co-author. Over time, `las` becomes a record of how you work.

---

## What a command looks like

A command is a single executable shell script with optional commented YAML frontmatter:

```bash
#!/bin/bash
#---
# description: Deploy the app to a target environment
# args:
#   env:
#     description: Target environment
#     required: true
#     choices: [staging, production]
#   version:
#     description: Version tag to deploy
#     default: latest
# flags:
#   dry-run:
#     description: Show what would happen without doing it
#     short: n
#   verbose:
#     description: Show detailed output
#     short: v
#---

set -euo pipefail

if [ "$FLAG_DRY_RUN" = "true" ]; then
  echo "[dry run] would deploy $ARG_ENV @ $ARG_VERSION"
  exit 0
fi

echo "Deploying $ARG_VERSION to $ARG_ENV..."
```

### Frontmatter spec

Delimited by `#---` lines. Content is YAML with `# ` prefix on each line.

**Top-level fields:**

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `description` | string | no | Shown in help text. Without it: "No description" |
| `args` | map | no | Positional arguments, keyed by name, ordered as written |
| `flags` | map | no | Named `--flag` options |
| `stdin` | string | no | Describes expected piped input (for agent context) |

**Arg fields:**

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `description` | string | — | Shown in help |
| `required` | bool | false | Fail with error if missing |
| `default` | string | — | Used when not provided |
| `choices` | list | — | Constrain to specific values |

**Flag fields:**

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `description` | string | — | Shown in help |
| `short` | char | — | Single-letter alias (`-v`) |
| `value` | string | — | If present, flag takes a value (`--output file.txt`). The string describes the value. |
| `default` | string | — | Default when flag takes a value |
| `choices` | list | — | Constrain flag values |

Flags without `value` are boolean (present = true, absent = false).

### Bare minimum command

```bash
#!/bin/bash
echo "hello"
```

`las hello` runs it. `las hello --help` shows the file path and "No description." Zero friction to start.

---

## Folder structure → command hierarchy

```
.commands/
├── deploy.sh              → las deploy
├── db/
│   ├── migrate.sh         → las db migrate
│   ├── seed.sh            → las db seed
│   └── snapshot/
│       ├── create.sh      → las db snapshot create
│       └── restore.sh     → las db snapshot restore
├── test.sh                → las test
└── agent/
    ├── summarize.sh       → las agent summarize
    └── review.sh          → las agent review
```

### Discovery rules

1. On invocation, `las` walks up from `cwd` looking for a `.commands/` directory (like git looks for `.git/`).
2. Walk `.commands/` recursively. Each `.sh` file becomes a command. File name minus extension = command name.
3. Directory name = command group. Groups nest arbitrarily deep.
4. A directory with children but no script of its own is a pure group — invoking it prints its children's help.
5. Alphabetical ordering within groups by default.
6. Hidden files (`.foo.sh`) and files starting with `_` are ignored.
7. Files must be executable (`chmod +x`). `las` warns if it finds a non-executable `.sh` file.

### _group.yml (optional)

A `_group.yml` file in any directory provides group-level metadata:

```yaml
description: Database management commands
order: [migrate, seed, snapshot]
```

---

## How arguments flow into scripts

`las` sets environment variables before executing the script:

```
ARG_ENV=staging           # Positional args: ARG_ + UPPER_SNAKE name
ARG_VERSION=latest
FLAG_DRY_RUN=true         # Flags: FLAG_ + UPPER_SNAKE name
FLAG_VERBOSE=false         # Boolean flags are "true" or "false"
FLAG_OUTPUT=file.txt       # Value flags contain the value
```

Positional args are *also* passed as `$1`, `$2`, etc. so scripts work naturally either way.

Stdin is passed through transparently. If the user pipes input, the script receives it on stdin as normal.

### Exit codes

`las` forwards the script's exit code. If `las` itself fails (bad args, missing command), it uses:
- `1` — general error
- `2` — usage error (bad arguments, missing required args)
- `127` — command not found

---

## Built-in meta-commands

These are built into the `las` binary and cannot be overridden by user commands:

```
las                         Show top-level help
las --help                  Same as above
las <cmd> --help            Show help for a specific command
las --list                  List all commands with descriptions (tree view)
las --which <cmd>           Print the file path of a command
las --edit <cmd>            Open the command file in $EDITOR
las --new <cmd>             Create a new command from template
las --new <group>/<cmd>     Create inside a group (creates directory if needed)
las --skill                 Print the agent skill document to stdout
las --completions <shell>   Generate shell completions (bash, zsh, fish)
las --config                Show .commands/ location and settings
las --version               Print version
```

### las --new

```
$ las --new deploy/rollback
Created .commands/deploy/rollback.sh (chmod +x)
```

Generates:

```bash
#!/bin/bash
#---
# description: TODO describe this command
#---

echo "TODO: implement deploy rollback"
```

If `.commands/_config.yml` defines a custom template, uses that instead.

### las --skill

Prints a structured document (markdown) to stdout describing:

- Where `.commands/` lives and the folder → hierarchy convention
- The frontmatter format with all fields
- How to create, edit, and organize commands
- How args/flags are passed to scripts
- Examples of well-structured commands
- Guidance: prefer small, composable commands; use descriptive names

Designed to be piped into an agent's context:
```
las --skill | claude "create a command that..."
```

### las --completions

Generates shell completion scripts by walking the command tree and frontmatter:

```
$ las --completions bash > ~/.local/share/bash-completion/completions/las
$ las --completions zsh > ~/.zfunc/_las
$ las --completions fish > ~/.config/fish/completions/las.fish
```

Completions include command names, subcommand names, flag names, and `choices` values from frontmatter.

---

## Help output

### Top-level

```
$ las

las — project commands

COMMANDS
  deploy          Deploy the app to a target environment
  db              Database management commands
    migrate       Run pending database migrations
    seed          Load seed data
    snapshot      Manage database snapshots
  test            Run the test suite
  agent           AI agent commands
    summarize     Summarize recent changes
    review        Code review via LLM

META
  --help              Show this help
  --list              List all commands (tree)
  --which <cmd>       Show file path for a command
  --edit <cmd>        Open command in $EDITOR
  --new <cmd>         Create a new command
  --skill             Print agent skill document
  --completions <sh>  Generate shell completions
  --config            Show configuration

This CLI is extensible. Commands live in:
  /home/user/project/.commands/

Create new commands: las --new <name>
Agent skill document: las --skill
```

### Per-command

```
$ las deploy --help

las deploy — Deploy the app to a target environment

USAGE
  las deploy <env> [version] [flags]

ARGUMENTS
  env             Target environment (required)
                  choices: staging, production
  version         Version tag to deploy
                  default: latest

FLAGS
  -n, --dry-run   Show what would happen without doing it
  -v, --verbose   Show detailed output

SOURCE
  .commands/deploy.sh
```

The `SOURCE` line tells an agent exactly which file to read or modify.

---

## Hooks (.commands/_hooks.sh, optional)

A `_hooks.sh` file in `.commands/` (or any subdirectory) defines lifecycle hooks:

```bash
# Runs before any command in this directory (and subdirectories)
before() {
  # Source environment
  if [ -f .env ]; then
    set -a
    source .env
    set +a
  fi
}

# Runs after any command in this directory (and subdirectories)
after() {
  # Cleanup, logging, etc.
  :
}
```

Rules:
- `_hooks.sh` in `.commands/` applies to all commands
- `_hooks.sh` in a subdirectory applies to that group and its children
- Hooks are inherited: a command in `db/` runs the root hooks *then* the `db/` hooks
- `before` runs before the command; if it exits nonzero, the command is skipped
- `after` runs after the command regardless of exit code; receives the command's exit code as `$1`
- Hooks receive the same `ARG_*` and `FLAG_*` environment variables as the command

---

## Config (.commands/_config.yml, optional)

```yaml
# Name shown in help output (defaults to "las")
name: myproject

# Shell to use for .sh files (defaults to /bin/bash)
shell: /bin/bash

# Template for --new (defaults to built-in)
template: |
  #!/bin/bash
  #---
  # description: TODO
  #---
  echo "TODO: implement"
```

---

## What las does NOT do

- **No build step.** Changes are live immediately.
- **No daemon.** Runs, executes, exits.
- **No plugin system.** Commands are the extension mechanism.
- **No package management.** Copy folders in and out.
- **No opinions about what scripts do.** It's a runner, not a framework.
- **No remote anything.** Local files only.
- **No non-shell scripts (v1).** `.sh` only for now. Other interpreters can be added later via extension-based dispatch.

---

## Rust implementation notes

### Crate candidates

- **clap** — for parsing `las` *own* flags (--help, --new, etc.). Not for user commands — those are parsed by `las` directly from frontmatter.
- **serde + serde_yaml** — frontmatter deserialization
- **walkdir** — recursive directory traversal
- **termtree** or manual — tree display for `--list`
- **clap_complete** — shell completion generation

### Architecture sketch

```
main.rs
├── cli.rs          — parse las's own args, dispatch to action
├── discovery.rs    — walk .commands/, build command tree
├── frontmatter.rs  — parse #--- delimited commented YAML
├── command.rs      — command struct, arg/flag definitions
├── runner.rs       — execute scripts (Command::new, env vars, stdio)
├── help.rs         — format and print help output
├── completions.rs  — generate shell completion scripts
├── hooks.rs        — discover and execute _hooks.sh
├── skill.rs        — generate the agent skill document
└── new.rs          — scaffold new command files
```

### Frontmatter parsing

1. Read file as string
2. Find first `#---` line, find second `#---` line
3. Extract lines between, strip leading `# ` from each
4. Parse resulting string as YAML via serde_yaml
5. Everything after the second `#---` is the script body (not needed by `las`, but useful for `--skill` introspection)

### Script execution

```rust
Command::new("bash")
    .arg("-c")
    .arg(&script_path)
    .args(&positional_args)         // $1, $2, ...
    .envs(&arg_env_vars)            // ARG_FOO=bar
    .envs(&flag_env_vars)           // FLAG_BAR=true
    .stdin(Stdio::inherit())
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit())
    .status()
```

Transparent stdio inheritance means interactive tools (gum, fzf, etc.) work inside commands.

---

## Future directions (not v1)

- **Non-shell scripts:** `.py`, `.ts`, `.js`, `.rb` with extension-based interpreter dispatch and language-appropriate comment frontmatter
- **Aliases:** `aliases: [d, dep]` in frontmatter
- **Composition primitives:** `depends: [db migrate]` to run prerequisites
- **Watch mode:** Re-run on file changes
- **Agent integration:** Commands as BusyTown agents, event bus emission
- **Remote command packs:** `las --install <git-url>` to clone a commands folder

# las (ལས)

A CLI that grows with your project. Drop shell scripts into `.commands/` and `las` exposes them as a typed, self-documenting CLI — no build step, no config file.

The name means "action" and "karma" in Tibetan.

## Quick start

```bash
# Install
cargo install --path .

# Create a commands directory
mkdir .commands

# Create your first command
las --new greet

# Edit it
las --edit greet

# Run it
las greet
```

## How it works

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

## Commands are just shell scripts

A command is a single executable `.sh` file with optional YAML frontmatter:

```bash
#!/bin/bash
#---
# description: Deploy the app to a target environment
# usage: |
#   las deploy staging            # deploy latest to staging
#   las deploy production v1.2.3 --dry-run
# on-failure: Check `kubectl get pods` — a stuck rollout is the usual culprit.
# artifacts:
#   - /tmp/deploy.log
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

The optional documentation fields all surface where they're needed:

- `usage:` — free-form examples and caveats, rendered verbatim in `las <cmd> --help` and the
  `--skill` document. This is the densest documentation an agent gets — make examples copy-pasteable.
- `on-failure:` — a one-line remediation hint printed to stderr (`hint: ...`) whenever the
  command exits nonzero, so failures explain how to recover at the failure site.
- `artifacts:` — files the command produces. After a successful run, `las` prints a
  `-> /path/to/file` pointer for each, telling the caller what to read next.
- `variadic: true` on the **last** argument makes it collect all remaining words
  (`ARG_NAME` gets them space-joined; `$1`, `$2`, … still see individual words). Usage lines
  render it as `<name>...`.

Mistype a command name and `las` suggests the closest matches plus the full command list,
so a typo costs one glance instead of a round-trip through `--list`.

No frontmatter? That's fine too:

```bash
#!/bin/bash
echo "hello"
```

`las hello` runs it. Zero friction to start.

## Folder structure = command hierarchy

```
.commands/
├── deploy.sh              → las deploy
├── db/
│   ├── migrate.sh         → las db migrate
│   ├── seed.sh            → las db seed
│   └── _group.yml         → group description + ordering
├── test.sh                → las test
└── agent/
    ├── summarize.sh       → las agent summarize
    └── review.sh          → las agent review
```

## How arguments reach your script

`las` sets environment variables before executing:

```
ARG_ENV=staging           # Positional args: ARG_ + UPPER_SNAKE name
ARG_VERSION=latest
FLAG_DRY_RUN=true         # Boolean flags: "true" or "false"
FLAG_VERBOSE=false
FLAG_OUTPUT=file.txt      # Value flags contain the value
```

Positional args are also passed as `$1`, `$2`, etc. Stdin is passed through transparently.

## Built-in commands

```
las                         Show top-level help
las <cmd> --help            Show help for a specific command
las --list                  List all commands (tree view)
las --which <cmd>           Print the file path of a command
las --edit <cmd>            Open the command file in $EDITOR
las --new <cmd>             Create a new command from template
las --new <group>/<cmd>     Create inside a group
las --skill                 Print agent skill document
las --sync                  Write skill document to .claude/skills/ for agent auto-discovery
las --json                  Print all command metadata as JSON
las --suggest               Report command usage + repeated shell commands worth extracting
las --observe [cmd]         Record an external shell command for --suggest (args or stdin)
las --completions <shell>   Generate shell completions (bash, zsh, fish)
las --config                Show configuration
las --version               Print version
```

## Shell completions

```bash
# Bash
las --completions bash > ~/.local/share/bash-completion/completions/las

# Zsh
las --completions zsh > ~/.zfunc/_las

# Fish
las --completions fish > ~/.config/fish/completions/las.fish
```

## Agent integration

`las --skill` prints a structured document describing your commands, frontmatter format, and how to create new ones — designed to be piped into an agent's context:

```bash
las --skill | claude "create a command that runs database backups"
```

Better: `las --sync` writes that document (with skill frontmatter) to
`.claude/skills/<name>/SKILL.md` next to your `.commands/` directory, so agents like
Claude Code discover the whole command vocabulary automatically — zero turns spent on
`--help`, no reliance on the agent remembering to look. Re-run it after adding or
changing commands (or wire it into a `_hooks.sh` `after()` for `--new`):

```bash
las --sync
# Wrote /path/to/project/.claude/skills/las/SKILL.md
```

For tooling that wants structure instead of prose, `las --json` dumps every command's
full metadata — path, invocation, description, usage, args/flags with choices and
defaults, artifacts, failure hints, script path — as JSON:

```bash
las --json | jq -r '.commands[].invocation'
```

## The learning loop: history and `--suggest`

Every `las <cmd>` run is appended to `.commands/.history.jsonl` (kept out of version
control via an auto-maintained `.commands/.gitignore`). `las --suggest` reads it and
reports which commands earn their keep, which are never used, and — the interesting
part — which **raw shell commands repeat often enough to deserve extraction** into a
command of their own:

```
COMMAND USAGE
  gconsole        42 runs
  shot            17 runs, 2 failed
  never used:     gcall

EXTRACTION CANDIDATES (repeated raw shell commands)
    9x  pkill -f "MacOS/Godot --remote-debug"
    4x  rg TODO scripts/

Save one as a command: las --new <name>, then paste the shell line into the script.
```

Raw shell commands get into the history via `las --observe`, which accepts the command
as arguments, as a raw line on stdin, or as a Claude Code PostToolUse hook payload
(it extracts `.tool_input.command` itself). Wire it up once in `.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": "las --observe" }]
      }
    ]
  }
}
```

Agents are bad at noticing repetition across sessions — each one starts amnesiac.
The history makes repetition a queryable fact: an agent (or you) runs `las --suggest`
and the case for extracting a new command is already made. `--observe` is safe to hook
globally: outside a las project it's a silent no-op, and invocations of `las` itself
are skipped (they're already recorded as runs).

## Hooks

Add `_hooks.sh` to any directory for before/after lifecycle hooks:

```bash
before() {
  if [ -f .env ]; then
    set -a; source .env; set +a
  fi
}

after() {
  echo "Command exited with code $1"
}
```

Hooks are inherited — a command in `db/` runs root hooks, then `db/` hooks.

## Configuration

Optional `.commands/_config.yml`:

```yaml
name: myproject       # Name shown in help (default: las)
shell: /bin/bash      # Shell for .sh files (default: /bin/bash)
template: |           # Custom template for --new
  #!/bin/bash
  set -euo pipefail
  echo "TODO: implement"
```

## Install

```bash
cargo install --path .
```

Requires Rust 1.85+ (edition 2024).

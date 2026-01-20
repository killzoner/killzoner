# Usage

## Prerequisites

- Rust toolchain (1.85+)
- GitHub CLI (`gh`) or a GitHub Personal Access Token

## Setup

Using GitHub CLI (recommended):
```bash
export GITHUB_TOKEN=$(gh auth token)
```

Or create a PAT at https://github.com/settings/tokens with `read:user` scope:
```bash
export GITHUB_TOKEN="ghp_xxxx"
```

## Running

```bash
# With all defaults (username: killzoner, template: template.md)
cargo run

# Specify username
cargo run -- --username myuser

# Specify custom template
cargo run -- --template intro.md

# Inline token usage with gh CLI (generates README.md)
GITHUB_TOKEN=$(gh auth token) cargo run > README.md
GITHUB_TOKEN=$(gh auth token) cargo run -- --username myuser > README.md
```

## Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--username` | `-u` | killzoner | GitHub username |
| `--template` | `-t` | template.md | Introduction template file |

## Makefile

```bash
make run                    # Run with defaults
make run USERNAME=myuser    # Run with custom username
make generate               # Generate README.md
make ci                     # Run all CI checks
```

## Output

Repositories are sorted by year (descending), then by stars:

```markdown
## Open Source Contributions

- [owner/repo](https://github.com/owner/repo) ⭐1.2k (2024)
- [owner/repo2](https://github.com/owner/repo2) ⭐500 (2023)
```

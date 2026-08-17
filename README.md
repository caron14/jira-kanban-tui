# jira-kanban-tui

A focused Jira Kanban TUI for checking project health and making the four updates used every day: Status, Assignee, Due date, and Priority.

## Install

Rust 1.75 or later is required.

```sh
cargo build --locked --release
./target/release/jira-kanban-tui
```

The first launch opens a two-step Setup:

1. Select Jira Cloud or Jira Data Center, then enter the URL and API Token. Jira Cloud also requires your email address.
2. Add one or more numeric Board IDs. Every Board is verified and shown by name before the Config is saved.

Tokens are stored in the OS keyring, never in the Config file.

## Daily use

The app starts on the Dashboard for the selected Board.

- `1` Board, `2` Dashboard, `3` WBS, `4` Activity
- `j/k` or `↑/↓` select an item
- `h/l` or `←/→` select a Board column; in WBS they collapse or expand
- `Enter` opens Issue details
- `e` opens one Edit menu: Status, Assignee, Due date, Priority
- `/` searches Key, Summary, and Assignee
- `f` filters by My Issues, Overdue, or Blocked
- `b` selects a configured Board
- `o` opens Jira, `r` refreshes, `?` shows contextual help, `q` quits

All edits require a visible choice or value followed by Enter. Cached data is clearly marked read-only and cannot be edited.

## Config

The default path is `~/.jira-kanban-tui.toml`; use `--config` to select another file.

```toml
version = 3

[jira]
url = "https://company.atlassian.net"
auth = "cloud_basic_api_token"
username = "you@example.com"
board_ids = [123, 456]
token_env = "JIRA_API_TOKEN" # optional fallback
token_command = ["op", "read", "op://Engineering/Jira/token"] # optional fallback
```

For Jira Data Center, use `auth = "data_center_bearer_pat"` and omit `username`.

Credential lookup order is OS keyring, `token_env`, then `token_command`. The command is an argv array and is never passed through a shell.

Config v2 is migrated automatically only when it contains exactly one Jira source. The original is retained as `<config>.v2.bak`. Configs with multiple sources or GitHub/Linear are left untouched with a specific migration error so no configuration is silently discarded.

## Diagnose

```sh
jira-kanban-tui doctor
jira-kanban-tui --config /path/to/config.toml doctor
jira-kanban-tui --board 123
```

`doctor` checks the Config, credential, current user, and every configured Board without printing secrets. `--board` is an ephemeral override for one launch.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo build --locked --release
```

Logs are written to `~/.jira-kanban-tui.log` with mode `0600`; credentials are not logged.

## License

MIT — see [LICENSE](LICENSE).

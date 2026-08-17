# jira-kanban-tui

A focused Jira Kanban TUI for checking project health and making the four updates used every day:
Status, Assignee, Due date, and Priority.

## Requirements

- Jira Software Cloud or Jira Software Data Center
- A Jira account that can view the configured Boards and Issues
- Permission to edit Status, Assignee, Due date, or Priority when those updates are needed
- A terminal of at least 80×24 cells
- Rust 1.75 or later when building from source

## Install

```sh
cargo build --locked --release
./target/release/jira-kanban-tui
```

The first launch opens a two-step Setup:

1. Select Jira Cloud or Jira Data Center, then enter the base URL and API Token or PAT. Jira
   Cloud also requires your email address.
2. Add one or more numeric Board IDs. Every Board is verified and shown by name before the Config
   is saved.

Open a Board in Jira to find its numeric ID in the URL. Depending on the Jira version, it may appear
as `rapidView=123` or `/boards/123`. If the URL does not expose it, ask your Jira administrator.

Setup controls:

- `Tab` / `Shift+Tab` moves between fields; `←/→` selects Cloud or Data Center
- `Enter` verifies the connection or adds the entered Board
- `Delete` removes the last Board; `Ctrl+S` saves and opens the Dashboard
- `Ctrl+T` toggles Token visibility; `Esc` asks before quitting Setup

Setup stores the Token in the OS keyring and never writes it to the Config file. If a usable keyring
is not available, create the Config manually and use `token_env` or `token_command` instead. Do not
put a Token directly in `token_command` arguments.

## Daily use

The app starts on the Dashboard for the selected Board.

- `1` Board, `2` Dashboard, `3` WBS, `4` Activity
- `j/k` or `↑/↓` select an item
- `h/l` or `←/→` select a Board column; in WBS they collapse or expand
- `Enter` opens Issue details
- `e` opens one Edit menu: Status, Assignee, Due date, Priority
- In the Board view, `/` searches Key, Summary, and Assignee
- In the Board view, `f` filters by My Issues, Overdue, or Blocked
- `b` selects a configured Board
- `o` opens Jira, `r` refreshes, and `?` shows contextual help
- `q` quits the main UI; Setup uses `Esc` so `q` can be entered normally

All edits require a visible choice or value followed by Enter. Cached data is clearly marked
read-only and cannot be edited. Mouse input can select tabs and items or scroll; dragging never
updates Jira.

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

Credential lookup order is OS keyring, `token_env`, then `token_command`. The command is an argv
array and is never passed through a shell.

Config v2 is migrated automatically only when it contains exactly one Jira source. The original is
retained as `<config>.v2.bak`. Configs with multiple sources or GitHub/Linear are left untouched with
a specific migration error so no configuration is silently discarded.

## Diagnose

```sh
jira-kanban-tui doctor
jira-kanban-tui --config /path/to/config.toml doctor
jira-kanban-tui --board 123
```

`doctor` checks the Config, credential, current user, and every configured Board without printing
secrets. `--board` is an ephemeral override for one launch.

## Local data and privacy

- Config: `~/.jira-kanban-tui.toml`; contains connection settings but no Token
- Credentials: OS keyring, or the configured environment variable or command
- Cache: `~/.jira-kanban-tui-cache/`; contains Board and Issue data for read-only offline use
- Log: `~/.jira-kanban-tui.log`; credentials are not logged

On Unix, Config, cache files, migration backups, and the log are written with mode `0600`. Treat the
cache as project data and do not commit or share it.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
cargo +1.75.0 check --locked
cargo build --locked --release
```

Before release, also complete the [release gates](docs/release-checklist.md) and
[terminal compatibility checks](docs/compatibility.md).

## License

MIT — see [LICENSE](LICENSE).

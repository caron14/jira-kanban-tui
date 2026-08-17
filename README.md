# jira-kanban-tui

A focused terminal app for Jira Software. Check project health, browse a Board, review its WBS and
recent activity, and make the four updates commonly needed during the day:

- Status
- Assignee
- Due date
- Priority

Edits always require an explicit selection or value followed by Enter. Mouse dragging never changes
Jira, and cached data is read-only.

## Get started

### 1. Prepare Jira access

You need:

- Jira Software Cloud, or Jira Software Data Center 8.14 or later
- Access to the Boards and Issues you want to view
- Jira permission to edit any of the four supported fields you intend to change
- The numeric ID of at least one Jira Board

For Jira Cloud, create an API token in your
[Atlassian account](https://support.atlassian.com/atlassian-account/docs/manage-api-tokens-for-your-atlassian-account/).
Use a token without scopes; scoped tokens require a different Atlassian API URL that this app does
not currently use.

For Jira Data Center, create a Personal Access Token from Profile > Personal access tokens. See
[Atlassian's PAT guide](https://confluence.atlassian.com/enterprise/using-personal-access-tokens-1026032365.html).

To find a Board ID, open the Board in Jira and look for the number in its URL. Depending on your Jira
version, it may appear as `rapidView=123` or `/boards/123`. Ask your Jira administrator if the Board
URL does not expose it.

### 2. Install and run

Install [Rust](https://www.rust-lang.org/tools/install) 1.75 or later and Git, then install the app
from the repository:

```sh
git clone https://github.com/sho/jira-kanban-tui.git
cd jira-kanban-tui
cargo install --locked --path .
jira-kanban-tui
```

`cargo install` normally places the executable in `~/.cargo/bin`. If your shell cannot find
`jira-kanban-tui`, add that directory to `PATH` as described by the Rust installer, or run
`~/.cargo/bin/jira-kanban-tui` directly.

To try the app without installing it, run this from the repository instead:

```sh
cargo run --locked --release
```

Use a terminal window of at least 80 columns by 24 rows. The first launch starts the guided Setup.

### 3. Complete Setup

The first launch opens a two-step Setup:

1. Choose Jira Cloud or Data Center, then enter the Jira base URL and Token. Jira Cloud also asks
   for your Atlassian account email address.
2. Enter each Board ID. The app verifies every Board and shows its name before saving.

| Action | Key |
| --- | --- |
| Move between fields | `Tab` / `Shift+Tab` |
| Choose Cloud or Data Center | `Left` / `Right` |
| Verify the connection or add a Board | `Enter` |
| Remove the last added Board | `Delete` |
| Show or hide the Token | `Ctrl+T` |
| Save and open the Dashboard | `Ctrl+S` |
| Quit Setup | `Esc`, then confirm |

Setup stores the Token in the OS keyring and never writes it to the Config file. If no usable
keyring is available, use the manual credential options described below.

## Daily use

The app opens on the Dashboard for the selected Board.

A typical workflow is: press `1` to open the Board, select an Issue with `j` / `k`, press `Enter`
to inspect it, and press `e` to edit it. Select an edit and confirm the new value with `Enter`.
Press `Esc` to close any dialog without continuing.

| Action | Key |
| --- | --- |
| Open Board, Dashboard, WBS, or Activity | `1`, `2`, `3`, `4` |
| Move through Issues or activity | `j` / `k` or `Down` / `Up` |
| Move between Board columns | `h` / `l` or `Left` / `Right` |
| Collapse or expand a WBS item | `h` / `l` or `Left` / `Right` |
| Open Issue details | `Enter` |
| Edit Status, Assignee, Due date, or Priority | `e` |
| Open the selected Issue in Jira | `o` |
| Choose another configured Board | `b` |
| Refresh | `r` |
| Show contextual help | `?` |
| Quit | `q` or `Ctrl+C` |

Search and filters are available in the Board view:

- `/` searches Issue Key, Summary, and Assignee
- `f` filters by My Issues, Overdue, or Blocked

Mouse input can select tabs and items or scroll. It cannot update an Issue.

When editing an Issue:

- Status and Priority are selected with `j` / `k` or `Down` / `Up`, then confirmed with `Enter`.
- Type to search for an Assignee; press `Delete` to unassign the Issue.
- Enter a Due date as `YYYY-MM-DD`; submit an empty value to clear it.
- Updates are sent only after an explicit value or choice is confirmed with `Enter`.

## If something goes wrong

Run the built-in diagnostic before changing the Config:

```sh
jira-kanban-tui doctor
```

It checks the Config, credential, current Jira user, and every configured Board without printing
secrets. For a non-default Config or a one-time Board override, use:

```sh
jira-kanban-tui --config /path/to/config.toml doctor
jira-kanban-tui --board 123
```

Common behavior:

- Missing or invalid credentials reopen Setup so they can be repaired.
- A network failure uses the last cache when available and clearly marks it read-only.
- A terminal smaller than 80x24 shows the required size instead of a broken layout.
- `-v` and `-vv` increase log detail when diagnosing a problem.

## Manual Config and credentials

Most users do not need to edit the Config. Its default path is `~/.jira-kanban-tui.toml`; use
`--config` to select another file.

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
array and is never passed through a shell. Do not place a Token directly in the command arguments.

If the OS keyring is unavailable, create the Config above at `~/.jira-kanban-tui.toml` and choose
one credential fallback. For `token_env`, set the named environment variable before starting the
app:

```sh
printf "Jira Token: "
read -r -s JIRA_API_TOKEN
printf "\n"
export JIRA_API_TOKEN
jira-kanban-tui doctor
jira-kanban-tui
unset JIRA_API_TOKEN
```

The `read` command above accepts the Token without displaying it or storing it in shell history.
Alternatively, configure `token_command` to retrieve it from a password manager, as in the Config
example.

### Config v2 migration

Config v2 is migrated automatically only when it contains exactly one Jira source. The original is
kept as `<config>.v2.bak`. Configs with multiple sources or GitHub/Linear are left untouched and show
a migration error, so unsupported configuration is never silently discarded.

## Local data and privacy

| Data | Default location | Contents |
| --- | --- | --- |
| Config | `~/.jira-kanban-tui.toml` | Jira URL, authentication type, and Board IDs; no Token |
| Credentials | OS keyring | API Token or PAT entered during Setup |
| Cache | `~/.jira-kanban-tui-cache/` | Board and Issue data for read-only offline use |
| Log | `~/.jira-kanban-tui.log` | Diagnostics with credentials excluded |

On Unix, Config, cache files, migration backups, and the log are written with mode `0600`. Treat the
cache as project data and do not commit or share it.

## Update or uninstall

From an existing repository checkout, update and reinstall with:

```sh
git pull --ff-only
cargo install --locked --path . --force
```

To remove the installed executable:

```sh
cargo uninstall jira-kanban-tui
```

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

MIT. See [LICENSE](LICENSE).

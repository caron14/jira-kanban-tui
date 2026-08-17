# Release gates

## Behavior

- [ ] Config v3 Setup works with Jira Cloud and Data Center
- [ ] Safe Config v2 migration retains a backup and never drops unsupported sources
- [ ] Multiple Board IDs are verified and selected by Board name
- [ ] Dashboard starts first and is scoped to the selected Board
- [ ] Board, Dashboard, WBS, and Activity share selection/detail/open behavior
- [ ] Status, Assignee, Due date, and Priority use the single staged Edit menu
- [ ] Unknown Status Issues remain selectable in the `Other` column
- [ ] Every error action works; unavailable actions are absent

## Quality

- [ ] Initial frame <100 ms before waiting for Jira
- [ ] Local selection/modal/search/filter operations <16 ms
- [ ] RSS <50 MB
- [ ] 500-Issue Board remains responsive
- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test --all-targets`
- [x] `cargo build --locked --release`
- [ ] All terminal checks in `docs/compatibility.md`

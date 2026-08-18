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

## Distribution

- [ ] `Cargo.toml` contains the intended release version and the corresponding `vX.Y.Z` tag does
      not already exist
- [ ] `install.sh` passes ShellCheck and `tests/install_test.sh`
- [ ] The Release workflow publishes macOS and Linux archives for x86_64 and arm64
- [ ] `checksums.txt` contains exactly one SHA-256 checksum for every archive
- [ ] Every archive and installer has a GitHub artifact attestation
- [ ] A clean macOS and Linux environment can install with the documented curl command
- [ ] The installed binary reports the tagged version and starts `doctor`

## Publishing

1. Complete the behavior, quality, terminal compatibility, and distribution gates.
2. Update `Cargo.toml` to the next unused semantic version and commit the change.
3. Create and push the matching tag, for example `git tag v0.1.1` followed by
   `git push origin v0.1.1`.
4. Confirm that the Release workflow creates a draft, uploads and attests all six assets, and only
   then publishes the GitHub Release.
5. Test the latest and version-pinned installer commands from the published Release.

Enable immutable releases in the GitHub repository settings when available. The workflow assembles
all assets in a draft before publishing so it remains compatible with immutable releases.

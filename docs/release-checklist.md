# Release gates

## Behavior

- [ ] Config v3 Setup works with Jira Cloud and Data Center
- [ ] Safe Config v2 migration retains a backup and never drops unsupported sources
- [ ] Multiple Board IDs are verified and selected by Board name
- [ ] Dashboard starts first and is scoped to the selected Board
- [ ] Board, Dashboard, WBS, and Activity share selection/detail/open behavior
- [ ] Status, Assignee, Due date, and Priority are editable directly from Issue details
- [ ] Unknown Status Issues remain selectable in the `Other` column
- [ ] Every error action works; unavailable actions are absent

## Quality

- [ ] Initial frame <100 ms before waiting for Jira
- [ ] Local selection/modal/search/filter operations <16 ms
- [ ] RSS <50 MB
- [x] 500-Issue Board remains responsive
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --locked --all-targets -- -D warnings`
- [x] `cargo test --locked --all-targets`
- [x] `cargo build --locked --release`
- [ ] All terminal checks in `docs/compatibility.md`

## Distribution

- [x] `Cargo.toml` contains the intended release version and the corresponding `vX.Y.Z` tag does
      not already exist
- [ ] `install.sh` and `tests/install_test.sh` pass ShellCheck
- [x] `tests/install_test.sh`
- [ ] The Release workflow publishes macOS and Linux archives for x86_64 and arm64
- [ ] `checksums.txt` contains exactly one SHA-256 checksum for every archive
- [ ] Every archive and installer has a GitHub artifact attestation
- [ ] A clean macOS and Linux environment can install with the documented curl command
- [ ] The installed binary reports the tagged version and starts `doctor`

## Publishing

1. Complete the behavior, quality, terminal compatibility, and distribution gates.
2. Update `Cargo.toml` to the next unused semantic version, then commit and push the release branch.
3. Run the Release workflow manually from that branch and confirm all four build jobs succeed. A
   manual run builds and packages every target but does not create a GitHub Release.
4. Create and push the matching tag, for example `git tag v0.1.2` followed by
   `git push origin v0.1.2`.
5. Confirm that the tag-triggered workflow creates a draft, uploads and attests all six assets, and
   only then publishes the GitHub Release.
6. Test the latest and version-pinned installer commands from the published Release.

Enable immutable releases in the GitHub repository settings when available. The workflow assembles
all assets in a draft before publishing so it remains compatible with immutable releases.

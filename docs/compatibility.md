# Terminal compatibility

Minimum supported size is 80×24. At smaller sizes the app shows the required dimensions instead of rendering a broken interface.

Run this checklist in Ghostty, iTerm2, WezTerm, Kitty, Alacritty, a standard Linux terminal, and an SSH session:

- [ ] Setup accepts typing and paste, including `t`, `q`, CJK text, and masked Tokens
- [ ] `1/2/3/4` switches Board, Dashboard, WBS, and Activity
- [ ] Keyboard navigation reaches every Issue and keeps the selection visible
- [ ] Mouse selects tabs/items and scrolls; dragging never updates Jira
- [ ] Board names, cards, CJK summaries, warning text, and modal borders remain aligned
- [ ] Resize from 80×24 to a large window and back without panic or stale layout
- [ ] Search and the three Filters show zero and many results correctly
- [ ] Cached data is marked read-only and does not expose Edit
- [ ] `q` and Ctrl+C restore raw mode, alternate screen, paste mode, mouse capture, and cursor

Automated baseline: `cargo test --all-targets`.

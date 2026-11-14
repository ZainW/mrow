# Changelog

What changed in each release of the Mrow app.
## 0.1.1 (2026-11-14)

### New

- A disk image for macOS beside the archive, to download and drag into Applications (#9)

### Fixed

- Sign and notarize the macOS release with a Developer ID (#11)

## 0.1.0 (2026-11-13)

### New

- Mrow, the reviews waiting on you from GitLab and GitHub, for macOS and Linux
- Ask Claude, opencode or pi, and post the comments they draft after you approve them (#4)
- One Mrow per user on Linux; Rust 1.100, the latest Rune and newer dependencies (#5)
- Mine, your own requests by whose move it is, with stacks that fold to what's on you
- Merge your own requests, turn on auto-merge, mark a draft ready, and merge a stack from the bottom up
- An inbox behind the bell for what changed, and notifications per kind of event, on macOS and Linux
- Charis SIL for reading text on macOS and Linux, and type sizes on whole points
- A review as one page with a contents rail, thread counts per file, and J and K to step through it
- Your own requests on the one page too, ending with what's left before they merge, and a slimmer contents rail
- A narrower queue by default, with tabs that wrap instead of clipping when it is dragged narrow

### Faster

- A contents rail that builds only the rows in sight, so a request touching thousands of files scrolls like a small one


# Command palette

A modal search over commands and merge requests. Typing filters both groups ("Commands" and "Go
to"), the arrow keys choose, Enter runs the choice, and Esc closes the palette.

## Sub-features

- `palette-open`: the chord or the menu opens the dialog with its search field focused.
- `palette-filter`: typing narrows the list, and the first match is selected.
- `palette-run`: Enter runs the selected command (for example, "Show Mine" switches the sidebar list).
- `palette-goto`: picking a "Go to !N" row opens that request.
- `palette-close`: Esc closes it without running anything.

## How to get to it (user POV)

- Ctrl+Shift+P on Linux, ⇧⌘P on macOS.
- macOS menu bar: the "Command Palette…" item.
- The `palette` and `palette-query` scenes start with it already open. Use them for looks only.

## Driving it with mrow-verify

Preconditions: the `to-review` scene, with To review selected.

- **Open and filter.** Script `[{"t":0.3,"type":"key","name":"p","cmd":true,"shift":true},{"t":0.8,"type":"type","text":"mine"},{"t":1.5,"type":"mark","name":"end"}]`.
  The a11y file then shows `dialog palette 'Command Palette' [modal]`,
  `text-input input 'Search commands and merge requests' value='mine' [focused]` and
  `list-box-option cmd-mode-2 'Show Mine, Ctrl+3' [selected]`.
- **Run a command.** Add `{"t":1.4,"type":"key","name":"Enter"}` and move `end` to 2.2. The `palette`
  lines are gone. `$V texts` shows the sidebar headers `rows/h-Your move`,
  `h-Waiting on reviewers` and `h-Drafts`, where the before run had `h-Today`, `h-Yesterday` and `h-This week`.
  In the PNG, "Mine 4" is highlighted and !231 is open with **Merge**.
- **Go to a request.** Type `301` and press Enter. The open tab becomes
  `tab-gitlab:fieldnotes/billing!301`.
- **Close.** Press Escape after opening. No `palette` lines remain, and the list is unchanged.

## Gotchas

- While the palette is open, the a11y tree holds only the dialog. Use `$V texts` for the page behind it.
- Commands that can't run now are left out of the list (`not_now` in `src/ui/actions.rs`), so a
  missing command can be correct. Check its precondition before reporting a bug.
- This journey was proven on Linux. Whether a `cmd` key reaches `on_key` headlessly on macOS is
  unverified, because macOS routes commands through the menu bar.

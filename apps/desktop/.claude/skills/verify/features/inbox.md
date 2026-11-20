# Inbox

The bell beside the account name shows how many events are new (approvals, review requests, ready
to merge, and so on). Clicking it opens a modal list of events. Clicking an event opens its request,
and "Mark All Done" clears the list.

## Sub-features

- `inbox-count`: the number on the bell (`inbox-bell` > `count`).
- `inbox-open`: the bell opens `dialog inbox 'Inbox'`.
- `inbox-row`: a row opens its request and closes the inbox.
- `inbox-done`: `mark-all-done` clears the new events, and the count goes away.
- `inbox-close`: Escape closes the dialog.

## How to get to it (user POV)

- Click the bell (`inbox-bell`, labelled "Inbox, N new").
- Desktop notifications (live only; `target/debug/mrow --banner` posts a fictional one and waits 15 s for a click).

## Driving it with mrow-verify

Preconditions: the `to-review` scene, where the bell reads "Inbox, 4 new".

- **Open.** Use `[{"t":0.3,"type":"tap","key":"inbox-bell"},{"t":1.0,"type":"mark","name":"end"}]`. The a11y file shows
  `dialog inbox 'Inbox' [modal]`, `button mark-all-done 'Mark All Done'`, and rows such as
  `'Ready to merge. Drop the legacy export endpoint, ledger!231'`.
- **Mark all done.** After opening, tap `mark-all-done`. The `count` text under `inbox-bell` is gone
  (`$V texts … | grep inbox-bell`), and the bell's label no longer says "N new".
- **Open an event.** Tap a row at its `rect` centre. The inbox closes and the open tab names that
  request (`tab-gitlab:fieldnotes/ledger!231`).

## Gotchas

- Row keys hold a timestamp generated at scene load (`…|Ready|1795173488`) that changes on every
  run, so never hard-code them. Find the row by its label in this run's tree.
- The `inbox`, `inbox-mine` and `inbox-empty` scenes start with it open, so use them for looks only.
- Proving a real click on a notification needs the live desktop and a person.

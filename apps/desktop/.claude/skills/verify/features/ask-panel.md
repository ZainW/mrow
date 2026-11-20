# Ask panel

A side panel for chatting with a coding agent (Cursor, Claude Code, OpenCode or Pi) about the open
request. It has a greeting, suggestion chips, a composer with a model picker, and a clone status.
Comments the agent drafts post only when the user approves them.

## Sub-features

- `ask-open`: `ask-toggle` in the header opens `complementary ask 'Ask <Agent>'`.
- `ask-chips`: suggestions such as `suggest-Summarize` and `suggest-What could break?`.
- `ask-agent`: the header button switches agent.
- `ask-model`: the model picker in the composer.
- `ask-chat`: asking, a streamed answer, and drafted comments. These are live only.

## How to get to it (user POV)

- Click the chat icon in the header (`ask-toggle`, labelled "Ask Cursor").
- Its command in the palette or menu (`COMMANDS` in `src/ui/actions.rs`).

## Driving it with mrow-verify

Preconditions: the `to-review` scene.

- **Open.** Use `[{"t":0.3,"type":"tap","key":"ask-toggle"},{"t":1.0,"type":"mark","name":"end"}]`. The a11y file gains
  `complementary ask 'Ask Cursor'`, and `$V texts` shows `ask-greeting` "Ask about !214" and the chips.
- **Chat look.** The `ask-chat`, `ask-steer` and `ask-comment` scenes render fixed conversations.
  They're for appearance only.
- **Live agent.** `target/debug/mrow --ask-models "<question>" --agent claude|cursor|opencode|pi`
  (no window) lists the models and asks once in an empty folder. It needs that agent installed and
  signed in, and it spends the user's agent quota, so ask first.

## Gotchas

- Headless Ask is offline: the clone status stays "Looking…", and sending gets no answer.
  Don't count that as a failure.
- The model shown comes from the fixture or the defaults, not from the user's setup.

import type { ReactNode } from 'react'
import { Keys } from './Chapter'
import { StandingMark } from './AppMock'
import { reviews } from './mockData'
import type { Review } from './mockData'

// Small illustrations for the home page's feature cards, drawn from the same fictional queue as the window.
const github = reviews.github
const need = (id: string): Review => {
  const r = github.find((x) => x.id === id)
  if (!r) throw new Error(`No made-up request ${id}`)
  return r
}

/** Wraps a card's illustration: a picture with a label, not content to read or click. */
export function Art({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="feat-art" role="img" aria-label={label}>
      <div className="feat-art-in" inert>
        {children}
      </div>
    </div>
  )
}

export function AskArt() {
  const r = need('gh-482')
  const file = r.files[0]
  const name = file?.path.split('/').pop() ?? ''
  return (
    <Art
      label={`A chat with an agent. It read ${name} and drafted a comment on line ${r.draftLine}: “${r.draftComment}” It is marked Not posted, with Edit and Post buttons.`}
    >
      <div className="art-ask">
        <p className="art-ask-q">Draft a comment for anything that could go wrong here.</p>
        <p className="chat-tool">Read {name}</p>
        <div className="draft">
          <p className="draft-head">
            <span>
              Draft comment · On {name} line {r.draftLine}
            </span>
            <span className="spacer" />
            <span className="quiet">Not posted</span>
          </p>
          <p className="draft-body">{r.draftComment}</p>
          <div className="actions end">
            <span className="act">Edit</span>
            <span className="act primary">Post</span>
          </div>
        </div>
      </div>
    </Art>
  )
}

export function MemoryArt() {
  return (
    <Art label="Memory in use: Mrow, the whole app with a 12,000-line diff open, 33 megabytes. One GitHub tab, 157 megabytes. Same scale.">
      <div className="art-mem">
        <p className="art-mem-num">
          33 <span>MB</span>
        </p>
        <p className="art-mem-what">the whole app, a 12,000-line diff open</p>
        <div className="art-bars">
          <div className="art-bar-row">
            <span>Mrow</span>
            <span className="art-bar">
              <span className="art-bar-fill ours" style={{ width: `${(33 / 157) * 100}%` }} />
            </span>
            <span className="mono">33 MB</span>
          </div>
          <div className="art-bar-row">
            <span>One GitHub tab</span>
            <span className="art-bar">
              <span className="art-bar-fill" style={{ width: '100%' }} />
            </span>
            <span className="mono">157 MB</span>
          </div>
        </div>
      </div>
    </Art>
  )
}

export function TabsArt() {
  return (
    <Art label="A review's bar with three tabs, Overview, Changes and Conversation. Under the Changes tab, a changed line has a thread with a comment and the buttons Reply and Resolve.">
      <div className="art-tabs">
        <div className="art-tabbar">
          <span>Overview</span>
          <span className="on">Changes</span>
          <span>Conversation</span>
        </div>
        <div className="art-diff">
          <p className="rem">
            <b>−</b> const [tab] = useState('open')
          </p>
          <p className="add">
            <b>+</b> const [tab] = useSearchParam('tab')
          </p>
        </div>
        <div className="art-thread">
          <p>Does this keep the filter on reload?</p>
          <div className="actions">
            <span className="act">Reply</span>
            <span className="act ok">Resolve</span>
          </div>
        </div>
      </div>
    </Art>
  )
}

const MOVES: [string, Review][] = [
  ['Your move', need('gh-490')],
  ['Waiting on reviewers', need('gh-1212')],
  ['Drafts', need('gh-503')],
]

export function MoveArt() {
  return (
    <Art label="Your own requests in three groups. Your move: Show the filter in the page title, approved by 1, ready to merge. Waiting on reviewers: Add an index for invoice lookups, waiting on Marcus, 3 days. Drafts: Saved views 4, a draft nobody has been asked about.">
      <div className="art-move">
        {MOVES.map(([group, r]) => (
          <div key={group}>
            <h4>{group}</h4>
            <p className="art-move-row">
              <StandingMark standing={r.standing ?? 'waiting'} />
              <span className="art-move-main">
                <span className="art-move-title">{r.title}</span>
                <span className={`art-move-step step-${r.standing}`}>{r.step}</span>
              </span>
            </p>
          </div>
        ))}
      </div>
    </Art>
  )
}

export function TokenArt() {
  return (
    <Art label="Mrow talks to gh, and gh talks to GitHub. Your token stays with gh.">
      <div className="art-chain">
        <span className="art-node">Mrow</span>
        <span className="art-link" />
        <span className="art-node art-node-key">
          <code>gh</code>
          <small>holds your token</small>
        </span>
        <span className="art-link" />
        <span className="art-node">GitHub</span>
      </div>
    </Art>
  )
}

const PALETTE: [string, string[]][] = [
  ['Approve', ['⇧⌘A']],
  ['Snooze Until Morning', ['⇧⌘S']],
  ['Keep Open in Tab', ['⌘T']],
  ['Inbox', ['⇧⌘I']],
]

export function PaletteArt() {
  return (
    <Art label="The command palette with the commands Approve, Snooze Until Morning, Keep Open in Tab and Inbox, each with its key.">
      <div className="art-palette">
        <p className="art-palette-input">
          <Keys keys={['⇧⌘P']} />
          <span>Run a command or go to a request</span>
        </p>
        {PALETTE.map(([name, keys], i) => (
          <p key={name} className={i === 1 ? 'art-palette-row on' : 'art-palette-row'}>
            <span>{name}</span>
            <Keys keys={keys} />
          </p>
        ))}
      </div>
    </Art>
  )
}

export function StackArt() {
  const stack = github.filter((r) => r.stack === 'Saved views')
  const word = { ready: 'Ready', held: 'Held', waiting: 'Waiting', draft: 'Draft' } as const
  return (
    <Art label="A stack of four requests that target each other’s branches, bottom first, over main: Saved views 1, ready to merge; 2, held until 1 merges; 3, waiting on a reviewer; 4, a draft. A Merge the Stack button sits beside the first.">
      <div className="art-stack">
        <p className="art-stack-base mono">main</p>
        {stack.map((p, i) => (
          <p key={p.id} className="art-stack-line">
            <StandingMark standing={p.standing ?? 'waiting'} />
            <span className="mono quiet">{p.number}</span>
            <span className="art-stack-title">{p.title}</span>
            {i === 0 ? <span className="act primary">Merge the Stack</span> : <span className="quiet">{word[p.standing as keyof typeof word]}</span>}
          </p>
        ))}
      </div>
    </Art>
  )
}

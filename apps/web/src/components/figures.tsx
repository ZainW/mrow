import { useState } from 'react'
import { AccountPicker, AskChat, BubbleIcon, Checklist, DiffView, InboxList, NextStep, ProviderMark, QueueRow, StackGroup, StandingMark } from './AppMock'
import { accounts, inbox, reviews } from './mockData'
import type { Review } from './mockData'

const gitlab = reviews.gitlab
const find = (id: string) => gitlab.find((r) => r.id === id)
const need = (id: string) => {
  const r = find(id)
  if (!r) throw new Error(`No made-up request ${id}`)
  return r
}

type MiniRow = { title: string; ref: string; reason: string; note: string; fresh?: boolean; old?: boolean }

/** The same queue twice: before and after someone asks again. */
export function QueueFigure() {
  const hooks = need('gl-88')
  const billing = need('gl-301')
  return (
    <div className="queue-figure">
      <div className="then-now">
        <MiniQueue
          when="Tuesday, 4:10 pm"
          groups={[
            { name: 'Today', rows: [{ title: hooks.title, ref: 'hooks!88', reason: 'Devika asked for your review', note: '2h' }] },
            { name: 'This week', rows: [{ title: billing.title, ref: 'billing!301', reason: 'Samuel asked for your review', note: '1d' }] },
          ]}
        />
        <MiniQueue
          when="Thursday, 9:40 am, after Devika pushes fixes"
          groups={[
            {
              name: 'Today',
              rows: [{ title: hooks.title, ref: 'hooks!88', reason: 'Devika asked again', note: '5m', fresh: true }],
            },
            {
              name: 'This week',
              rows: [{ title: billing.title, ref: 'billing!301', reason: 'Samuel asked for your review', note: '3d', old: true }],
            },
          ]}
        />
      </div>
      <dl className="lists">
        <div>
          <dt>To review</dt>
          <dd>Your turn: you haven’t reviewed it, or new commits reset your review.</dd>
        </div>
        <div>
          <dt>Reviewed</dt>
          <dd>You approved, commented or asked for changes, and it’s still open.</dd>
        </div>
        <div>
          <dt>Mine</dt>
          <dd>The ones you opened, by whose move it is.</dd>
        </div>
        <div>
          <dt>Done</dt>
          <dd>Merged or closed in the last 14 days. The window is yours to change.</dd>
        </div>
      </dl>
    </div>
  )
}

function MiniQueue({ when, groups }: { when: string; groups: { name: string; rows: MiniRow[] }[] }) {
  return (
    <div className="mini">
      <p className="mini-when">{when}</p>
      <div className="mini-list">
        {groups.map((g) => (
          <div key={g.name}>
            <h4>{g.name}</h4>
            {g.rows.map((r) => (
              <div key={r.ref} className={r.fresh ? 'mini-row fresh' : 'mini-row'}>
                <span className="mini-top">
                  <span className="mini-title">{r.title}</span>
                  <span className={r.old ? 'mini-age old' : 'mini-age'}>{r.note}</span>
                </span>
                <span className={r.fresh ? 'mini-reason again' : 'mini-reason'}>{r.reason}</span>
                <span className="mini-ref">{r.ref}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}

/** A review's page, part way down, with its contents rail. */
export function ReviewFigure() {
  const r = need('gl-214')
  const file = r.files[0]
  const rail: { label: string; file?: boolean; threads?: number; unseen?: boolean; note?: string; here?: boolean }[] = [
    { label: 'The ask' },
    { label: 'Description' },
    { label: 'The changes', note: '0 of 2' },
    { label: 'job.rs', file: true, threads: 1, unseen: true, here: true },
    { label: 'exports.md', file: true },
    { label: 'Conversation', note: '1' },
    { label: 'Your verdict' },
  ]
  return (
    <figure className="review-figure" aria-label="A review in Mrow: the contents rail on the left marks job.rs, the file being read, which has one thread with a reply you haven’t seen.">
      <div className="rail static" aria-hidden="true">
        <p className="rail-title">Contents</p>
        {rail.map((p) => (
          <span key={p.label} className={`rail-item${p.file ? ' file' : ''}`} aria-current={p.here ? 'location' : undefined}>
            <span className="rail-label">{p.label}</span>
            {p.threads && <span className={p.unseen ? 'rail-threads unseen' : 'rail-threads'}>
                <BubbleIcon filled={p.unseen} />
                {p.threads}
              </span>}
            {p.file && <span className="rail-viewed" />}
            {p.note && <span className="rail-note">{p.note}</span>}
          </span>
        ))}
      </div>
      <div className="review-figure-page">
        <p className="page-ref">
          {r.project} <b>{r.number}</b>
        </p>
        <p className="review-figure-title">{r.title}</p>
        {file && <DiffView file={file.path} lines={file.lines} />}
      </div>
    </figure>
  )
}

export function AccountsFigure() {
  return (
    <div className="accounts-figure">
      <div className="accounts-picker">
        <AccountPicker account={accounts.gitlab} forceOpen />
      </div>
      <ul className="accounts-pair">
        <li>
          <ProviderMark provider="gitlab" />
          <span>
            Merge requests, through <code>glab</code>
          </span>
        </li>
        <li>
          <ProviderMark provider="github" />
          <span>
            Pull requests, through <code>gh</code>
          </span>
        </li>
      </ul>
    </div>
  )
}

/** The Mine list: your move first, with a stack folded to what's on you. */
export function MineFigure() {
  const [selected, setSelected] = useState('gl-231')
  const [open, setOpen] = useState(false)
  const stack = gitlab.filter((r) => r.stack === 'Ledger paging')
  const shown = open ? stack : stack.filter((r) => r.standing === 'ready' || r.standing === 'needs' || r.id === selected)
  const row = (r: Review, inStack?: boolean) => (
    <QueueRow key={r.id} review={r} inStack={inStack} active={r.id === selected} unread={false} onSelect={() => setSelected(r.id)} onKeep={() => {}} />
  )
  return (
    <div className="mine-figure">
      <div className="sidebar figure-sidebar">
        <div className="marks" aria-hidden="true">
          <span className="mark-filter on">All</span>
          <span className="mark-filter">
            <StandingMark standing="ready" />2
          </span>
          <span className="mark-filter">
            <StandingMark standing="needs" />2
          </span>
          <span className="mark-filter">
            <StandingMark standing="waiting" />3
          </span>
          <span className="mark-filter">
            <StandingMark standing="draft" />1
          </span>
        </div>
        <div className="queue">
          <section className="queue-group">
            <h4>Your move</h4>
            {row(need('gl-231'))}
            <StackGroup name="Ledger paging" parts={stack} shown={shown} open={open} onToggle={() => setOpen((v) => !v)} row={(r) => row(r, true)} />
            {row(need('gl-92'))}
            {row(need('gl-90'))}
          </section>
          <section className="queue-group">
            <h4>Waiting on reviewers</h4>
            {row(need('gl-307'))}
          </section>
        </div>
      </div>
    </div>
  )
}

/** The top of one of your requests and the checklist at the bottom of its page. */
export function MergeFigure() {
  const r = need('gl-90')
  return (
    <div className="merge-figure">
      <div className="merge-figure-part">
        <p className="figure-label">Where it stands</p>
        <NextStep
          review={r}
          account={accounts.gitlab}
          walking={false}
          onAsk={() => {}}
          onCancel={() => {}}
          onMerge={() => {}}
          onStopWalk={() => {}}
          onAutoMerge={() => {}}
          onReady={() => {}}
          onGo={() => {}}
          onThreads={() => {}}
        />
      </div>
      <div className="merge-figure-part">
        <p className="figure-label">Before it merges</p>
        <Checklist checks={r.checks ?? []} />
      </div>
    </div>
  )
}

/** The bottom of a stack, ready to merge, with Merge the Stack. */
export function StackFigure() {
  const r = need('gl-240')
  const stack = gitlab.filter((p) => p.stack === r.stack)
  const [confirm, setConfirm] = useState<'merge' | 'stack'>()
  const [walking, setWalking] = useState(false)
  return (
    <div className="stack-figure">
      <NextStep
        review={r}
        above={stack[1]}
        account={accounts.gitlab}
        confirm={confirm}
        walking={walking}
        onAsk={setConfirm}
        onCancel={() => setConfirm(undefined)}
        onMerge={(kind) => {
          setConfirm(undefined)
          if (kind === 'stack') setWalking(true)
        }}
        onStopWalk={() => setWalking(false)}
        onAutoMerge={() => {}}
        onReady={() => {}}
        onGo={() => {}}
        onThreads={() => {}}
      />
      <div className="stack-card">
        <p className="stack-card-title">Stack · {r.stack}</p>
        <div className="stack-card-list">
          <span className="stack-card-base">main</span>
          {stack.map((p) => (
            <span key={p.id} className={p.id === r.id ? 'stack-line here' : 'stack-line'}>
              {p.standing && <StandingMark standing={p.standing} />}
              <span className="mono quiet">{p.number}</span>
              <span className="stack-line-title">{p.title}</span>
              <span className="quiet">{p.id === r.id ? 'This one' : { held: 'Held', waiting: 'Waiting', draft: 'Draft' }[p.standing as 'held'] ?? ''}</span>
            </span>
          ))}
        </div>
      </div>
    </div>
  )
}

export function InboxFigure() {
  return (
    <div className="inbox-figure">
      <div className="inbox-head">
        <span>Inbox</span>
        <span className="quiet-btn static">Mark All Done</span>
      </div>
      <InboxList events={inbox.gitlab} find={find} />
    </div>
  )
}

/** A chat in which the agent has drafted a comment you haven't posted. */
export function DraftFigure() {
  return <AskChat review={need('gl-214')} account={accounts.gitlab} />
}

/** What Mrow talks to. */
export function FlowFigure() {
  const paths: { via: string; to: string; when?: string }[] = [
    { via: 'glab', to: 'GitLab' },
    { via: 'gh', to: 'GitHub' },
    { via: 'agent', to: 'its model', when: 'when you ask' },
    { via: 'curl', to: 'GitHub releases', when: 'every few hours' },
    { via: 'curl', to: 'avatars and pictures', when: 'from where they’re hosted' },
  ]
  return (
    <div
      className="flow"
      role="img"
      aria-label="Mrow talks to glab, which talks to GitLab, and to gh, which talks to GitHub. When you ask, it talks to the agent you chose, which talks to its model. Release builds check GitHub releases for updates with curl every few hours, and avatars and pictures in a request are downloaded with curl from where they’re hosted."
    >
      <div className="flow-node flow-app">Mrow</div>
      <div className="flow-paths">
        {paths.map((p) => (
          <div key={p.to} className={p.when ? 'flow-path optional' : 'flow-path'}>
            <span className="flow-line" />
            <span className="flow-node">
              <code>{p.via}</code>
            </span>
            <span className="flow-line" />
            <span className="flow-node flow-far">
              {p.to}
              {p.when && <span className="flow-when">, {p.when}</span>}
            </span>
          </div>
        ))}
      </div>
    </div>
  )
}

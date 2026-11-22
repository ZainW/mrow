import { useState } from 'react'
import {
  AccountPicker,
  AskChat,
  Checklist,
  DiffView,
  FileList,
  InboxList,
  NextStep,
  pendingIn,
  ProviderMark,
  QueueRow,
  ReviewBar,
  ReviewPanel,
  StackGroup,
  StandingMark,
  ThreadView,
  threadsOf,
} from './AppMock'
import { accounts, inbox, reviews, shortRef } from './mockData'
import type { Review, Standing } from './mockData'

// The figures show the GitHub account, the one most people will use.
const github = reviews.github
const find = (id: string) => github.find((r) => r.id === id)
const need = (id: string) => {
  const r = find(id)
  if (!r) throw new Error(`No made-up request ${id}`)
  return r
}

type MiniRow = { title: string; ref: string; reason: string; note: string; fresh?: boolean; old?: boolean }

/** The same queue twice: before and after someone asks again. */
export function QueueFigure() {
  const api = need('gh-1207')
  const docs = need('gh-58')
  return (
    <div className="queue-figure">
      <div className="then-now">
        <MiniQueue
          when="Tuesday, 4:10 pm"
          groups={[
            { name: 'Today', rows: [{ title: api.title, ref: shortRef(api), reason: 'Marcus asked for your review', note: '2h' }] },
            { name: 'This week', rows: [{ title: docs.title, ref: shortRef(docs), reason: 'Ada asked for your review', note: '1d' }] },
          ]}
        />
        <MiniQueue
          when="Thursday, 9:40 am, after Marcus pushes fixes"
          groups={[
            {
              name: 'Today',
              rows: [{ title: api.title, ref: shortRef(api), reason: 'Marcus asked again', note: '5m', fresh: true }],
            },
            {
              name: 'This week',
              rows: [{ title: docs.title, ref: shortRef(docs), reason: 'Ada asked for your review', note: '3d', old: true }],
            },
          ]}
        />
      </div>
      <dl className="lists">
        <div>
          <dt>To review</dt>
          <dd>
            Your turn: you haven’t reviewed it, new commits reset your review, or its author pushed after you asked for
            changes.
          </dd>
        </div>
        <div>
          <dt>Reviewed</dt>
          <dd>You approved or commented, and it’s still open.</dd>
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

/** A review on its Changes tab: the file list, then a diff with a thread under its line. */
export function ReviewFigure() {
  const r = need('gh-482')
  const threads = threadsOf(r)
  const file = r.files[0]
  return (
    <figure
      className="review-figure"
      aria-label="A review in Mrow on its Changes tab. The bar holds the tabs Overview, Changes and Conversation, then Open in GitHub, Review with one pending comment, and Approve. Below it, 2 of 3 files viewed, then the diff of IssueList.tsx with a thread under a line: a question, its answer and your pending comment, with Reply and Resolve."
    >
      <div className="card" inert>
        <ReviewBar
          id="review-figure"
          tab="changes"
          files={r.files.length}
          threads={threads.length}
          unseen
          account={accounts.github}
          pending={pendingIn(threads)}
          reviews
          approves
          approved={false}
          panelOpen={false}
        />
        <div className="pane">
          <FileList files={r.files} viewed={new Set(r.files.slice(1).map((f) => f.path))} />
          {file && (
            <div className="page-file">
              <DiffView file={file.path} lines={file.lines} />
            </div>
          )}
        </div>
      </div>
    </figure>
  )
}

/** The Conversation tab with a comment held in a review, and the panel that finishes it. */
export function FinishFigure() {
  const r = need('gh-482')
  const threads = threadsOf(r).filter((t) => t.file)
  return (
    <figure
      className="review-figure finish-figure"
      aria-label="The Conversation tab of a review. A thread on IssueList.tsx ends with your comment marked Pending, then Reply and Resolve; a resolved thread on useSearchParam.ts is dimmed. The Finish your review panel hangs from the Review button: one pending comment posts with it, a summary, then Comment, Request Changes and Approve."
    >
      <div className="card" inert>
        <ReviewBar
          id="finish-figure"
          tab="conversation"
          files={r.files.length}
          threads={threadsOf(r).length}
          unseen
          account={accounts.github}
          pending={pendingIn(threads)}
          reviews
          approves
          approved={false}
          panelOpen
        />
        <ReviewPanel pending={pendingIn(threads)} mine={false} noun={accounts.github.noun} summary="Looks right to me. One small thing inline; it doesn’t block." />
        <div className="pane">
          {threads.map((t) => (
            <ThreadView key={t.id} thread={t} live flat />
          ))}
        </div>
      </div>
    </figure>
  )
}

export function AccountsFigure() {
  return (
    <div className="accounts-figure">
      <div className="accounts-picker">
        <AccountPicker account={accounts.github} forceOpen />
      </div>
      <ul className="accounts-pair">
        <li>
          <ProviderMark provider="github" />
          <span>
            Pull requests, through <code>gh</code>
          </span>
        </li>
        <li>
          <ProviderMark provider="gitlab" />
          <span>
            Merge requests, through <code>glab</code>
          </span>
        </li>
      </ul>
    </div>
  )
}

/** The Mine list: your move first, with a stack folded to what's on you. */
export function MineFigure() {
  const [selected, setSelected] = useState('gh-490')
  const [open, setOpen] = useState(false)
  const stack = github.filter((r) => r.stack === 'Saved views')
  const mine = github.filter((r) => r.list === 'mine')
  const count = (...s: Standing[]) => mine.filter((r) => r.standing && s.includes(r.standing)).length
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
            <StandingMark standing="ready" />
            {count('ready')}
          </span>
          <span className="mark-filter">
            <StandingMark standing="needs" />
            {count('needs')}
          </span>
          <span className="mark-filter">
            <StandingMark standing="waiting" />
            {count('waiting', 'quiet', 'held')}
          </span>
          <span className="mark-filter">
            <StandingMark standing="draft" />
            {count('draft')}
          </span>
        </div>
        <div className="queue">
          <section className="queue-group">
            <h4>Your move</h4>
            {row(need('gh-490'))}
            <StackGroup name="Saved views" parts={stack} shown={shown} open={open} onToggle={() => setOpen((v) => !v)} row={(r) => row(r, true)} />
            {row(need('gh-1215'))}
            {row(need('gh-1210'))}
          </section>
          <section className="queue-group">
            <h4>Waiting on reviewers</h4>
            {row(need('gh-1212'))}
          </section>
        </div>
      </div>
    </div>
  )
}

/** The top of one of your requests and the checklist at the bottom of its page. */
export function MergeFigure() {
  const r = need('gh-1210')
  return (
    <div className="merge-figure">
      <div className="merge-figure-part">
        <p className="figure-label">Where it stands</p>
        <NextStep
          review={r}
          account={accounts.github}
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
  const r = need('gh-500')
  const stack = github.filter((p) => p.stack === r.stack)
  const [confirm, setConfirm] = useState<'merge' | 'stack'>()
  const [walking, setWalking] = useState(false)
  return (
    <div className="stack-figure">
      <NextStep
        review={r}
        above={stack[1]}
        account={accounts.github}
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
      <InboxList events={inbox.github} find={find} />
    </div>
  )
}

/** A chat in which the agent has drafted a comment you haven't posted. */
export function DraftFigure() {
  return <AskChat review={need('gh-482')} account={accounts.github} />
}

/** What Mrow talks to. */
export function FlowFigure() {
  const paths: { via: string; to: string; when?: string }[] = [
    { via: 'gh', to: 'GitHub' },
    { via: 'glab', to: 'GitLab' },
    { via: 'agent', to: 'its model', when: 'when you ask' },
    { via: 'curl', to: 'GitHub releases', when: 'every few hours' },
    { via: 'curl', to: 'avatars and pictures', when: 'from where they’re hosted' },
  ]
  return (
    <div
      className="flow"
      role="img"
      aria-label="Mrow talks to gh, which talks to GitHub, and to glab, which talks to GitLab. When you ask, it talks to the agent you chose, which talks to its model. Release builds check GitHub releases for updates with curl every few hours, and avatars and pictures in a request are downloaded with curl from where they’re hosted."
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

/** Memory in use, measured 2026-11-19 (the figure's note says how). Whole megabytes. */
const MEMORY: { who: string; what: string; mb: number; upTo?: number; ours?: boolean }[] = [
  { who: 'Mrow', what: 'The whole app, a 40-file, 12,000-line diff open', mb: 33, upTo: 79, ours: true },
  { who: 'One GitHub tab', what: 'A pull request’s changes, in Chrome', mb: 157 },
  { who: 'Five GitHub tabs', what: 'Five pull requests, a tab each', mb: 444 },
  { who: 'Chrome, with those five', what: 'The tabs and the browser around them', mb: 903 },
]

/** Mrow's memory beside GitHub's in Chrome. A table, so it reads without the bars. */
export function MemoryFigure() {
  const max = Math.max(...MEMORY.map((m) => m.upTo ?? m.mb))
  return (
    <figure className="memory">
      <table>
        <caption className="sr-only">Memory in use, in megabytes</caption>
        <thead className="sr-only">
          <tr>
            <th scope="col">What</th>
            <th scope="col">Memory</th>
          </tr>
        </thead>
        <tbody>
          {MEMORY.map((m) => (
            <tr key={m.who} className={m.ours ? 'ours' : undefined}>
              <th scope="row">
                <span className="memory-who">{m.who}</span>
                <span className="memory-what">{m.what}</span>
              </th>
              <td>
                <span className="memory-bar" aria-hidden="true" style={{ width: `calc(${(m.upTo ?? m.mb) / max} * (100% - 7.5em))` }}>
                  <span className="memory-fill" style={{ width: `${(m.mb / (m.upTo ?? m.mb)) * 100}%` }} />
                </span>
                <span className="memory-value">
                  {m.mb} MB
                  {m.upTo && <span className="memory-upto">up to {m.upTo}</span>}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <figcaption className="memory-how">
        Memory in use (proportional set size), measured on 19 November 2026 on Linux, with an AMD Ryzen AI Max+ 395 and
        Radeon 8060S graphics. Mrow is a release build in a window, showing a made-up review; it reached 79 MB on runs
        where the graphics stack also loaded NVIDIA’s driver. Chrome is Chrome for Testing 158 with a fresh profile,
        signed out of GitHub, each tab scrolled until every file’s diff had loaded. The pull requests were
        facebook/react #38278, #38135 and #38172, which measured 151 to 158 MB, then #38209 and #38215 for five. A tab
        is its own process; the browser is every Chrome process.
      </figcaption>
    </figure>
  )
}

const COMPARE: { row: string; mrow: string; github: string; hosted: string }[] = [
  { row: 'What it is', mrow: 'An app on your computer', github: 'A tab in your browser', hosted: 'A website in your browser' },
  { row: 'Memory', mrow: 'About 33 MB for everything', github: 'About 157 MB per pull request', hosted: 'A browser tab per review' },
  {
    row: 'Your token',
    mrow: 'Stays in gh or glab; Mrow never reads it',
    github: 'Your browser’s GitHub session',
    hosted: 'Held on their servers, through an app you authorize',
  },
  { row: 'Your code', mrow: 'Your clone, on your disk', github: 'Stays on GitHub', hosted: 'Passes through their servers' },
  { row: 'AI', mrow: 'The agent you already use, kept to reading', github: 'Copilot, on GitHub’s plans', hosted: 'Theirs, on their bill' },
  { row: 'Price', mrow: 'Free, MIT-licensed', github: 'Comes with GitHub', hosted: 'Usually a monthly seat' },
]

/** Mrow beside GitHub in a browser and a hosted review service. */
export function CompareFigure() {
  return (
    <table className="compare">
      <caption className="sr-only">Mrow compared with GitHub in Chrome and a hosted review service</caption>
      <thead>
        <tr>
          <td />
          <th scope="col" className="ours">
            Mrow
          </th>
          <th scope="col">GitHub in Chrome</th>
          <th scope="col">A hosted review service</th>
        </tr>
      </thead>
      <tbody>
        {COMPARE.map((c) => (
          <tr key={c.row}>
            <th scope="row">{c.row}</th>
            <td className="ours" data-col="Mrow">
              {c.mrow}
            </td>
            <td data-col="GitHub in Chrome">{c.github}</td>
            <td data-col="A hosted review service">{c.hosted}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

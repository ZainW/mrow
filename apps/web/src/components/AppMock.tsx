import { useEffect, useId, useMemo, useRef, useState } from 'react'
import type { KeyboardEvent, ReactNode, RefObject } from 'react'
import { highlight } from '../lib/highlight'
import { useKeyLabel } from '../lib/keys'
import { accounts, inbox as inboxData, listLabels, reviews, shortRef } from './mockData'
import type { Account, Check, Comment, DiffFile, InboxEvent, ListId, Provider, Review, Standing } from './mockData'

const GROUP_ORDER = ['Today', 'Yesterday', 'This week', 'Older'] as const
const AGENTS = ['Cursor', 'Claude', 'opencode', 'pi'] as const

type Filter = 'all' | 'ready' | 'needs' | 'waiting' | 'draft'
type Confirm = { id: string; kind: 'merge' | 'stack' }
type Line = { kind: 'row'; review: Review; inStack?: boolean } | { kind: 'stack'; name: string; parts: Review[] }

/** Which filter mark a standing falls under: quiet and held requests count as waiting. */
function filterOf(s: Standing | undefined): Filter {
  if (s === 'ready' || s === 'merged') return 'ready'
  if (s === 'needs') return 'needs'
  if (s === 'draft') return 'draft'
  return 'waiting'
}
const onYou = (r: Review) => r.standing === 'ready' || r.standing === 'needs' || r.standing === 'merged'
const rank: Record<Standing, number> = { merged: 0, ready: 0, needs: 1, quiet: 2, held: 3, waiting: 3, draft: 4 }
const word: Record<Standing, string> = {
  ready: 'ready',
  merged: 'merged',
  needs: 'needs you',
  waiting: 'waiting',
  quiet: 'quiet',
  held: 'held',
  draft: 'draft',
}

/** The interactive window on the home page. Everything in it is made up. */
export function AppMock() {
  const [provider, setProvider] = useState<Provider>('gitlab')
  const [list, setList] = useState<ListId>('review')
  const [selectedId, setSelectedId] = useState('gl-214')
  const [tabs, setTabs] = useState<string[]>([])
  const [chatOpen, setChatOpen] = useState(false)
  const [inboxOpen, setInboxOpen] = useState(false)
  const [events, setEvents] = useState(inboxData)
  const [read, setRead] = useState<Set<string>>(() => new Set())
  const [approved, setApproved] = useState<Set<string>>(() => new Set())
  const [filter, setFilter] = useState<Filter>('all')
  const [unfolded, setUnfolded] = useState<Set<string>>(() => new Set())
  const [changes, setChanges] = useState<Record<string, Partial<Review>>>({})
  const [confirm, setConfirm] = useState<Confirm>()
  const [walking, setWalking] = useState<Set<string>>(() => new Set())
  const queueRef = useRef<HTMLDivElement>(null)
  const bellRef = useRef<HTMLButtonElement>(null)
  const keyLabel = useKeyLabel()

  const all = useMemo(() => reviews[provider].map((r) => ({ ...r, ...changes[r.id] })), [provider, changes])
  const byId = (id: string) => all.find((r) => r.id === id)
  const inList = all.filter((r) => r.list === list)
  const selected = byId(selectedId) ?? inList[0]
  const fresh = events[provider].filter((e) => e.fresh).length

  function select(r: Review) {
    setSelectedId(r.id)
    setConfirm(undefined)
    setRead((prev) => new Set(prev).add(r.id))
  }
  function keep(r: Review) {
    select(r)
    setTabs((prev) => (prev.includes(r.id) ? prev : [...prev, r.id]))
  }
  function open(r: Review) {
    setList(r.list)
    setFilter('all')
    if (r.stack) setUnfolded((prev) => new Set(prev).add(r.stack ?? ''))
    select(r)
  }
  function switchAccount(next: Provider) {
    setProvider(next)
    setInboxOpen(false)
    const first = reviews[next].find((r) => r.list === list)
    if (first) select(first)
  }
  function switchList(next: ListId) {
    setList(next)
    setFilter('all')
    const first = all.find((r) => r.list === next)
    if (first) select(first)
  }
  function change(id: string, patch: Partial<Review>) {
    setChanges((prev) => ({ ...prev, [id]: { ...prev[id], ...patch } }))
  }

  // The rows in the order they're drawn, for j and k.
  const lines = useMemo(() => {
    if (list !== 'mine') return [] as Line[]
    const mine = all.filter((r) => r.list === 'mine')
    if (filter !== 'all') return mine.filter((r) => filterOf(r.standing) === filter).map((review): Line => ({ kind: 'row', review }))
    const stacks = new Map<string, Review[]>()
    for (const r of mine) if (r.stack) stacks.set(r.stack, [...(stacks.get(r.stack) ?? []), r])
    const placed: Line[] = []
    const seen = new Set<string>()
    const sorted = [...mine].sort((a, b) => rank[a.standing ?? 'waiting'] - rank[b.standing ?? 'waiting'])
    for (const r of sorted) {
      if (!r.stack) placed.push({ kind: 'row', review: r })
      else if (!seen.has(r.stack)) {
        seen.add(r.stack)
        placed.push({ kind: 'stack', name: r.stack, parts: stacks.get(r.stack) ?? [] })
      }
    }
    return placed
  }, [all, list, filter])

  function visibleParts(name: string, parts: Review[]) {
    if (unfolded.has(name)) return parts
    return parts.filter((p) => onYou(p) || p.id === selected?.id)
  }

  const ordered: Review[] =
    list === 'mine'
      ? lines.flatMap((l) => (l.kind === 'row' ? [l.review] : visibleParts(l.name, l.parts)))
      : GROUP_ORDER.flatMap((g) => inList.filter((r) => r.group === g))

  function onQueueKey(e: KeyboardEvent<HTMLDivElement>) {
    if (e.metaKey || e.ctrlKey || e.altKey) return
    if (list === 'mine' && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
      e.preventDefault()
      setUnfolded(e.key === 'ArrowRight' ? new Set(lines.flatMap((l) => (l.kind === 'stack' ? [l.name] : []))) : new Set())
      return
    }
    const step = e.key === 'j' || e.key === 'ArrowDown' ? 1 : e.key === 'k' || e.key === 'ArrowUp' ? -1 : 0
    if (!step || !selected) return
    e.preventDefault()
    const next = ordered[ordered.findIndex((r) => r.id === selected.id) + step]
    if (next) {
      select(next)
      queueRef.current?.querySelector<HTMLButtonElement>(`[data-id="${next.id}"]`)?.focus()
    }
  }

  const rowProps = (r: Review, inStack?: boolean) => ({
    review: r,
    inStack,
    active: r.id === selected?.id,
    unread: !!r.unread && !read.has(r.id),
    onSelect: () => select(r),
    onKeep: () => keep(r),
  })

  const counts: Record<ListId, number | undefined> = {
    review: all.filter((r) => r.list === 'review').length,
    reviewed: all.filter((r) => r.list === 'reviewed').length,
    mine: all.filter((r) => r.list === 'mine' && onYou(r) && r.standing !== 'merged').length,
    done: undefined,
  }

  const openTabs = tabs.map(byId).filter((r): r is Review => !!r)
  const preview = selected && !tabs.includes(selected.id) ? selected : undefined

  return (
    <figure className="window" aria-label="An interactive illustration of the Mrow window, filled with made-up requests">
      <div className="window-bar">
        <span className="lights" aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
        <div className="tabstrip" role="tablist" aria-label="Open reviews">
          {openTabs.map((r) => (
            <span key={r.id} className={r.id === selected?.id ? 'tab on' : 'tab'}>
              <button role="tab" aria-selected={r.id === selected?.id} onClick={() => open(r)}>
                <b>{r.number}</b> {r.title}
              </button>
              <button
                className="tab-close"
                aria-label={`Close ${r.number}`}
                onClick={() => setTabs((prev) => prev.filter((id) => id !== r.id))}
              >
                <CloseIcon />
              </button>
            </span>
          ))}
          {preview && (
            <span className="tab on preview" title="A preview. Double-click a row or this tab to keep it.">
              <button role="tab" aria-selected="true" onDoubleClick={() => keep(preview)} onClick={() => {}}>
                <b>{preview.number}</b> {preview.title}
              </button>
            </span>
          )}
        </div>
        <button
          className={chatOpen ? 'bar-btn on' : 'bar-btn'}
          aria-pressed={chatOpen}
          onClick={() => setChatOpen((v) => !v)}
          title="Ask an agent (⌘L)"
        >
          <ChatIcon />
          <span>Ask</span>
        </button>
      </div>

      <div className={chatOpen ? 'window-body with-chat' : 'window-body'}>
        <aside className="sidebar">
          <div className="sidebar-head">
            <AccountPicker account={accounts[provider]} onChange={switchAccount} />
            <button
              ref={bellRef}
              className={inboxOpen ? 'bell on' : 'bell'}
              aria-expanded={inboxOpen}
              aria-label={fresh ? `Inbox, ${fresh} new` : 'Inbox'}
              title="Inbox (⇧⌘I)"
              onClick={() => setInboxOpen((v) => !v)}
            >
              <BellIcon />
              {fresh > 0 && <span className="bell-count">{fresh}</span>}
            </button>
          </div>

          <div className="list-bar" role="tablist" aria-label="Lists">
            {(Object.keys(listLabels) as ListId[]).map((id) => (
              <button key={id} role="tab" aria-selected={list === id} className="list-bar-item" onClick={() => switchList(id)}>
                {listLabels[id]}
                {counts[id] !== undefined && <span className="count">{counts[id]}</span>}
              </button>
            ))}
          </div>

          <div className="filter" aria-hidden="true">
            <SearchIcon />
            <span>Filter {accounts[provider].noun}s</span>
            <kbd>{keyLabel('⌘K')}</kbd>
          </div>

          {list === 'mine' && (
            <div className="marks" role="group" aria-label="Show">
              {(['all', 'ready', 'needs', 'waiting', 'draft'] as Filter[]).map((f) => {
                const n = all.filter((r) => r.list === 'mine' && filterOf(r.standing) === f).length
                const label = { all: 'All', ready: 'Ready to merge', needs: 'Needs you', waiting: 'Waiting', draft: 'Draft' }[f]
                return (
                  <button
                    key={f}
                    className="mark-filter"
                    aria-pressed={filter === f}
                    aria-label={f === 'all' ? 'All' : `${label}, ${n}`}
                    title={label}
                    onClick={() => setFilter(f)}
                  >
                    {f === 'all' ? 'All' : <><StandingMark standing={f === 'needs' ? 'needs' : f} />{n}</>}
                  </button>
                )
              })}
            </div>
          )}

          <div className="queue" ref={queueRef} onKeyDown={onQueueKey}>
            {list !== 'mine' &&
              GROUP_ORDER.map((g) => {
                const rows = inList.filter((r) => r.group === g)
                if (!rows.length) return null
                return (
                  <section key={g} className="queue-group">
                    <h4>{g}</h4>
                    {rows.map((r) => (
                      <QueueRow key={r.id} {...rowProps(r)} />
                    ))}
                  </section>
                )
              })}
            {list === 'mine' && filter !== 'all' && (
              <section className="queue-group">
                {lines.map((l) => l.kind === 'row' && <QueueRow key={l.review.id} {...rowProps(l.review)} />)}
              </section>
            )}
            {list === 'mine' &&
              filter === 'all' &&
              (
                [
                  ['Your move', (s: Standing) => s === 'ready' || s === 'needs' || s === 'merged'],
                  ['Waiting on reviewers', (s: Standing) => s === 'waiting' || s === 'quiet' || s === 'held'],
                  ['Drafts', (s: Standing) => s === 'draft'],
                ] as const
              ).map(([title, fits]) => {
                const here = lines.filter((l) => {
                  const lead = l.kind === 'row' ? l.review : [...l.parts].sort((a, b) => rank[a.standing ?? 'waiting'] - rank[b.standing ?? 'waiting'])[0]
                  return lead && fits(lead.standing ?? 'waiting')
                })
                if (!here.length) return null
                return (
                  <section key={title} className="queue-group">
                    <h4>{title}</h4>
                    {here.map((l) =>
                      l.kind === 'row' ? (
                        <QueueRow key={l.review.id} {...rowProps(l.review)} />
                      ) : (
                        <StackGroup
                          key={l.name}
                          name={l.name}
                          parts={l.parts}
                          shown={visibleParts(l.name, l.parts)}
                          open={unfolded.has(l.name)}
                          onToggle={() =>
                            setUnfolded((prev) => {
                              const next = new Set(prev)
                              if (next.has(l.name)) next.delete(l.name)
                              else next.add(l.name)
                              return next
                            })
                          }
                          row={(r) => <QueueRow key={r.id} {...rowProps(r, true)} />}
                        />
                      ),
                    )}
                  </section>
                )
              })}
          </div>
        </aside>

        {inboxOpen && (
          <Inbox
            events={events[provider]}
            find={byId}
            onOpen={(e) => {
              const r = byId(e.review)
              setEvents((prev) => ({ ...prev, [provider]: prev[provider].map((x) => (x.id === e.id ? { ...x, fresh: false } : x)) }))
              setInboxOpen(false)
              if (r) open(r)
            }}
            onDoneAll={() => setEvents((prev) => ({ ...prev, [provider]: [] }))}
            onClose={(refocus) => {
              setInboxOpen(false)
              if (refocus) bellRef.current?.focus()
            }}
            bell={bellRef}
          />
        )}

        {selected && (
          <Detail
            key={selected.id}
            review={selected}
            all={all}
            account={accounts[provider]}
            approved={approved.has(selected.id)}
            onApprove={() =>
              setApproved((prev) => {
                const next = new Set(prev)
                if (next.has(selected.id)) next.delete(selected.id)
                else next.add(selected.id)
                return next
              })
            }
            confirm={confirm?.id === selected.id ? confirm.kind : undefined}
            walking={!!selected.stack && walking.has(selected.stack)}
            onAsk={(kind) => setConfirm({ id: selected.id, kind })}
            onCancel={() => setConfirm(undefined)}
            onMerge={(kind) => {
              setConfirm(undefined)
              change(selected.id, { standing: 'merged', step: 'Merged · it moves to Done on the next refresh' })
              if (kind === 'stack' && selected.stack) setWalking((prev) => new Set(prev).add(selected.stack ?? ''))
            }}
            onStopWalk={() =>
              setWalking((prev) => {
                const next = new Set(prev)
                next.delete(selected.stack ?? '')
                return next
              })
            }
            onAutoMerge={(on) => {
              const step = reviews[provider].find((r) => r.id === selected.id)?.step ?? ''
              change(selected.id, { step: on ? `Auto-merge on · ${lower(step)}` : step })
            }}
            onReady={() => change(selected.id, { standing: 'waiting', step: 'No reviewers yet', draft: false, why: 'Ask someone to review it.' })}
            onGo={(r) => open(r)}
          />
        )}

        {chatOpen && selected && (
          <div className="window-chat">
            <AskChat key={selected.id} review={selected} account={accounts[provider]} compact onClose={() => setChatOpen(false)} />
          </div>
        )}
      </div>
    </figure>
  )
}

const lower = (s: string) => s.charAt(0).toLowerCase() + s.slice(1)

export function QueueRow({
  review,
  inStack,
  active,
  unread,
  onSelect,
  onKeep,
}: {
  review: Review
  inStack?: boolean
  active: boolean
  unread: boolean
  onSelect: () => void
  onKeep: () => void
}) {
  const mine = review.list === 'mine'
  const line = mine ? review.step : review.reason
  const tone = mine ? `step-${review.standing}` : review.again ? 'step-again' : ''
  return (
    <button
      className={`row${active ? ' active' : ''}${inStack ? ' in-stack' : ''}`}
      aria-current={active ? 'true' : undefined}
      data-id={review.id}
      onClick={onSelect}
      onDoubleClick={onKeep}
    >
      <span className={unread ? 'dot on' : 'dot'}>{unread && <span className="sr-only">Changed since you last looked. </span>}</span>
      <span className="row-main">
        <span className="row-top">
          <span className="row-title">{review.title}</span>
          <span className={review.stale ? 'row-age stale' : 'row-age'}>{review.age}</span>
        </span>
        {line && <span className={`row-line ${tone}`}>{line}</span>}
        <span className="row-sub">
          <span className="row-ref">{shortRef(review)}</span>
          {!mine && <span className="row-author">{review.author}</span>}
          {review.draft && !mine && <span className="row-draft">Draft</span>}
          <span className="spacer" />
          {mine && review.standing ? <StandingMark standing={review.standing} /> : <PipelineDot pipeline={review.pipeline} />}
        </span>
      </span>
    </button>
  )
}

export function StackGroup({
  name,
  parts,
  shown,
  open,
  onToggle,
  row,
}: {
  name: string
  parts: Review[]
  shown: Review[]
  open: boolean
  onToggle: () => void
  row: (r: Review) => ReactNode
}) {
  const hidden = parts.filter((p) => !shown.includes(p))
  const tally = Object.entries(
    hidden.reduce<Record<string, number>>((acc, p) => {
      const w = word[p.standing ?? 'waiting']
      acc[w] = (acc[w] ?? 0) + 1
      return acc
    }, {}),
  )
    .map(([w, n]) => `${n} ${w}`)
    .join(', ')
  return (
    <div className="stack">
      <button className="stack-head" aria-expanded={open} onClick={onToggle}>
        <Chevron className={open ? 'chev open' : 'chev'} />
        <span className="stack-name">{name}</span>
        <span className="stack-of">stack of {parts.length}</span>
      </button>
      <div className="stack-parts">
        {shown.map(row)}
        {!open && hidden.length > 0 && (
          <button className="stack-more" onClick={onToggle}>
            {hidden.length} more · {tally}
          </button>
        )}
      </div>
    </div>
  )
}

function Inbox({
  events,
  find,
  onOpen,
  onDoneAll,
  onClose,
  bell,
}: {
  events: InboxEvent[]
  find: (id: string) => Review | undefined
  onOpen: (e: InboxEvent) => void
  onDoneAll: () => void
  /** Closes it, putting focus back on the bell unless the click went somewhere else. */
  onClose: (refocus: boolean) => void
  bell: RefObject<HTMLButtonElement | null>
}) {
  const ref = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  useEffect(() => {
    close.current = onClose
  })
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>('button')?.focus()
    const away = (e: PointerEvent) => {
      const t = e.target as Node
      if (!ref.current?.contains(t) && !bell.current?.contains(t)) close.current(false)
    }
    document.addEventListener('pointerdown', away)
    return () => document.removeEventListener('pointerdown', away)
  }, [bell])
  return (
    <div
      className="inbox-pop"
      role="dialog"
      aria-label="Inbox"
      ref={ref}
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose(true)
      }}
    >
      <div className="inbox-head">
        <span>Inbox</span>
        {events.length > 0 && (
          <button className="quiet-btn" onClick={onDoneAll}>
            Mark All Done
          </button>
        )}
        <button className="icon-btn" aria-label="Close the inbox" onClick={() => onClose(true)}>
          <CloseIcon />
        </button>
      </div>
      {events.length === 0 ? (
        <p className="inbox-empty">Nothing new. What changes after the next refresh shows up here.</p>
      ) : (
        <InboxList events={events} find={find} onOpen={onOpen} />
      )}
    </div>
  )
}

export function InboxList({
  events,
  find,
  onOpen,
}: {
  events: InboxEvent[]
  find: (id: string) => Review | undefined
  onOpen?: (e: InboxEvent) => void
}) {
  return (
    <div className="inbox-list">
      {(
        [
          ['New', events.filter((e) => e.fresh)],
          ['Earlier', events.filter((e) => !e.fresh)],
        ] as const
      ).map(
        ([title, rows]) =>
          rows.length > 0 && (
            <section key={title}>
              <h4>{title}</h4>
              {rows.map((e) => {
                const r = find(e.review)
                const body = (
                  <>
                    <span className={e.fresh ? 'dot on' : 'dot'} />
                    <span className="inbox-main">
                      <span className="inbox-top">
                        <span className={`inbox-what tone-${e.tone}`}>{e.what}</span>
                        <span className="row-age">{e.when}</span>
                      </span>
                      <span className="inbox-title">{r?.title}</span>
                      <span className="row-ref">{r && shortRef(r)}</span>
                    </span>
                  </>
                )
                return onOpen ? (
                  <button key={e.id} className="inbox-row" onClick={() => onOpen(e)}>
                    {body}
                  </button>
                ) : (
                  <div key={e.id} className="inbox-row">
                    {body}
                  </div>
                )
              })}
            </section>
          ),
      )}
    </div>
  )
}

export type ReviewTab = 'overview' | 'changes' | 'conversation'
export type Verdict = 'Comment' | 'Request Changes' | 'Approve'
/** A thread on a line of a file, or a comment on the whole request. */
export type Thread = { id: string; file?: string; fileIndex?: number; line?: number; comments: Comment[]; resolvable: boolean; resolved: boolean }

const TABS: [ReviewTab, string][] = [
  ['overview', 'Overview'],
  ['changes', 'Changes'],
  ['conversation', 'Conversation'],
]
const VERDICTS: Verdict[] = ['Comment', 'Request Changes', 'Approve']

/** A review's threads, those on lines first, as the Conversation tab lists them. */
export function threadsOf(r: Review): Thread[] {
  const out: Thread[] = []
  r.files.forEach((f, fi) =>
    f.lines.forEach((l, li) => {
      if (l.thread) out.push({ id: `${fi}-${li}`, file: f.path, fileIndex: fi, line: l.new ?? l.old, comments: l.thread, resolvable: true, resolved: !!l.resolved })
    }),
  )
  r.conversation.forEach((c, i) => out.push({ id: `c${i}`, comments: [c], resolvable: false, resolved: false }))
  return out
}
export const pendingIn = (threads: Thread[]) => threads.reduce((n, t) => n + t.comments.filter((c) => c.pending).length, 0)
const fileName = (path: string) => path.split('/').pop() ?? path

function Detail({
  review,
  all,
  account,
  approved,
  onApprove,
  confirm,
  walking,
  onAsk,
  onCancel,
  onMerge,
  onStopWalk,
  onAutoMerge,
  onReady,
  onGo,
}: {
  review: Review
  all: Review[]
  account: Account
  approved: boolean
  onApprove: () => void
  confirm?: Confirm['kind']
  walking: boolean
  onAsk: (kind: Confirm['kind']) => void
  onCancel: () => void
  onMerge: (kind: Confirm['kind']) => void
  onStopWalk: () => void
  onAutoMerge: (on: boolean) => void
  onReady: () => void
  onGo: (r: Review) => void
}) {
  const id = useId()
  const [tab, setTab] = useState<ReviewTab>('overview')
  const [viewed, setViewed] = useState<Set<string>>(() => new Set())
  const [threads, setThreads] = useState(() => threadsOf(review))
  const [panel, setPanel] = useState(false)
  const [summary, setSummary] = useState('')
  const [askedChanges, setAskedChanges] = useState(false)
  // The file J and K are on, and a jump to make once the Changes tab is showing.
  const [file, setFile] = useState<number>()
  const [jump, setJump] = useState<{ file: number }>()
  const changesRef = useRef<HTMLDivElement>(null)
  const reviewRef = useRef<HTMLButtonElement>(null)

  const mine = review.list === 'mine'
  const open = review.list !== 'done' && review.standing !== 'merged'
  const pending = pendingIn(threads)
  const reviews = open && (!mine || pending > 0)
  const approves = open && !mine

  useEffect(() => {
    const pane = changesRef.current
    const el = jump && pane?.querySelector<HTMLElement>(`[data-file="${jump.file}"]`)
    if (pane && el) pane.scrollTop = el.offsetTop - 12
  }, [jump])

  function goFile(i: number) {
    setTab('changes')
    setFile(i)
    setJump({ file: i })
  }
  function closePanel(refocus: boolean) {
    setPanel(false)
    if (refocus) reviewRef.current?.focus()
  }
  function approve() {
    if (pending > 0) setPanel(true)
    else onApprove()
  }
  function edit(threadId: string, fn: (t: Thread) => Thread | undefined) {
    setThreads((prev) => prev.flatMap((t) => (t.id === threadId ? (fn(t) ?? []) : [t])))
  }
  function add(body: string, held: boolean) {
    setThreads((prev) => [...prev, { id: `n${prev.length}`, comments: [{ author: 'You', when: held ? '' : 'just now', body, pending: held }], resolvable: false, resolved: false }])
  }
  function submit(verdict: Verdict) {
    const said = summary.trim()
    setThreads((prev) => {
      const sent = prev.map((t) => ({ ...t, comments: t.comments.map((c) => (c.pending ? { ...c, pending: false, when: 'just now' } : c)) }))
      return said ? [...sent, { id: `n${sent.length}`, comments: [{ author: 'You', when: 'just now', body: said }], resolvable: false, resolved: false }] : sent
    })
    if (verdict === 'Approve' && !approved) onApprove()
    if (verdict === 'Request Changes') setAskedChanges(true)
    setSummary('')
    closePanel(true)
  }

  function onKey(e: KeyboardEvent<HTMLDivElement>) {
    const digit = ['Digit1', 'Digit2', 'Digit3'].indexOf(e.code)
    if (e.altKey && (e.metaKey || e.ctrlKey) && digit >= 0) {
      e.preventDefault()
      setTab(TABS[digit]?.[0] ?? 'overview')
      return
    }
    if (e.key === 'Escape' && panel) return closePanel(true)
    if (tab !== 'changes' || e.metaKey || e.ctrlKey || e.altKey || (e.target as HTMLElement).closest('button, input, textarea')) return
    const step = e.key === 'j' ? 1 : e.key === 'k' ? -1 : 0
    if (!step) return
    e.preventDefault()
    const next = Math.min(Math.max((file ?? -1) + step, 0), review.files.length - 1)
    setFile(next)
    setJump({ file: next })
  }

  const stack = review.stack ? all.filter((r) => r.stack === review.stack) : []
  const below = stack[stack.findIndex((r) => r.id === review.id) - 1]
  const above = stack[stack.findIndex((r) => r.id === review.id) + 1]
  const adds = review.files.reduce((n, f) => n + f.lines.filter((l) => l.kind === 'add').length, 0)
  const dels = review.files.reduce((n, f) => n + f.lines.filter((l) => l.kind === 'del').length, 0)
  const unviewed = review.files.findIndex((f) => !viewed.has(f.path))
  const openThreads = threads.filter((t) => t.resolvable && !t.resolved).length
  const toggleViewed = (path: string) =>
    setViewed((prev) => {
      const next = new Set(prev)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  const threadProps = (t: Thread) => ({
    thread: t,
    live: open,
    onResolve: () => edit(t.id, (x) => ({ ...x, resolved: !x.resolved })),
    onDelete: (j: number) => edit(t.id, (x) => (x.comments.length > 1 ? { ...x, comments: x.comments.filter((_, k) => k !== j) } : undefined)),
    onSave: (j: number, body: string) => edit(t.id, (x) => ({ ...x, comments: x.comments.map((c, k) => (k === j ? { ...c, body } : c)) })),
  })
  const pane = (t: ReviewTab) => ({ id: `${id}-${t}`, role: 'tabpanel', 'aria-labelledby': `${id}-${t}-tab`, hidden: tab !== t, tabIndex: 0, className: 'pane' })

  return (
    <div className="detail" onKeyDown={onKey}>
      <article className="card" aria-label={`${review.number} ${review.title}`}>
        <ReviewBar
          id={id}
          tab={tab}
          onTab={setTab}
          files={review.files.length}
          threads={threads.length}
          unseen={threads.some((t) => t.comments.some((c) => c.unseen))}
          account={account}
          pending={pending}
          reviews={reviews}
          approves={approves}
          approved={approved}
          panelOpen={panel}
          onReview={() => setPanel((v) => !v)}
          onApprove={approve}
          reviewRef={reviewRef}
        />
        {panel && (
          <ReviewPanel
            pending={pending}
            mine={mine}
            noun={account.noun}
            summary={summary}
            onSummary={setSummary}
            onSubmit={submit}
            onClose={closePanel}
            anchor={reviewRef}
          />
        )}

        <div {...pane('overview')}>
          <p className="page-ref">
            {review.project} <b>{review.number}</b>
          </p>
          <h3 className="page-title">{review.title}</h3>
          <p className="byline">
            <Avatar name={review.author} />
            <span>
              <strong>{review.author}</strong>{' '}
              {mine
                ? 'opened this'
                : review.list === 'done'
                  ? `${review.outcome === 'Closed' ? 'closed' : 'merged'} ${review.age} ago`
                  : review.list === 'reviewed'
                    ? `opened this · ${lower(review.reason ?? '')}`
                    : `${review.again ? 'asked again' : 'asked for your review'} ${review.age} ago`}
            </span>
          </p>
          <p className="chips">
            <span className="chip mono">
              {review.source} → {review.target}
            </span>
            <span className="chip mono">
              {adds > 0 && <span className="plus">+{adds}</span>} {dels > 0 && <span className="minus">−{dels}</span>} · {review.files.length}{' '}
              {review.files.length === 1 ? 'file' : 'files'}
            </span>
            <span className="chip">
              <PipelineDot pipeline={review.pipeline} /> Pipeline {review.pipeline}
            </span>
          </p>

          {mine && review.standing && (
            <NextStep
              review={review}
              above={above}
              below={below}
              confirm={confirm}
              walking={walking}
              account={account}
              onAsk={onAsk}
              onCancel={onCancel}
              onMerge={onMerge}
              onStopWalk={onStopWalk}
              onAutoMerge={onAutoMerge}
              onReady={onReady}
              onGo={onGo}
              onThreads={() => setTab('conversation')}
            />
          )}

          {stack.length > 0 && (
            <div className="stack-card">
              <p className="stack-card-title">Stack · {review.stack}</p>
              <div className="stack-card-list">
                <span className="stack-card-base">{stack[0]?.target}</span>
                {stack.map((r) => (
                  <button key={r.id} className={r.id === review.id ? 'stack-line here' : 'stack-line'} onClick={() => r.id !== review.id && onGo(r)}>
                    {r.standing && <StandingMark standing={r.standing} />}
                    <span className="mono quiet">{r.number}</span>
                    <span className="stack-line-title">{r.title}</span>
                    <span className="quiet">{r.id === review.id ? 'This one' : cap(word[r.standing ?? 'waiting'])}</span>
                  </button>
                ))}
              </div>
            </div>
          )}

          {review.reviewers.length > 0 && (
            <p className="reviewers">
              <span className="quiet">Reviewers</span>
              {review.reviewers.map((p) => (
                <span key={p.name} className="reviewer">
                  <Avatar name={p.name} />
                  {p.name}
                  {p.state === 'approved' || (p.name === 'You' && approved) ? (
                    <StandingMark standing="ready" />
                  ) : p.state === 'changes' || (p.name === 'You' && askedChanges) ? (
                    <StandingMark standing="needs" />
                  ) : (
                    <span className="ring" aria-label="hasn’t reviewed yet" />
                  )}
                </span>
              ))}
            </p>
          )}

          <div className="page-desc">
            {review.description.map((p) => (
              <p key={p}>{p}</p>
            ))}
          </div>

          {!open ? (
            <section className="page-section">
              <h4>How it ended</h4>
              <p className="page-end-text">
                This {account.noun} {review.outcome === 'Closed' ? 'was closed without merging' : 'was merged'}.
              </p>
            </section>
          ) : mine ? (
            review.checks && (
              <section className="page-section">
                <h4>Before it merges</h4>
                <Checklist
                  checks={review.checks}
                  onGo={(c) => {
                    if (c.label === 'Threads') setTab('conversation')
                    if (c.label === 'Stack' && below) onGo(below)
                  }}
                />
              </section>
            )
          ) : (
            <div className="standing">
              <p>{sumUp(review.files.length - viewed.size, openThreads)}</p>
              <button className="act" onClick={() => (unviewed >= 0 ? goFile(unviewed) : setTab('changes'))}>
                {unviewed >= 0 ? `Start with ${fileName(review.files[unviewed]?.path ?? '')}` : 'See the Changes'}
              </button>
            </div>
          )}
        </div>

        <div {...pane('changes')} ref={changesRef}>
          <FileList files={review.files} viewed={viewed} onGo={goFile} />
          {review.files.map((f, i) => (
            <div key={f.path} data-file={i} className="page-file">
              <DiffView
                file={f.path}
                lines={f.lines}
                viewed={viewed.has(f.path)}
                onToggleViewed={() => toggleViewed(f.path)}
                threadAt={(li) => {
                  const t = threads.find((x) => x.id === `${i}-${li}`)
                  return t && <ThreadView {...threadProps(t)} />
                }}
              />
            </div>
          ))}
        </div>

        <div {...pane('conversation')}>
          {threads.length === 0 && <p className="page-sub">No comments yet.</p>}
          {threads.map((t) => (
            <ThreadView key={t.id} {...threadProps(t)} flat onWhere={t.fileIndex !== undefined ? () => goFile(t.fileIndex ?? 0) : undefined} />
          ))}
          {open && (
            <Composer
              holds={account.provider === 'gitlab'}
              reviewing={account.provider === 'gitlab' && pending > 0}
              onPost={(body) => add(body, false)}
              onHold={(body) => add(body, true)}
            />
          )}
        </div>
      </article>
    </div>
  )
}

const cap = (s: string) => s.charAt(0).toUpperCase() + s.slice(1)

/** Where the review stands, in one sentence, as the end of the Overview says it. */
function sumUp(files: number, threads: number) {
  const f = files === 0 ? 'You’ve viewed every file' : files === 1 ? '1 file still to view' : `${files} files still to view`
  const t = threads === 0 ? 'no threads are open' : threads === 1 ? '1 thread is open' : `${threads} threads are open`
  return `${f}, and ${t}.`
}

/** The bar over a review: its three tabs, then Open in the forge, Review and Approve. */
export function ReviewBar({
  id,
  tab,
  onTab,
  files,
  threads,
  unseen,
  account,
  pending,
  reviews,
  approves,
  approved,
  panelOpen,
  onReview,
  onApprove,
  reviewRef,
}: {
  id: string
  tab: ReviewTab
  onTab?: (t: ReviewTab) => void
  files: number
  threads: number
  unseen: boolean
  account: Account
  pending: number
  reviews: boolean
  approves: boolean
  approved: boolean
  panelOpen: boolean
  onReview?: () => void
  onApprove?: () => void
  reviewRef?: RefObject<HTMLButtonElement | null>
}) {
  const keyLabel = useKeyLabel()
  const counts: Record<ReviewTab, number | undefined> = { overview: undefined, changes: files, conversation: threads }
  const reviewLabel = pending === 0 ? 'Review' : pending === 1 ? 'Review, 1 pending comment' : `Review, ${pending} pending comments`
  return (
    <div className="review-bar">
      <div className="review-tabs" role="tablist" aria-label="Parts of the review">
        {TABS.map(([t, label], i) => (
          <button
            key={t}
            id={`${id}-${t}-tab`}
            role="tab"
            aria-selected={tab === t}
            aria-controls={`${id}-${t}`}
            tabIndex={tab === t ? 0 : -1}
            className="review-tab"
            title={`${label} (${keyLabel(`⌥⌘${i + 1}`)})`}
            onClick={() => onTab?.(t)}
          >
            {label}
            {counts[t] !== undefined && <span className="tab-count">{counts[t]}</span>}
            {t === 'conversation' && unseen && <span className="tab-dot" title="A reply you haven’t seen" />}
          </button>
        ))}
      </div>
      <div className="bar-actions">
        <span className="act static forge-btn" aria-label={`Open in ${account.name}`} title={`Open in ${account.name} (${keyLabel('⌘O')})`}>
          <OutIcon /> <span className="act-label">Open in {account.name}</span>
        </span>
        {reviews && (
          <button
            ref={reviewRef}
            className={panelOpen ? 'act review-btn on' : 'act review-btn'}
            aria-label={reviewLabel}
            aria-expanded={panelOpen}
            aria-haspopup="dialog"
            title={`Review (${keyLabel('⇧⌘R')})`}
            onClick={onReview}
          >
            <ChatIcon /> <span className="act-label">Review</span>
            {pending > 0 && <span className="pending-count">{pending}</span>}
          </button>
        )}
        {approves && (
          <button
            className={approved ? 'act approve-btn' : 'act primary approve-btn'}
            aria-label={approved ? 'Revoke Approval' : 'Approve'}
            title={approved ? 'Revoke Approval' : `Approve (${keyLabel('⇧⌘A')})`}
            onClick={onApprove}
          >
            {approved ? <UndoIcon /> : <CheckIcon />}{' '}
            <span className="act-label">{approved ? <>Revoke<span className="act-label-more"> Approval</span></> : 'Approve'}</span>
          </button>
        )}
      </div>
    </div>
  )
}

/** "Finish your review": a summary, then Comment, Request Changes or Approve. */
export function ReviewPanel({
  pending,
  mine,
  noun,
  summary,
  onSummary,
  onSubmit,
  onClose,
  anchor,
}: {
  pending: number
  mine: boolean
  noun: string
  summary: string
  onSummary?: (s: string) => void
  onSubmit?: (v: Verdict) => void
  onClose?: (refocus: boolean) => void
  anchor?: RefObject<HTMLButtonElement | null>
}) {
  const ref = useRef<HTMLDivElement>(null)
  const close = useRef(onClose)
  useEffect(() => {
    close.current = onClose
  })
  useEffect(() => {
    if (!onClose) return
    ref.current?.querySelector('textarea')?.focus()
    const away = (e: PointerEvent) => {
      const t = e.target as Node
      if (!ref.current?.contains(t) && !anchor?.current?.contains(t)) close.current?.(false)
    }
    document.addEventListener('pointerdown', away)
    return () => document.removeEventListener('pointerdown', away)
    // Only on opening: the panel focuses its summary once.
  }, [])
  const said = pending > 0 || summary.trim() !== ''
  const held = pending === 0 ? 'Nothing held yet. Your summary and verdict post on their own.' : pending === 1 ? '1 pending comment posts with it.' : `${pending} pending comments post with it.`
  return (
    <div className="review-panel" role="dialog" aria-label="Finish your review" ref={ref}>
      <div className="review-panel-body">
        <h4>Finish your review</h4>
        <p className="review-panel-note">{held}</p>
        <textarea
          aria-label="Summary"
          placeholder="Sum up your review, if you like. Markdown works."
          rows={3}
          value={summary}
          readOnly={!onSummary}
          onChange={(e) => onSummary?.(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && said) {
              e.preventDefault()
              onSubmit?.('Comment')
            }
          }}
        />
        {mine && <p className="review-panel-note quiet">This {noun} is yours, so you can only comment on it.</p>}
      </div>
      <div className="review-panel-foot">
        {VERDICTS.map((v) => (
          <button
            key={v}
            className={v === 'Approve' ? 'act primary' : 'act'}
            disabled={v === 'Approve' ? mine : v === 'Request Changes' ? !said || mine : !said}
            onClick={() => onSubmit?.(v)}
          >
            {v}
          </button>
        ))}
      </div>
    </div>
  )
}

/** The top of the Changes tab: every file, how many are viewed, and a way to each. */
export function FileList({ files, viewed, onGo }: { files: DiffFile[]; viewed: Set<string>; onGo?: (i: number) => void }) {
  return (
    <div className="files">
      <p className="files-head">
        <span>
          {files.filter((f) => viewed.has(f.path)).length} of {files.length} viewed
        </span>
        <span className="files-progress" aria-hidden="true">
          <i style={{ width: `${(100 * files.filter((f) => viewed.has(f.path)).length) / Math.max(files.length, 1)}%` }} />
        </span>
      </p>
      {files.map((f, i) => {
        const adds = f.lines.filter((l) => l.kind === 'add').length
        const dels = f.lines.filter((l) => l.kind === 'del').length
        return (
          <button key={f.path} className="file-row" onClick={() => onGo?.(i)}>
            <span className="file-path">{f.path}</span>
            <span className="diff-stat">
              {adds > 0 && <span className="plus">+{adds}</span>} {dels > 0 && <span className="minus">−{dels}</span>}
            </span>
            <span className={viewed.has(f.path) ? 'file-viewed on' : 'file-viewed'} role="img" aria-label={viewed.has(f.path) ? 'Viewed' : 'Not viewed yet'} />
          </button>
        )
      })}
    </div>
  )
}

/** The comment box at the end of the conversation. Once a review is started, ⌘↩ adds to it. */
function Composer({ holds, reviewing, onPost, onHold }: { holds: boolean; reviewing: boolean; onPost: (b: string) => void; onHold: (b: string) => void }) {
  const keyLabel = useKeyLabel()
  const [text, setText] = useState('')
  const ready = text.trim() !== ''
  const send = (held: boolean) => {
    if (!ready) return
    if (held) onHold(text.trim())
    else onPost(text.trim())
    setText('')
  }
  return (
    <div className="composer">
      <textarea
        aria-label="Comment"
        placeholder="Leave a comment. Markdown works."
        rows={3}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
            e.preventDefault()
            send(reviewing)
          }
        }}
      />
      <div className="composer-foot">
        <span className="composer-hint">{keyLabel('⌘↩')} {reviewing ? 'to add to your review' : 'to post'}</span>
        {holds && reviewing ? (
          <>
            <button className="act" disabled={!ready} onClick={() => send(false)}>
              Comment Now
            </button>
            <button className="act primary" disabled={!ready} onClick={() => send(true)}>
              <ChatIcon /> Add to Review
            </button>
          </>
        ) : (
          <>
            {holds && (
              <button className="act" disabled={!ready} onClick={() => send(true)}>
                Start a Review
              </button>
            )}
            <button className="act primary" disabled={!ready} onClick={() => send(false)}>
              <ChatIcon /> Comment
            </button>
          </>
        )}
      </div>
    </div>
  )
}

/** A thread: its comments, then Reply and Resolve. In Conversation it starts with its file and line. */
export function ThreadView({
  thread,
  live,
  flat,
  onWhere,
  onResolve,
  onDelete,
  onSave,
}: {
  thread: Thread
  /** The request is open, so the thread can be resolved and drafts edited. */
  live?: boolean
  flat?: boolean
  onWhere?: () => void
  onResolve?: () => void
  onDelete?: (j: number) => void
  onSave?: (j: number, body: string) => void
}) {
  const started = thread.comments.some((c) => !c.pending)
  const head = flat && thread.file
  return (
    <div className={`thread${flat ? ' flat' : ''}${thread.resolved ? ' resolved' : ''}`}>
      {head && (
        <div className="thread-head">
          {onWhere ? (
            <button className="thread-where" onClick={onWhere} title="Show this line in the changes">
              {thread.file}:{thread.line}
            </button>
          ) : (
            <span className="thread-where">
              {thread.file}:{thread.line}
            </span>
          )}
          {thread.resolved && (
            <span className="resolved-chip">
              <CheckIcon /> Resolved
            </span>
          )}
        </div>
      )}
      {thread.comments.map((c, j) => (
        <CommentView
          key={`${j}-${c.body}`}
          comment={c}
          onDelete={live && onDelete ? () => onDelete(j) : undefined}
          onSave={live && onSave ? (body) => onSave(j, body) : undefined}
        />
      ))}
      {started && thread.resolvable && (
        <div className="thread-foot">
          <span className="thread-btn static">
            <ReplyIcon /> Reply
          </span>
          <span className="spacer" />
          {live !== false && (
            <>
              {!head && thread.resolved && <span className="thread-state">Resolved</span>}
              {onResolve ? (
                <button className="thread-btn" onClick={onResolve} aria-label={thread.resolved ? 'Unresolve Thread' : 'Resolve Thread'}>
                  {thread.resolved ? <UndoIcon /> : <CheckIcon />} {thread.resolved ? 'Unresolve' : 'Resolve'}
                </button>
              ) : (
                <span className="thread-btn static">
                  {thread.resolved ? <UndoIcon /> : <CheckIcon />} {thread.resolved ? 'Unresolve' : 'Resolve'}
                </span>
              )}
            </>
          )}
        </div>
      )}
    </div>
  )
}


export function NextStep({
  review,
  above,
  below,
  confirm,
  walking,
  account,
  onAsk,
  onCancel,
  onMerge,
  onStopWalk,
  onAutoMerge,
  onReady,
  onGo,
  onThreads,
}: {
  review: Review
  above?: Review
  below?: Review
  confirm?: Confirm['kind']
  walking: boolean
  account: Account
  onAsk: (kind: Confirm['kind']) => void
  onCancel: () => void
  onMerge: (kind: Confirm['kind']) => void
  onStopWalk: () => void
  onAutoMerge: (on: boolean) => void
  onReady: () => void
  onGo: (r: Review) => void
  onThreads: () => void
}) {
  const s = review.standing ?? 'waiting'
  const auto = review.step?.startsWith('Auto-merge on')
  let note: ReactNode = review.why
  let actions: ReactNode = null
  if (walking) {
    note = 'Merging the stack: each part merges as soon as it’s ready. Keep Mrow open until it’s done.'
    actions = (
      <button className="act" onClick={onStopWalk}>
        Stop
      </button>
    )
  } else if (confirm && s === 'ready') {
    note =
      confirm === 'stack' && above
        ? `Merge ${review.number} into ${review.target} now, then ${above.number} and each part above it once it’s approved and green? It stops at the first part that isn’t.`
        : `Merge ${review.number} into ${review.target}?`
    actions = (
      <>
        <button className="act primary" onClick={() => onMerge(confirm)}>
          <MergeIcon /> {confirm === 'stack' ? 'Merge the Stack' : 'Merge'}
        </button>
        <button className="act" onClick={onCancel}>
          Cancel
        </button>
      </>
    )
  } else if (s === 'ready') {
    actions = (
      <>
        <button className="act primary" onClick={() => onAsk('merge')}>
          <MergeIcon /> Merge
        </button>
        {above && (
          <button className="act" onClick={() => onAsk('stack')}>
            Merge the Stack…
          </button>
        )}
      </>
    )
  } else if (s === 'held' && below) {
    actions = (
      <button className="act" onClick={() => onGo(below)}>
        Go to {below.number}
      </button>
    )
  } else if (s === 'needs' && review.pipeline === 'failed') {
    actions = <span className="act primary static">Open the Pipeline</span>
  } else if (s === 'needs') {
    actions = (
      <button className="act primary" onClick={onThreads}>
        Go to the Threads
      </button>
    )
  } else if (s === 'waiting' || s === 'quiet') {
    actions = (
      <button className="act" onClick={() => onAutoMerge(!auto)} title={`${account.name} merges it by itself once everything passes`}>
        {auto ? 'Turn Auto-Merge Off' : <><MergeIcon /> Merge When Ready</>}
      </button>
    )
  } else if (s === 'draft') {
    actions = (
      <button className="act" onClick={onReady}>
        <CheckIcon /> Mark Ready
      </button>
    )
  }
  return (
    <div className="next-step">
      <p className="next-head">
        <StandingMark standing={s} /> {review.step}
      </p>
      {note && <p className="next-why">{note}</p>}
      {actions && <div className="actions">{actions}</div>}
    </div>
  )
}

export function Checklist({ checks, onGo }: { checks: Check[]; onGo?: (c: Check) => void }) {
  return (
    <ul className="checks">
      {checks.map((c) => {
        const goes = onGo && (c.label === 'Threads' ? c.state === 'bad' : c.label === 'Stack')
        const body = (
          <>
            <CheckMark state={c.state} />
            <span className="check-label">{c.label}</span>
            <span className={c.state === 'bad' ? 'check-detail bad' : 'check-detail'}>{c.detail}</span>
            {goes && <Chevron className="chev right" />}
          </>
        )
        return (
          <li key={c.label}>
            {goes ? (
              <button className="check-line" onClick={() => onGo?.(c)}>
                {body}
              </button>
            ) : (
              <span className="check-line">{body}</span>
            )}
          </li>
        )
      })}
    </ul>
  )
}

export function CheckMark({ state }: { state: Check['state'] }) {
  if (state === 'ok') return <StandingMark standing="ready" />
  if (state === 'bad') return <StandingMark standing="needs" />
  return <StandingMark standing="waiting" />
}

export function AccountPicker({ account, onChange, forceOpen }: { account: Account; onChange?: (p: Provider) => void; forceOpen?: boolean }) {
  const [open, setOpen] = useState(false)
  const isOpen = forceOpen || open
  return (
    <div className="picker">
      <button className="picker-btn" aria-expanded={isOpen} aria-haspopup="menu" onClick={() => !forceOpen && setOpen((v) => !v)}>
        <ProviderMark provider={account.provider} />
        <span className="picker-text">
          <span className="picker-name">{account.name}</span>
          <span className="picker-host">{account.handle}</span>
        </span>
        <Chevron className="chev" />
      </button>
      {isOpen && (
        <div className="picker-menu" role="menu">
          {(Object.values(accounts) as Account[]).map((a) => (
            <button
              key={a.provider}
              role="menuitemradio"
              aria-checked={a.provider === account.provider}
              className="picker-item"
              onClick={() => {
                onChange?.(a.provider)
                setOpen(false)
              }}
            >
              <ProviderMark provider={a.provider} />
              <span className="picker-text">
                <span className="picker-name">
                  {a.name} <span className="picker-handle">{a.handle}</span>
                </span>
                <span className="picker-host">
                  {a.host}, through {a.cli}
                </span>
              </span>
              {a.provider === account.provider && <CheckIcon className="check" />}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

export function DiffView({
  file,
  lines,
  viewed,
  onToggleViewed,
  threadAt,
}: {
  file: string
  lines: Line2[]
  viewed?: boolean
  onToggleViewed?: () => void
  /** The thread under a line, drawn by the review it's in; without it, the line's own, as a still. */
  threadAt?: (line: number) => ReactNode
}) {
  const adds = lines.filter((l) => l.kind === 'add').length
  const dels = lines.filter((l) => l.kind === 'del').length
  const threads = lines.filter((l) => l.thread).length
  return (
    <div className={viewed ? 'diff is-viewed' : 'diff'}>
      <div className="diff-head">
        <span className="diff-file">{file}</span>
        <span className="spacer" />
        {threads > 0 && (
          <span className="diff-threads">
            <BubbleIcon filled />
            {threads}
          </span>
        )}
        <span className="diff-stat">
          {adds > 0 && <span className="plus">+{adds}</span>} {dels > 0 && <span className="minus">−{dels}</span>}
        </span>
        {onToggleViewed && (
          <label className="viewed">
            <input type="checkbox" checked={!!viewed} onChange={onToggleViewed} />
            Viewed
          </label>
        )}
      </div>
      {!viewed && (
        <div className="diff-lines" role="group" aria-label={`Changes to ${file}`}>
          {lines.map((l, i) => (
            <div key={i}>
              <div className={`diff-line ${l.kind}`}>
                <span className="ln">{l.old ?? ''}</span>
                <span className="ln">{l.new ?? ''}</span>
                <span className="sign" aria-hidden="true">
                  {l.kind === 'add' ? '+' : l.kind === 'del' ? '−' : ''}
                </span>
                <code>{highlight(l.code)}</code>
              </div>
              {threadAt
                ? threadAt(i)
                : l.thread && <ThreadView thread={{ id: `${i}`, comments: l.thread, resolvable: true, resolved: !!l.resolved }} />}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
type Line2 = Review['files'][number]['lines'][number]

function CommentView({ comment, onDelete, onSave }: { comment: Comment; onDelete?: () => void; onSave?: (body: string) => void }) {
  const [editing, setEditing] = useState<string>()
  const tools = comment.pending && (onDelete || onSave)
  return (
    <div className={comment.pending ? 'comment pending' : 'comment'}>
      <Avatar name={comment.author} />
      <div className="comment-main">
        <p className="comment-meta">
          <strong>{comment.author}</strong>
          {comment.pending ? (
            <span className="pending-mark" title="Only you can see this until you submit your review">
              Pending
            </span>
          ) : (
            <span>{comment.when}</span>
          )}
          {comment.unseen && <span className="new">· New</span>}
          {tools && editing === undefined && (
            <span className="comment-tools">
              {onSave && (
                <button className="icon-btn small" aria-label="Edit" title="Edit" onClick={() => setEditing(comment.body)}>
                  <PencilIcon />
                </button>
              )}
              {onDelete && (
                <button className="icon-btn small" aria-label="Delete from Review" title="Delete from Review" onClick={onDelete}>
                  <TrashIcon />
                </button>
              )}
            </span>
          )}
        </p>
        {editing !== undefined ? (
          <div className="composer">
            <textarea
              aria-label="Edit your comment"
              rows={2}
              value={editing}
              autoFocus
              onChange={(e) => setEditing(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Escape') setEditing(undefined)
                if (e.key === 'Enter' && (e.metaKey || e.ctrlKey) && editing.trim()) {
                  e.preventDefault()
                  onSave?.(editing.trim())
                  setEditing(undefined)
                }
              }}
            />
            <div className="composer-foot">
              <span className="spacer" />
              <button className="act" onClick={() => setEditing(undefined)}>
                Cancel
              </button>
              <button
                className="act primary"
                disabled={!editing.trim()}
                onClick={() => {
                  onSave?.(editing.trim())
                  setEditing(undefined)
                }}
              >
                Save
              </button>
            </div>
          </div>
        ) : (
          <p className="comment-body">{comment.body}</p>
        )}
      </div>
    </div>
  )
}

function Avatar({ name }: { name: string }) {
  const initials = name === 'You' ? 'RO' : name.split(' ').map((w) => w[0]).join('').slice(0, 2)
  return (
    <span className="avatar" aria-hidden="true">
      {initials}
    </span>
  )
}

/** The chat beside a review, with a comment the agent drafted and you haven't posted. */
export function AskChat({ review, account, compact, onClose }: { review: Review; account: Account; compact?: boolean; onClose?: () => void }) {
  const [agent, setAgent] = useState<(typeof AGENTS)[number]>('Claude')
  const [posted, setPosted] = useState(false)
  const [editing, setEditing] = useState(false)
  const file = review.files[0]
  const line = review.draftLine ?? file?.lines.find((l) => l.kind === 'add')?.new ?? 1
  const name = file?.path.split('/').pop() ?? ''
  const [draft, setDraft] = useState(
    review.draftComment ?? `Is anything else in the project still calling this the old way? If so, it needs the same change before this merges.`,
  )
  const threads = review.files.flatMap((f) => f.lines.filter((l) => l.thread)).length + review.conversation.length
  return (
    <div className={compact ? 'chat compact' : 'chat'}>
      <div className="chat-head">
        <ChatIcon />
        <label className="chat-agent">
          <span className="sr-only">Agent</span>
          Ask{' '}
          <select value={agent} onChange={(e) => setAgent(e.target.value as (typeof AGENTS)[number])}>
            {AGENTS.map((a) => (
              <option key={a}>{a}</option>
            ))}
          </select>
        </label>
        <span className="quiet mono">{review.number}</span>
        <span className="spacer" />
        {onClose && (
          <button className="icon-btn" aria-label="Close the chat" onClick={onClose}>
            <CloseIcon />
          </button>
        )}
      </div>
      <div className="chat-log">
        <div className="chat-q">
          <p className="chat-context">
            <span className="mono">{review.number}</span> Description, {review.files.length} changed {review.files.length === 1 ? 'file' : 'files'},{' '}
            {threads} {threads === 1 ? 'thread' : 'threads'}
          </p>
          <p>Draft a comment for anything that could go wrong here.</p>
        </div>
        <p className="chat-tool">Read {name}</p>
        <p className="msg agent">One worth raising, on the line that changed:</p>
        <div className={posted ? 'draft posted' : 'draft'}>
          <p className="draft-head">
            <ChatIcon />
            <span>
              Draft comment · On {name} line {line}
            </span>
            <span className="spacer" />
            <span className="quiet">{posted ? 'Posted' : 'Not posted'}</span>
          </p>
          {editing ? (
            <textarea aria-label="Comment" value={draft} onChange={(e) => setDraft(e.target.value)} rows={4} />
          ) : (
            <p className="draft-body">{draft}</p>
          )}
          {!posted && (
            <div className="actions end">
              <button className="act" onClick={() => setEditing((v) => !v)}>
                {editing ? 'Done' : 'Edit'}
              </button>
              <button
                className="act primary"
                onClick={() => {
                  setEditing(false)
                  setPosted(true)
                }}
              >
                Post
              </button>
            </div>
          )}
        </div>
      </div>
      <div className="chat-input" aria-hidden="true">
        <span>Reply…</span>
        <span className="chat-folder">{review.project.split('/').pop()}</span>
      </div>
      <span className="sr-only">
        {agent} reads your clone of {review.project}. Nothing is posted to {account.name} until you press Post.
      </span>
    </div>
  )
}

export function StandingMark({ standing }: { standing: Standing | 'ready' | 'needs' | 'waiting' | 'draft' }) {
  const label =
    standing === 'ready' || standing === 'merged'
      ? 'Ready'
      : standing === 'needs'
        ? 'Needs you'
        : standing === 'draft'
          ? 'Draft'
          : standing === 'held'
            ? 'Held'
            : 'Waiting'
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" className={`smark smark-${standing}`} role="img" aria-label={label}>
      {standing === 'ready' || standing === 'merged' ? (
        <>
          <circle cx="8" cy="8" r="7" fill="currentColor" />
          <path d="M5 8.3l2 2 4-4.3" fill="none" stroke="var(--paper)" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
        </>
      ) : standing === 'needs' ? (
        <>
          <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.5" />
          <path d="M8 4.8v3.6" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          <circle cx="8" cy="11" r="0.95" fill="currentColor" />
        </>
      ) : standing === 'draft' ? (
        <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      ) : standing === 'held' ? (
        <>
          <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M5.5 6.8h5M5.5 9.2h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </>
      ) : (
        <>
          <circle cx="8" cy="8" r="6.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
          <path d="M8 5v3.2l2 1.3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </>
      )}
    </svg>
  )
}

function PipelineDot({ pipeline }: { pipeline: Review['pipeline'] }) {
  return (
    <span className={`pdot pdot-${pipeline}`}>
      <span className="sr-only">Pipeline {pipeline}</span>
    </span>
  )
}

export function ProviderMark({ provider }: { provider: Provider }) {
  return (
    <span className={`mark mark-${provider}`} aria-label={provider === 'gitlab' ? 'GitLab' : 'GitHub'} role="img">
      {provider === 'gitlab' ? 'GL' : 'GH'}
    </span>
  )
}

function SearchIcon() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
      <circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
      <path d="M10.5 10.5 14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
    </svg>
  )
}

export function ChatIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
      <path
        d="M3 3.5h10a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H7l-3 2.5v-2.5H3a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
    </svg>
  )
}

function BubbleIcon({ filled }: { filled?: boolean }) {
  return (
    <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true">
      <path
        d="M3 3.5h10a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H7l-3 2.5v-2.5H3a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1z"
        fill={filled ? 'currentColor' : 'none'}
        stroke="currentColor"
        strokeWidth="1.4"
        strokeLinejoin="round"
      />
    </svg>
  )
}

function BellIcon() {
  return (
    <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true">
      <path
        d="M4 11.5V7a4 4 0 0 1 8 0v4.5l1 1H3zM6.5 13.5a1.5 1.5 0 0 0 3 0"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
    </svg>
  )
}

function MergeIcon() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
      <circle cx="4.5" cy="3.5" r="1.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="4.5" cy="12.5" r="1.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="11.5" cy="9" r="1.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M4.5 5.1v5.8M4.5 5.5c0 2.5 2 3.5 5.4 3.5" fill="none" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  )
}

function CloseIcon() {
  return (
    <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true">
      <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  )
}

function Chevron({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true" className={className}>
      <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  )
}

function CheckIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" className={className}>
      <path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  )
}

function OutIcon() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
      <path d="M5 11l6-6M6 4.5h5.5V10" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function ReplyIcon() {
  return (
    <svg viewBox="0 0 16 16" width="11" height="11" aria-hidden="true">
      <path d="M6.5 4L3 7.5 6.5 11M3.5 7.5H10a3 3 0 0 1 3 3V12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function UndoIcon() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
      <path d="M5 3.5L2.5 6 5 8.5M3 6h6.5a3.5 3.5 0 0 1 0 7H6" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function PencilIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
      <path d="M10.5 3l2.5 2.5L6 12.5H3.5V10z" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" />
    </svg>
  )
}

function TrashIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
      <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

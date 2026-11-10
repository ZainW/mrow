import { useMemo, useRef, useState } from 'react'
import type { KeyboardEvent } from 'react'
import { highlight } from '../lib/highlight'
import { accounts, listLabels, reviews } from './mockData'
import type { Account, Comment, Group, Line, ListId, Provider, Review } from './mockData'

const GROUP_ORDER: Group[] = ['Today', 'Yesterday', 'This week', 'Last 14 days']
type Tab = 'overview' | 'changes' | 'discussion'

/** The interactive window in the hero. Everything in it is made up. */
export function AppMock() {
  const [provider, setProvider] = useState<Provider>('github')
  const [list, setList] = useState<ListId>('review')
  const [selectedId, setSelectedId] = useState<string>('gh-1')
  const [tab, setTab] = useState<Tab>('changes')
  const [read, setRead] = useState<Set<string>>(() => new Set())
  const [approved, setApproved] = useState<Set<string>>(() => new Set())
  const [viewed, setViewed] = useState<Set<string>>(() => new Set())
  const [chatOpen, setChatOpen] = useState(false)
  const listRef = useRef<HTMLDivElement>(null)

  const account = accounts[provider]
  const all = reviews[provider]
  const visible = useMemo(() => all.filter((r) => r.list === list), [all, list])
  const selected = visible.find((r) => r.id === selectedId) ?? visible[0]

  function select(r: Review) {
    setSelectedId(r.id)
    setRead((prev) => new Set(prev).add(r.id))
  }

  function switchAccount(next: Provider) {
    setProvider(next)
    const first = reviews[next].find((r) => r.list === list)
    if (first) setSelectedId(first.id)
  }

  function switchList(next: ListId) {
    setList(next)
    const first = all.find((r) => r.list === next)
    if (first) setSelectedId(first.id)
  }

  function onListKey(e: KeyboardEvent<HTMLDivElement>) {
    const step = e.key === 'j' || e.key === 'ArrowDown' ? 1 : e.key === 'k' || e.key === 'ArrowUp' ? -1 : 0
    if (!step || !selected) return
    e.preventDefault()
    const ordered = GROUP_ORDER.flatMap((g) => visible.filter((r) => r.group === g))
    const next = ordered[ordered.indexOf(selected) + step]
    if (next) {
      select(next)
      listRef.current?.querySelector<HTMLButtonElement>(`[data-id="${next.id}"]`)?.focus()
    }
  }

  const waitingCount = all.filter((r) => r.list === 'review').length

  return (
    <figure className="window" aria-label="An interactive illustration of the Mrow window, filled with made-up reviews">
      <div className="window-bar">
        <span className="lights" aria-hidden="true">
          <i />
          <i />
          <i />
        </span>
        <span className="window-title">Mrow</span>
        <span className="dock-badge" title="The Dock badge shows how many reviews are waiting on you">
          {waitingCount}
        </span>
      </div>

      <div className={chatOpen ? 'window-body with-chat' : 'window-body'}>
        <aside className="sidebar">
          <AccountPicker account={account} onChange={switchAccount} />

          <div className="list-bar" role="tablist" aria-label="Lists">
            {(Object.keys(listLabels) as ListId[]).map((id) => (
              <button
                key={id}
                role="tab"
                aria-selected={list === id}
                className="list-bar-item"
                onClick={() => switchList(id)}
              >
                {listLabels[id]}
                <span className="count">{all.filter((r) => r.list === id).length}</span>
              </button>
            ))}
          </div>

          <div className="filter" aria-hidden="true">
            <SearchIcon />
            <span>Filter</span>
            <kbd>⌘K</kbd>
          </div>

          <div className="queue" ref={listRef} onKeyDown={onListKey}>
            {GROUP_ORDER.map((g) => {
              const rows = visible.filter((r) => r.group === g)
              if (!rows.length) return null
              return (
                <section key={g} className="queue-group">
                  <h4>{g}</h4>
                  {rows.map((r) => (
                    <QueueRow
                      key={r.id}
                      review={r}
                      active={r.id === selected?.id}
                      unread={!!r.unread && !read.has(r.id)}
                      onSelect={() => select(r)}
                    />
                  ))}
                </section>
              )
            })}
          </div>
        </aside>

        {selected && (
          <div className="detail">
            <article className="card">
              <header className="card-head">
                <div className="card-ref">
                  <ProviderMark provider={provider} />
                  <span>{selected.ref}</span>
                  {selected.outcome && (
                    <span className={`pill pill-${selected.outcome.toLowerCase()}`}>{selected.outcome}</span>
                  )}
                </div>
                <h3 className="card-title">{selected.title}</h3>
                <p className="card-meta">
                  {selected.author} wants to merge <code>{selected.source}</code> into <code>{selected.target}</code>
                </p>
                <div className="card-actions">
                  <div className="tabs" role="tablist" aria-label="Sections">
                    {(['overview', 'changes', 'discussion'] as Tab[]).map((t) => (
                      <button key={t} role="tab" aria-selected={tab === t} onClick={() => setTab(t)}>
                        {t === 'overview' ? 'Overview' : t === 'changes' ? 'Changes' : 'Discussion'}
                      </button>
                    ))}
                  </div>
                  <span className="spacer" />
                  <button
                    className={chatOpen ? 'ghost-btn on' : 'ghost-btn'}
                    aria-pressed={chatOpen}
                    onClick={() => setChatOpen((v) => !v)}
                    title="Ask Cursor (⌘L)"
                  >
                    <ChatIcon />
                    <span className="hide-sm">Ask Cursor</span>
                  </button>
                  {selected.list === 'review' && (
                    <button
                      className={approved.has(selected.id) ? 'approve-btn done' : 'approve-btn'}
                      onClick={() =>
                        setApproved((prev) => {
                          const next = new Set(prev)
                          if (next.has(selected.id)) next.delete(selected.id)
                          else next.add(selected.id)
                          return next
                        })
                      }
                    >
                      {approved.has(selected.id) ? 'Approved' : 'Approve'}
                    </button>
                  )}
                </div>
              </header>

              <div className="card-body">
                {tab === 'overview' && <p className="card-desc">{selected.description}</p>}
                {tab === 'changes' && (
                  <DiffView
                    file={selected.file}
                    lines={selected.diff}
                    viewed={viewed.has(selected.id)}
                    onToggleViewed={() =>
                      setViewed((prev) => {
                        const next = new Set(prev)
                        if (next.has(selected.id)) next.delete(selected.id)
                        else next.add(selected.id)
                        return next
                      })
                    }
                  />
                )}
                {tab === 'discussion' && <Discussion review={selected} />}
              </div>
            </article>
          </div>
        )}

        {chatOpen && selected && (
          <div className="window-chat">
            <CursorChat review={selected} compact />
          </div>
        )}
      </div>
    </figure>
  )
}

function QueueRow({
  review,
  active,
  unread,
  onSelect,
}: {
  review: Review
  active: boolean
  unread: boolean
  onSelect: () => void
}) {
  return (
    <button
      className={active ? 'row active' : 'row'}
      aria-current={active ? 'true' : undefined}
      data-id={review.id}
      onClick={onSelect}
    >
      <span className={unread ? 'dot on' : 'dot'} aria-label={unread ? 'Changed since you last looked' : undefined} />
      <span className="row-main">
        <span className="row-title">{review.title}</span>
        <span className="row-sub">
          <span className="row-ref">{review.ref}</span>
          <span className={review.rerequested ? 'row-when again' : 'row-when'}>{review.asked}</span>
        </span>
      </span>
      {review.waiting && (
        <span className="row-age" title={`Waiting on you for ${review.waiting.replace('d', ' days')}`}>
          {review.waiting}
        </span>
      )}
    </button>
  )
}

export function AccountPicker({
  account,
  onChange,
  forceOpen,
}: {
  account: Account
  onChange?: (p: Provider) => void
  forceOpen?: boolean
}) {
  const [open, setOpen] = useState(false)
  const isOpen = forceOpen || open
  return (
    <div className="picker">
      <button
        className="picker-btn"
        aria-expanded={isOpen}
        aria-haspopup="menu"
        onClick={() => !forceOpen && setOpen((v) => !v)}
      >
        <ProviderMark provider={account.provider} />
        <span className="picker-text">
          <span className="picker-name">{account.handle}</span>
          <span className="picker-host">{account.host}</span>
        </span>
        <Chevron />
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
                <span className="picker-host">through {a.provider === 'gitlab' ? 'glab' : 'gh'}</span>
              </span>
              {a.provider === account.provider && <Check />}
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
  withReply,
}: {
  file: string
  lines: Line[]
  viewed?: boolean
  onToggleViewed?: () => void
  withReply?: boolean
}) {
  const adds = lines.filter((l) => l.kind === 'add').length
  const dels = lines.filter((l) => l.kind === 'del').length
  return (
    <div className={viewed ? 'diff is-viewed' : 'diff'}>
      <div className="diff-head">
        <span className="diff-file">{file}</span>
        <span className="diff-stat">
          <span className="plus">+{adds}</span> <span className="minus">−{dels}</span>
        </span>
        <span className="spacer" />
        <label className="viewed">
          <input type="checkbox" checked={!!viewed} onChange={() => onToggleViewed?.()} />
          Viewed
        </label>
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
              {l.thread && <Thread comments={l.thread} withReply={withReply} />}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}

function Thread({ comments, withReply }: { comments: Comment[]; withReply?: boolean }) {
  return (
    <div className="thread">
      {comments.map((c, i) => (
        <div key={i} className="comment">
          <span className="avatar" aria-hidden="true">{c.author[0]}</span>
          <div>
            <p className="comment-meta">
              <strong>{c.author}</strong> <span>{c.when}</span>
            </p>
            <p className="comment-body">{c.body}</p>
          </div>
        </div>
      ))}
      {withReply && (
        <div className="reply" aria-hidden="true">
          <span>Reply</span>
          <kbd>⌘↩</kbd>
        </div>
      )}
    </div>
  )
}

function Discussion({ review }: { review: Review }) {
  const threads = review.diff.filter((l) => l.thread)
  if (!threads.length) {
    return <p className="card-desc quiet">No threads yet. Comment on a line in Changes, or below the description.</p>
  }
  return (
    <div className="discussion">
      {threads.map((l, i) => (
        <div key={i}>
          <p className="thread-where">
            {review.file}, line {l.new ?? l.old}
          </p>
          <Thread comments={l.thread ?? []} withReply />
        </div>
      ))}
    </div>
  )
}

export function CursorChat({ review, compact }: { review: Review; compact?: boolean }) {
  return (
    <div className={compact ? 'chat compact' : 'chat'}>
      <div className="chat-head">
        <ChatIcon />
        <span>Ask Cursor</span>
        <span className="spacer" />
        <span className="chat-model">Read-only</span>
      </div>
      <div className="chat-log">
        <p className="chat-context">Given the description, the diff of {review.file} and its threads.</p>
        <div className="msg you">Is anything else calling this the old way?</div>
        <div className="msg agent">
          <p>
            Two callers still expect the old behaviour. Both are in tests, so nothing in production changes, but
            they'll need updating before this merges.
          </p>
          <pre>
            <code>{highlight('// fixtures/' + review.file.split('/').pop() + ', line 12')}</code>
          </pre>
        </div>
      </div>
      <div className="chat-input" aria-hidden="true">
        <span>Ask about this {review.ref.includes('!') ? 'merge request' : 'pull request'}</span>
      </div>
    </div>
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

function ChatIcon() {
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

function Chevron() {
  return (
    <svg viewBox="0 0 16 16" width="10" height="10" aria-hidden="true" className="chev">
      <path d="M4 6l4 4 4-4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
    </svg>
  )
}

function Check() {
  return (
    <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" className="check">
      <path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
    </svg>
  )
}

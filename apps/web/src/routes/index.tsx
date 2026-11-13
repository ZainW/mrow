import { useEffect, useState, type ReactNode } from 'react'
import { createFileRoute } from '@tanstack/react-router'
import { AccountPicker, AppMock, CursorChat, DiffView, ProviderMark } from '../components/AppMock'
import { CopyButton } from '../components/CopyButton'
import { accounts, reviews } from '../components/mockData'

export const Route = createFileRoute('/')({
  component: Home,
})

const REPO = 'https://github.com/ZainW/mrow'
const RELEASES = `${REPO}/releases`
const INSTALL = 'curl -fsSL https://mrow.wania.app/install | bash'
const BUILD_STEPS = ['git clone https://github.com/ZainW/mrow.git', 'cd mrow/apps/desktop', 'scripts/install.sh']

type Build = 'macos-aarch64' | 'linux-x86_64' | 'linux-aarch64'

const DOWNLOADS: { build: Build; system: string; arch: string }[] = [
  { build: 'macos-aarch64', system: 'macOS', arch: 'Apple silicon' },
  { build: 'linux-x86_64', system: 'Linux', arch: 'x86_64' },
  { build: 'linux-aarch64', system: 'Linux', arch: 'arm64' },
]

function Home() {
  return (
    <>
      <a className="skip" href="#main">
        Skip to content
      </a>
      <SiteHeader />
      <main id="main">
        <Hero />
        <Queue />
        <Reviewing />
        <Accounts />
        <AskCursor />
        <Privacy />
        <Keys />
        <Install />
      </main>
      <SiteFooter />
    </>
  )
}

function Wordmark() {
  return (
    <a href="/" className="wordmark" aria-label="Mrow, home">
      <svg viewBox="0 0 32 32" width="22" height="22" aria-hidden="true">
        <path
          fill="currentColor"
          d="M6 4l7 6.2a12 12 0 0 1 6 0L26 4v12.5A10 10 0 0 1 16 28 10 10 0 0 1 6 16.5z"
        />
        <circle cx="12.2" cy="17" r="1.6" className="eye" />
        <circle cx="19.8" cy="17" r="1.6" className="eye" />
      </svg>
      <span>Mrow</span>
    </a>
  )
}

function SiteHeader() {
  return (
    <header className="site-header">
      <Wordmark />
      <nav aria-label="Sections">
        <a href="#queue">Queue</a>
        <a href="#reviewing">Reviewing</a>
        <a href="#accounts">Accounts</a>
        <a href="#ask">Ask Cursor</a>
        <a href="#privacy">Privacy</a>
        <a href="#keys">Keys</a>
      </nav>
      <a className="btn btn-small" href="#install">
        Install
      </a>
    </header>
  )
}

function Hero() {
  return (
    <section className="hero" aria-labelledby="hero-title">
      <div className="hero-copy">
        <h1 id="hero-title">Everything waiting on your review, in the order you were asked.</h1>
        <p className="lede">
          Mrow is a small desktop app for the pull and merge requests where you’re the reviewer. It reads GitLab through{' '}
          <code>glab</code> and GitHub through <code>gh</code>, shows every diff natively, and keeps quiet until
          someone asks for you.
        </p>
        <div className="hero-actions">
          <a className="btn" href="#install">
            Install Mrow
          </a>
          <a className="text-link" href={REPO}>
            Source on GitHub
          </a>
          <span className="hero-note">Free, for macOS 15 and later on Apple silicon, and Linux.</span>
        </div>
      </div>
      <div className="hero-window">
        <AppMock />
        <p className="caption">
          Try it: switch accounts, lists and rows, or move with j and k. Every person and repository in it is made up.
        </p>
      </div>
    </section>
  )
}

/** A reading column with optional margin notes and a wider figure. */
function Chapter({
  id,
  title,
  children,
  aside,
  figure,
  figureWidth = 'wide',
}: {
  id: string
  title: string
  children: ReactNode
  aside?: ReactNode
  figure?: ReactNode
  figureWidth?: 'wide' | 'text'
}) {
  return (
    <section id={id} className="chapter" aria-labelledby={`${id}-title`}>
      <h2 id={`${id}-title`}>{title}</h2>
      <div className="prose">{children}</div>
      {aside && <aside className="note">{aside}</aside>}
      {figure && <div className={`plate plate-${figureWidth}`}>{figure}</div>}
    </section>
  )
}

function Keys1({ keys }: { keys: string[] }) {
  return (
    <span className="keys">
      {keys.map((k) => (
        <kbd key={k}>{k}</kbd>
      ))}
    </span>
  )
}

function Queue() {
  const sam = reviews.gitlab.find((r) => r.id === 'gl-4')
  return (
    <Chapter
      id="queue"
      title="Sorted by when you were asked"
      aside={
        <>
          <p>
            Unread dots mark anything that changed since you last opened it. The Dock badge counts what’s waiting, and
            each new or repeated request sends a notification.
          </p>
          <p>Mrow refreshes every five minutes and whenever you come back to it.</p>
          <p className="note-keys">
            <Keys1 keys={['⌘1', '⌘2', '⌘3']} /> switch lists
          </p>
        </>
      }
      figure={
        <div className="queue-figure">
          <div className="then-now">
            <MiniQueue
              when="Tuesday, 4:10 pm"
              groups={[
                { name: 'Today', rows: [{ title: sam?.title ?? '', ref: sam?.ref ?? '', note: 'Asked 4:02' }] },
                {
                  name: 'This week',
                  rows: [{ title: 'Move preview environments to the shared runner pool', ref: 'northwind/infra!97', note: 'Asked Monday' }],
                },
              ]}
            />
            <MiniQueue
              when="Thursday, 9:40 am, after Sam pushes fixes"
              groups={[
                {
                  name: 'Today',
                  rows: [{ title: sam?.title ?? '', ref: sam?.ref ?? '', note: 'Re-requested 9:40', fresh: true }],
                },
                {
                  name: 'This week',
                  rows: [
                    { title: 'Move preview environments to the shared runner pool', ref: 'northwind/infra!97', note: '3d', old: true },
                  ],
                },
              ]}
            />
          </div>
          <dl className="lists">
            <div>
              <dt>To review</dt>
              <dd>You haven’t reviewed it yet, you’ve started, or new commits reset your review.</dd>
            </div>
            <div>
              <dt>Reviewed</dt>
              <dd>You approved, commented or asked for changes, and it’s still open.</dd>
            </div>
            <div>
              <dt>Done</dt>
              <dd>Merged or closed in the last 14 days. The window is yours to change.</dd>
            </div>
          </dl>
        </div>
      }
    >
      <p>
        Most review lists sort by when a request was opened or last touched. Mrow sorts by the moment you were asked to
        look. When someone asks again after pushing fixes, the request comes back to the top of Today.
      </p>
      <p>
        The queue falls into Today, Yesterday, This week, This month and Older. Anything that has waited on you for
        more than two days shows its age in clay, so the old ones don’t hide under the new.
      </p>
    </Chapter>
  )
}

type MiniRow = { title: string; ref: string; note: string; fresh?: boolean; old?: boolean }

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
                <span className="mini-title">{r.title}</span>
                <span className="mini-sub">
                  <span>{r.ref}</span>
                  <span className={r.old ? 'age' : r.fresh ? 'again' : undefined}>{r.note}</span>
                </span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  )
}

function Reviewing() {
  const pr = reviews.github[0]
  return (
    <Chapter
      id="reviewing"
      title="The whole review, without the browser"
      aside={
        <dl className="note-keylist">
          <div>
            <dt><Keys1 keys={['⇧⌘A']} /></dt>
            <dd>Approve</dd>
          </div>
          <div>
            <dt><Keys1 keys={['⌘↩']} /></dt>
            <dd>Post the comment</dd>
          </div>
          <div>
            <dt><Keys1 keys={['⌥⌘1', '⌥⌘2', '⌥⌘3']} /></dt>
            <dd>Overview, Changes, Discussion</dd>
          </div>
        </dl>
      }
      figure={
        pr && (
          <div className="review-figure">
            <div className="review-figure-head">
              <span className="review-figure-title">{pr.title}</span>
              <span className="approve-btn static">Approve</span>
            </div>
            <DiffView file={pr.file} lines={pr.diff} withReply />
          </div>
        )
      }
    >
      <p>
        The description renders the way its author wrote it. Every file’s diff has line numbers and syntax
        highlighting, and comment threads sit on the lines they’re about.
      </p>
      <p>
        Comment on a line or on the whole request, tick files off as viewed while you go, and approve when you’re
        satisfied. Keep a few open in tabs and move between them.
      </p>
    </Chapter>
  )
}

function Accounts() {
  return (
    <Chapter
      id="accounts"
      title="GitLab and GitHub, one picker"
      aside={
        <p>
          Sign in once with <code>glab auth login</code> or <code>gh auth login</code>. Mrow uses whatever those tools
          are signed in to.
        </p>
      }
      figure={
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
      }
    >
      <p>
        Mrow reads GitLab through the <code>glab</code> command-line tool and GitHub through <code>gh</code>. Signed in
        to both? The picker at the top of the sidebar flips between them, the way you’d switch teams in a chat app.
      </p>
      <p>Each account keeps its own queue, its own unread dots and its own three lists.</p>
    </Chapter>
  )
}

function AskCursor() {
  const pr = reviews.github[0]
  return (
    <Chapter
      id="ask"
      title="Ask Cursor about the change"
      aside={
        <>
          <p>
            Needs Cursor’s agent CLI, signed in. Commands already on your own Cursor allowlist still run without asking.
          </p>
          <dl className="note-keylist">
            <div>
              <dt><Keys1 keys={['⌘L']} /></dt>
              <dd>Open or close the chat</dd>
            </div>
            <div>
              <dt><Keys1 keys={['⇧⌘L']} /></dt>
              <dd>New chat</dd>
            </div>
            <div>
              <dt><Keys1 keys={['esc']} /></dt>
              <dd>Stop the answer</dd>
            </div>
          </dl>
        </>
      }
      figure={pr && <CursorChat review={pr} />}
      figureWidth="text"
    >
      <p>
        Press ⌘L and a chat opens beside the review, run by Cursor’s <code>agent acp</code>. Your first question
        carries the description, the diff and the threads, and the agent can read your local clone as it’s checked
        out. Ask where else a function is called, or what a change might break.
      </p>
      <p>
        It’s read-only three ways. Chats run in Cursor’s ask mode, Mrow offers the agent no file or terminal access,
        and it declines every permission request, showing each one in the chat. You get one chat per request, kept
        across relaunches.
      </p>
    </Chapter>
  )
}

function Privacy() {
  return (
    <Chapter
      id="privacy"
      title="Talks to nothing but glab and gh"
      figure={
        <div className="flow" role="img" aria-label="Mrow talks to glab, which talks to GitLab, and to gh, which talks to GitHub. Only when you open Ask Cursor does it also talk to Cursor's agent.">
          <div className="flow-node flow-app">Mrow</div>
          <div className="flow-paths">
            <div className="flow-path">
              <span className="flow-line" />
              <span className="flow-node"><code>glab</code></span>
              <span className="flow-line" />
              <span className="flow-node flow-far">GitLab</span>
            </div>
            <div className="flow-path">
              <span className="flow-line" />
              <span className="flow-node"><code>gh</code></span>
              <span className="flow-line" />
              <span className="flow-node flow-far">GitHub</span>
            </div>
            <div className="flow-path optional">
              <span className="flow-line" />
              <span className="flow-node"><code>agent acp</code></span>
              <span className="flow-line" />
              <span className="flow-node flow-far">Cursor, when you ask</span>
            </div>
          </div>
        </div>
      }
    >
      <p>
        Mrow has no server, no sign-up and no analytics. Every request goes through the <code>glab</code> and{' '}
        <code>gh</code> tools you already use, and their tokens stay in your keychain, where those tools put them.
        Mrow never sees a token. All it keeps is a few preferences.
      </p>
      <p>
        The only exception is the one you choose. Open Ask Cursor, and your question goes to Cursor through its agent,
        along with the review it’s about.
      </p>
    </Chapter>
  )
}

const SHORTCUTS: [string[], string][] = [
  [['⌘1', '⌘2', '⌘3'], 'To review, Reviewed, Done'],
  [['⌥⌘1', '⌥⌘2', '⌥⌘3'], 'Overview, Changes, Discussion'],
  [['↑', '↓', 'j', 'k'], 'Move through the list'],
  [['⌥⌘]', '⌥⌘['], 'Next or previous request'],
  [['⌘K', '⌘F'], 'Filter the list'],
  [['⌘R'], 'Refresh'],
  [['⌘O', '↩'], 'Open in the browser'],
  [['⇧⌘A'], 'Approve'],
  [['⇧⌘C'], 'Copy link'],
  [['⌘↩'], 'Post the comment'],
  [['⌃⌘S'], 'Show or hide the sidebar'],
  [['⌘L'], 'Ask Cursor: open the chat, move into it, or close it'],
  [['⇧⌘L', '⌥⌘L'], 'New chat, choose the model'],
  [['←', '→'], 'Change the model’s effort, in the model picker'],
  [['esc'], 'Stop the answer, in the chat'],
]

function Keys() {
  return (
    <Chapter id="keys" title="Every action has a key" figureWidth="text" figure={
      <table className="shortcuts">
        <caption className="sr-only">Keyboard shortcuts</caption>
        <thead className="sr-only">
          <tr>
            <th scope="col">Keys</th>
            <th scope="col">Action</th>
          </tr>
        </thead>
        <tbody>
          {SHORTCUTS.map(([keys, action]) => (
            <tr key={action}>
              <td>
                <Keys1 keys={keys} />
              </td>
              <td>{action}</td>
            </tr>
          ))}
        </tbody>
      </table>
    }>
      <p>You can work through a whole morning’s reviews without touching the trackpad.</p>
    </Chapter>
  )
}

/**
 * The build that fits the visitor's computer, worked out after hydration so the server render stays
 * neutral. Undefined on phones, Windows, or when we can't tell; an Intel Mac reads as Apple silicon.
 */
function useLocalBuild() {
  const [build, setBuild] = useState<Build>()
  useEffect(() => {
    const nav = navigator as Navigator & {
      userAgentData?: {
        platform: string
        getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string }>
      }
    }
    const ua = navigator.userAgent
    const platform = nav.userAgentData?.platform ?? navigator.platform
    if (/android|iphone|ipad/i.test(ua)) return
    if (/mac/i.test(platform)) {
      setBuild('macos-aarch64')
    } else if (/linux/i.test(platform)) {
      setBuild(/aarch64|arm/i.test(platform + ua) ? 'linux-aarch64' : 'linux-x86_64')
      nav.userAgentData?.getHighEntropyValues?.(['architecture']).then(
        ({ architecture }) => {
          if (architecture) setBuild(architecture === 'arm' ? 'linux-aarch64' : 'linux-x86_64')
        },
        () => {},
      )
    }
  }, [])
  return build
}

function Install() {
  const local = useLocalBuild()
  return (
    <Chapter
      id="install"
      title="Install it"
      aside={
        <>
          <p>
            Mrow keeps itself up to date. It checks for a new release every few hours and asks you to Restart to
            Update. Turn that off in Settings → Updates.
          </p>
          <p>
            The Mac build isn’t notarized. Installed with the command it opens normally; a copy downloaded in the
            browser needs right-click → Open the first time.
          </p>
        </>
      }
      figureWidth="text"
      figure={
        <div className="install">
          <div className="terminal">
            <div className="terminal-head">
              <span>Terminal</span>
              <CopyButton text={INSTALL} />
            </div>
            <pre className="command">
              <code>{INSTALL}</code>
            </pre>
          </div>
          <p className="build-after">
            On a Mac it puts Mrow in <code>~/Applications</code> and opens it. On Linux it installs <code>mrow</code>{' '}
            and its desktop entry into <code>~/.local</code>. Then sign in with <code>glab auth login</code>,{' '}
            <code>gh auth login</code>, or both.
          </p>

          <h3 className="install-sub">Or download it</h3>
          <ul className="downloads">
            {DOWNLOADS.map((d) => (
              <li key={d.build}>
                <a
                  className={d.build === local ? 'download is-local' : 'download'}
                  href={`${RELEASES}/latest/download/mrow-${d.build}.tar.gz`}
                >
                  <span className="download-system">{d.system}</span>
                  <span className="download-arch">{d.arch}</span>
                  {d.build === local && <span className="download-local">Your computer</span>}
                  <span className="download-file">mrow-{d.build}.tar.gz</span>
                </a>
              </li>
            ))}
          </ul>
          <p className="build-after">
            The latest release. Older ones, and what changed in each, are on the{' '}
            <a className="text-link inline" href={RELEASES}>
              releases page
            </a>
            .
          </p>

          <h3 className="install-sub">Or build it yourself</h3>
          <div className="terminal">
            <div className="terminal-head">
              <span>Terminal</span>
              <CopyButton text={BUILD_STEPS.join('\n')} label="Copy all" />
            </div>
            <ol className="steps">
              {BUILD_STEPS.map((s) => (
                <li key={s}>
                  <code>{s}</code>
                </li>
              ))}
            </ol>
          </div>
          <p className="build-after">
            Needs a Rust toolchain (<code>rustup</code>); on Linux, also <code>libxkbcommon-dev</code> and{' '}
            <code>libwayland-dev</code>, and run <code>scripts/package-linux.sh --install</code> instead. Builds from
            source don’t update themselves.
          </p>
        </div>
      }
    >
      <p>One command downloads the latest release, checks it against the release’s checksum and installs it.</p>
      <ul className="needs">
        <li>macOS 15 or later on Apple silicon, or Linux (x86_64 or arm64) with a Vulkan driver</li>
        <li>
          <code>glab</code> signed in for GitLab, <code>gh</code> for GitHub, or both
        </li>
      </ul>
    </Chapter>
  )
}

function SiteFooter() {
  return (
    <footer className="site-footer">
      <Wordmark />
      <p>Free, and it runs entirely on your computer.</p>
      <p className="quiet">Not affiliated with GitLab, GitHub or Cursor. Everyone in the illustrations is made up.</p>
      <p className="footer-links">
        <a className="text-link" href="#install">
          Install
        </a>
        <a className="text-link" href={RELEASES}>
          Releases
        </a>
        <a className="text-link" href={REPO}>
          Source on GitHub
        </a>
      </p>
    </footer>
  )
}

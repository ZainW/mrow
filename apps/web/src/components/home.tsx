import type { ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { Keys } from './Chapter'
import { CompareFigure, MemoryFigure } from './figures'
import { GetMrow } from './Download'
import { ArrowIcon, CheckIcon, GitHubIcon, RssIcon } from './icons'
import { AskArt, MemoryArt, MoveArt, PaletteArt, StackArt, TabsArt, TokenArt } from './featureArt'
import { CatMark } from './Layout'
import { formatDate } from '../lib/changelog'
import { LATEST } from '../lib/releases'
import { REPO } from '../lib/site'

const EXTRAS = [
  'Two accounts, GitHub and GitLab, with one switcher and a key for each',
  'Snooze a request until morning, and it comes back on its own, even after a restart',
  'One click previews a request in a tab; a second click keeps it',
  'An inbox behind the bell collects what changed since you last looked',
  'Notifications for each kind of event, set to Notify, Inbox or Off',
  'Updates download in the background and are checked against a signature',
]

function Feature({
  size,
  title,
  art,
  link,
  children,
}: {
  size: 2 | 3 | 4
  title: string
  art: ReactNode
  link?: ReactNode
  children: ReactNode
}) {
  return (
    <article className={`feat feat-${size}`}>
      {art}
      <div className="feat-body">
        <h3>{title}</h3>
        <p>{children}</p>
        {link && <p className="feat-link">{link}</p>}
      </div>
    </article>
  )
}

function More({ to, hash, children }: { to: '/ask' | '/reviewing' | '/mine' | '/privacy' | '/keys' | '/'; hash?: string; children: string }) {
  return (
    <Link to={to} hash={hash}>
      {children}
      <ArrowIcon />
    </Link>
  )
}

export function Features() {
  return (
    <section id="features" className="band-section" aria-labelledby="features-title">
      <div className="sec-wrap">
        <header className="sec-head">
          <p className="eyebrow">What it does</p>
          <h2 id="features-title">Everything that’s on you. Nothing that isn’t.</h2>
          <p>
            Mrow keeps the requests waiting on you at the top, folds away the ones waiting on someone else, and lets you
            finish a review without opening the browser.
          </p>
        </header>
        <div className="feat-grid">
          <Feature size={4} title="Ask the agent you already use" art={<AskArt />} link={<More to="/ask">How Ask works</More>}>
            Press <Keys keys={['⌘L']} /> and Cursor, Claude Code, opencode or pi reads the diff, the threads and your clone.
            It drafts comments on their lines, and nothing posts until you post it.
          </Feature>
          <Feature size={2} title="Lighter than one GitHub tab" art={<MemoryArt />} link={<More to="/" hash="memory">The measurements</More>}>
            Written in Rust and drawn on your graphics card, with no browser engine inside.
          </Feature>
          <Feature size={2} title="A review in three tabs" art={<TabsArt />} link={<More to="/reviewing">How reviewing works</More>}>
            Overview, Changes and Conversation. Comment, resolve threads, then approve or request changes.
          </Feature>
          <Feature size={2} title="Whose move it is" art={<MoveArt />} link={<More to="/mine">Your own requests</More>}>
            Your own requests by Your move, Waiting on reviewers and Drafts, each with its next step.
          </Feature>
          <Feature size={2} title="Your token stays in gh" art={<TokenArt />} link={<More to="/privacy">What it connects to</More>}>
            Mrow reaches GitHub through gh and never reads your token. GitHub Enterprise and self-hosted GitLab work too.
          </Feature>
          <Feature size={3} title="Every action has a key" art={<PaletteArt />} link={<More to="/keys">All shortcuts</More>}>
            Can’t remember one? The command palette runs any command, or jumps to a request by title or number.
          </Feature>
          <Feature size={3} title="Stacks stay together" art={<StackArt />} link={<More to="/mine" hash="stacks">How stacks work</More>}>
            Requests that target each other’s branches show as one group, bottom first. Merge the Stack lands them in order.
          </Feature>
        </div>
        <ul className="extras">
          {EXTRAS.map((item) => (
            <li key={item}>
              <CheckIcon className="extras-check" />
              {item}
            </li>
          ))}
        </ul>
      </div>
    </section>
  )
}

const POINTS = [
  { title: 'Your agent, your plan', text: 'It runs on your own sign-in, so there’s no second subscription and nothing marked up.' },
  { title: 'No account, no server', text: 'No sign-up, no analytics, no crash reports. It’s MIT-licensed.' },
  {
    title: 'Opens already drawn',
    text: 'Your lists and the requests you were reading are on screen at once. The refresh fills in what’s new.',
  },
]

export function OutOfBrowser() {
  return (
    <section className="out" aria-labelledby="out-title">
      <div className="sec-wrap out-grid">
        <div>
          <p className="eyebrow">Out of the browser</p>
          <h2 id="out-title">Your reviews, out of a tab.</h2>
          <p className="out-lede">
            Review in an app that lives on your computer, with your own tools. Nobody runs a service behind it, so
            nothing about your work is kept anywhere else.
          </p>
          <ul className="out-points">
            {POINTS.map((p) => (
              <li key={p.title}>
                <h3>{p.title}</h3>
                <p>{p.text}</p>
              </li>
            ))}
          </ul>
        </div>
        <div className="out-figure">
          <CompareFigure />
        </div>
      </div>
    </section>
  )
}

export function Memory() {
  return (
    <section id="memory" className="band-section" aria-labelledby="memory-title">
      <div className="sec-wrap">
        <header className="sec-head">
          <p className="eyebrow">Memory</p>
          <h2 id="memory-title">Lighter than one GitHub tab.</h2>
          <p>
            Mrow is written in Rust and drawn on your graphics card with Rune, with no browser engine and no Electron.
            With a 40-file, 12,000-line diff open, the whole app used 33 MB; one pull request in a Chrome tab used 157 MB,
            before counting the browser around it.
          </p>
        </header>
        <div className="memory-plate">
          <MemoryFigure />
        </div>
      </div>
    </section>
  )
}

export function ChangelogTeaser() {
  const items = LATEST.sections.flatMap((s) => s.items)
  return (
    <section className="band-section" aria-labelledby="log-title">
      <div className="sec-wrap log-grid">
        <div>
          <p className="eyebrow">Changelog</p>
          <h2 id="log-title">Every change, written down.</h2>
          <p className="log-lede">
            Release builds update themselves in the background, checked against their signature. Each release says
            what’s new, what’s fixed and what’s faster.
          </p>
          <div className="log-actions">
            <Link to="/changelog" className="btn btn-secondary">
              Read the changelog
            </Link>
            <a href="/changelog/rss.xml" className="btn btn-ghost">
              <RssIcon className="log-rss" />
              RSS
            </a>
          </div>
        </div>
        <article className="log-card">
          <header>
            <span className="log-version">{LATEST.version}</span>
            <span className="chip-soft">Latest</span>
            <time dateTime={LATEST.date}>{formatDate(LATEST.date)}</time>
          </header>
          <ul>
            {items.map((i) => (
              <li key={i.text}>{i.text}</li>
            ))}
          </ul>
          <Link to="/changelog" className="log-more">
            Every release
            <ArrowIcon />
          </Link>
        </article>
      </div>
    </section>
  )
}

export function ClosingCard() {
  return (
    <section className="cta" aria-labelledby="cta-title">
      <div className="cta-card">
        <CatMark size={56} className="cta-mark" />
        <h2 id="cta-title">Clear your review queue.</h2>
        <p>Free and open source. Sign in to gh and your pull requests are already there.</p>
        <GetMrow
          note={false}
          secondary={
            <a className="btn btn-big btn-secondary" href={REPO}>
              <GitHubIcon size={17} /> Source on GitHub
            </a>
          }
        />
        <p className="cta-quiet">macOS 15 on Apple silicon · Linux x86_64 and arm64</p>
      </div>
    </section>
  )
}

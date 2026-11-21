import { useLayoutEffect, useRef, useState } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { AppMock } from '../components/AppMock'
import { accounts } from '../components/mockData'
import type { Provider } from '../components/mockData'
import { Chapter, Keys } from '../components/Chapter'
import { GetMrow } from '../components/Download'
import { AGENT_ACCOUNTS, CompareFigure, DraftFigure, FlowFigure, MemoryFigure } from '../components/figures'
import { Page } from '../components/Layout'
import { REPO, seo } from '../lib/site'

export const Route = createFileRoute('/')({
  head: () =>
    seo({
      path: '/',
      title: 'Mrow: the inbox for pull requests',
      description:
        'A free, native app for the pull requests waiting on your review and the ones you opened. It asks the agent you already use, keeps your token in gh, and uses less memory than one GitHub tab. macOS and Linux; GitLab too.',
    }),
  component: Home,
})

const PAGES = [
  {
    to: '/reviewing',
    title: 'Reviewing',
    text: 'Requests waiting on you, sorted by when you were asked. Each one opens in three tabs: overview, changes and conversation. Comment, resolve threads, approve or request changes without going to the browser, and snooze what can wait.',
  },
  {
    to: '/mine',
    title: 'Mine',
    text: 'The requests you opened, grouped by whose move it is. Each says what’s left before it merges, stacked requests stay together, and an inbox collects what changed.',
  },
  {
    to: '/ask',
    title: 'Ask',
    text: 'Talk through the open review with Cursor, Claude Code, opencode or pi. The agent can draft comments. Nothing is posted until you post it.',
  },
] as const

function Home() {
  const [provider, setProvider] = useState<Provider>('github')
  return (
    <Page>
      <section className="hero" aria-labelledby="hero-title">
        <div className="hero-copy">
          <h1 id="hero-title">
            The inbox for <Swap word={`${accounts[provider].noun}s`} />.
          </h1>
          <p className="lede">
            The pull requests waiting on your review, and the ones you opened, sorted by whose move it is. Mrow is a
            native app for macOS and Linux. It asks the agent you already use, works through <code>gh</code> so it never
            sees your token, and costs nothing. GitLab works too, through <code>glab</code>.
          </p>
          <GetMrow />
        </div>
        <div className="hero-window">
          <AppMock provider={provider} onProvider={setProvider} />
          <p className="caption">
            It works. Switch lists, switch to the GitLab account, open the bell, or double-click a row to keep it in a tab. The people
            and projects are made up.
          </p>
        </div>
      </section>

      <Chapter
        id="agent"
        title="Bring your own agent"
        figure={<DraftFigure />}
        figureWidth="text"
        aside={
          <>
            <dl className="note-agents">
              {AGENT_ACCOUNTS.map(([who, how]) => (
                <div key={who}>
                  <dt>{who}</dt>
                  <dd>{how}</dd>
                </div>
              ))}
            </dl>
            <p>
              <Link to="/ask" hash="limits" className="text-link inline">
                How each one is kept to reading
              </Link>
            </p>
          </>
        }
      >
        <p>
          Press <Keys keys={['⌘L']} /> and Mrow starts the agent you already use: Cursor, Claude Code, opencode or pi.
          It runs on your own sign-in and plan, with the model you choose. Mrow doesn’t sell AI and doesn’t sit between
          you and the model, so there’s no second subscription and nothing marked up.
        </p>
        <p>
          The first question carries the description, the diff and the threads, and the agent can read and search your
          clone. So it can tell you where else a function is called, not only what changed. It can’t edit a file, run a
          command or post. Ask for comments and it drafts them on the lines they belong to; you edit and post them, or
          you don’t. Send a follow-up while it’s still answering and it stops and takes that instead.
        </p>
      </Chapter>

      <Chapter id="native" title="Lighter than one GitHub tab" figure={<MemoryFigure />}>
        <p>
          Mrow is written in Rust and drawn on your graphics card with Rune. There’s no browser engine inside it and no
          Electron. With a 40-file, 12,000-line diff open, the whole app used 33 MB. One pull request in a Chrome tab
          used 157 MB, before counting the browser around it.
        </p>
        <p>
          It opens with your lists and the requests you were last reading already drawn, and the refresh fills in what’s
          new. Lists, reviews and tabs switch on the key press, with no page to load, and every action has{' '}
          <Link to="/keys" className="text-link inline">
            a key
          </Link>
          . For the ones you don’t remember, <Keys keys={['⇧⌘P']} /> opens a palette: type part of a command to run it,
          or part of a request’s title or number to go to it. The macOS download is 12 MB.
        </p>
      </Chapter>

      <Chapter
        id="token"
        title="Your token stays in gh"
        figure={<FlowFigure />}
        aside={
          <p>
            Everything Mrow connects to, and when, is on{' '}
            <Link to="/privacy" className="text-link inline">
              the Privacy page
            </Link>
            .
          </p>
        }
      >
        <p>
          Mrow reaches GitHub through <code>gh</code> and GitLab through <code>glab</code>, the command-line tools you’ve
          already signed in to. Your token stays where they keep it, usually your system’s keychain. Mrow never reads
          it, stores it or sends it anywhere.
        </p>
        <p>
          There’s no OAuth app to approve and no organization admin to ask. If you can use <code>gh</code> at work, Mrow
          works there too, GitHub Enterprise and self-hosted GitLab included.
        </p>
      </Chapter>

      <Chapter id="free" title="Free, with no account to make">
        <p>
          There’s no Mrow server, no web version and no sign-up. Nobody runs a service behind it, so there’s nothing to
          subscribe to, and nothing about your work is kept anywhere but your computer. No analytics, no crash reports.
        </p>
        <p>
          It’s MIT-licensed, and{' '}
          <a href={REPO} className="text-link inline">
            the source is on GitHub
          </a>
          . Read what it does, build it yourself, change it. Release builds check GitHub releases for a new version every
          six hours, and Settings turns that off; a copy you build never checks.
        </p>
      </Chapter>

      <Chapter id="compare" title="Out of the browser" figure={<CompareFigure />}>
        <p>What changes when your reviews move out of a tab and into Mrow.</p>
      </Chapter>

      <section className="chapter rundown" aria-labelledby="rundown-title">
        <h2 id="rundown-title">What it does</h2>
        <ul className="rundown-list">
          {PAGES.map((p) => (
            <li key={p.to}>
              <Link to={p.to} className="rundown-link">
                {p.title}
                <span aria-hidden="true"> →</span>
              </Link>
              <p>{p.text}</p>
            </li>
          ))}
        </ul>
      </section>

      <section className="chapter get-block" aria-labelledby="get-title">
        <h2 id="get-title">Download it</h2>
        <div className="prose">
          <p>
            It’s free, and it updates itself. Before you open it, sign in to <code>gh</code> for GitHub,{' '}
            <code>glab</code> for GitLab, or both.
          </p>
        </div>
        <div className="plate plate-text">
          <GetMrow />
        </div>
      </section>
    </Page>
  )
}

/**
 * A word that changes in place: the old one rises out as the new one rises in, and the width
 * between them eases so the text after it doesn't jump.
 */
function Swap({ word }: { word: string }) {
  const [shown, setShown] = useState(word)
  const [leaving, setLeaving] = useState<string>()
  const [width, setWidth] = useState<number>()
  const current = useRef<HTMLSpanElement>(null)
  if (word !== shown) {
    setLeaving(shown)
    setShown(word)
  }
  useLayoutEffect(() => {
    setWidth(current.current?.offsetWidth)
  }, [shown])
  return (
    <span className="swap" style={{ width }}>
      <span key={shown} ref={current} className={leaving ? 'swap-word in' : 'swap-word'}>
        {shown}
      </span>
      {leaving && (
        <span key={`out-${leaving}`} className="swap-word out" aria-hidden="true" onAnimationEnd={() => setLeaving(undefined)}>
          {leaving}
        </span>
      )}
    </span>
  )
}

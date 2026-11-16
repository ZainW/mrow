import { useLayoutEffect, useRef, useState } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { AppMock } from '../components/AppMock'
import { accounts } from '../components/mockData'
import type { Provider } from '../components/mockData'
import { GetMrow } from '../components/Download'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/')({
  head: () =>
    seo({
      path: '/',
      title: 'Mrow: the inbox for pull requests',
      description:
        'Mrow is a desktop app for the pull requests waiting on your review, and the ones you opened. It works through gh, and with GitLab through glab, on macOS and Linux.',
    }),
  component: Home,
})

const PAGES = [
  {
    to: '/reviewing',
    title: 'Reviewing',
    text: 'Requests waiting on you, sorted by when you were asked. Each one opens in three tabs: overview, changes and conversation. Comment, resolve threads, approve or request changes without going to the browser.',
  },
  {
    to: '/mine',
    title: 'Mine',
    text: 'The requests you opened, grouped by whose move it is. Each says what’s left before it merges, stacked requests stay together, and an inbox collects what changed.',
  },
  {
    to: '/ask',
    title: 'Ask',
    text: 'Talk through the open review with Cursor, Claude, opencode or pi. The agent can draft comments. Nothing is posted until you post it.',
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
            desktop app for macOS and Linux that works through <code>gh</code>, so it never sees your token. GitLab
            works too, through <code>glab</code>.
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
        <p className="rundown-after">
          There’s no server, no account and no analytics.{' '}
          <Link to="/privacy" className="text-link inline">
            The privacy page
          </Link>{' '}
          lists everything Mrow talks to. Every action has{' '}
          <Link to="/keys" className="text-link inline">
            a key
          </Link>
          , on macOS and Linux.
        </p>
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

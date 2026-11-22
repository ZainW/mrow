import { useLayoutEffect, useRef, useState } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { AppMock } from '../components/AppMock'
import { accounts } from '../components/mockData'
import type { Provider } from '../components/mockData'
import { GetMrow } from '../components/Download'
import { ChangelogTeaser, ClosingCard, Features, Memory, OutOfBrowser } from '../components/home'
import { ArrowIcon } from '../components/icons'
import { Page } from '../components/Layout'
import { LATEST } from '../lib/releases'
import { seo } from '../lib/site'

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

function Home() {
  const [provider, setProvider] = useState<Provider>('github')
  return (
    <Page>
      <section className="hero" aria-labelledby="hero-title">
        <div className="hero-bg" aria-hidden="true" />
        <div className="hero-copy">
          <Link to="/changelog" hash={LATEST.anchor} className="release-pill">
            <span className="chip-soft">v{LATEST.version}</span>
            <span className="release-pill-text">{LATEST.sections[0]?.items[0]?.text}</span>
            <ArrowIcon className="release-pill-arrow" />
          </Link>
          <h1 id="hero-title">
            The inbox for <Swap word={`${accounts[provider].noun}s`} />.
          </h1>
          <p className="lede">
            The pull requests waiting on your review, and the ones you opened, sorted by whose move it is. A native app for
            macOS and Linux that brings the agent you already use. GitLab works too.
          </p>
          <GetMrow
            secondary={
              <Link to="/reviewing" className="btn btn-big btn-secondary">
                How reviewing works
              </Link>
            }
          />
        </div>
        <div className="hero-window">
          <AppMock provider={provider} onProvider={setProvider} />
          <p className="caption">
            It works. Switch lists, switch to the GitLab account, open the bell, or double-click a row to keep it in a tab. The people
            and projects are made up.
          </p>
        </div>
      </section>
      <Features />
      <OutOfBrowser />
      <Memory />
      <ChangelogTeaser />
      <ClosingCard />
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

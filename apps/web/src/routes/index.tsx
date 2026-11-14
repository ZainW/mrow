import { createFileRoute, Link } from '@tanstack/react-router'
import { AppMock } from '../components/AppMock'
import { GetMrow } from '../components/Download'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/')({
  head: () =>
    seo({
      path: '/',
      title: 'Mrow: your reviews and your own requests, on your desktop',
      description:
        'Mrow is a small desktop app for the pull and merge requests waiting on your review, and the ones you opened. GitLab through glab, GitHub through gh, for macOS and Linux.',
    }),
  component: Home,
})

const PAGES = [
  {
    to: '/reviewing',
    title: 'Reviewing',
    text: 'Four lists, the reviews sorted by when you were asked. Each review is one page with a contents rail beside it.',
  },
  {
    to: '/mine',
    title: 'Mine',
    text: 'Your own requests by whose move it is, with stacks, what’s left before each one merges, and an inbox for what changed.',
  },
  {
    to: '/ask',
    title: 'Ask',
    text: 'A chat with Cursor, Claude, opencode or pi about the open review. It drafts comments; you decide what gets posted.',
  },
  { to: '/privacy', title: 'Privacy', text: 'No server, no account, no analytics. Everything Mrow talks to, and when.' },
  { to: '/keys', title: 'Keys', text: 'Every action has a key, on macOS and on Linux.' },
] as const

function Home() {
  return (
    <Page>
      <section className="hero" aria-labelledby="hero-title">
        <div className="hero-copy">
          <h1 id="hero-title">Everything waiting on your review, in the order you were asked.</h1>
          <p className="lede">
            Mrow is a small desktop app for pull and merge requests: the ones waiting on your review, and the ones you
            opened. It reads GitLab through <code>glab</code> and GitHub through <code>gh</code>, shows every diff
            natively, and keeps quiet until it’s your move.
          </p>
          <GetMrow />
        </div>
        <div className="hero-window">
          <AppMock />
          <p className="caption">
            Try it: switch lists and accounts, open the bell, double-click a row to keep it in a tab. Everyone in it is
            made up.
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
      </section>

      <section className="chapter get-block" aria-labelledby="get-title">
        <h2 id="get-title">Download it</h2>
        <div className="prose">
          <p>
            Free. You need <code>glab</code> signed in for GitLab, <code>gh</code> for GitHub, or both. Once it’s
            installed, Mrow keeps itself up to date.
          </p>
        </div>
        <div className="plate plate-text">
          <GetMrow />
          <p className="get-more">
            <Link to="/download" className="text-link">
              Every download, the install command and building from source
            </Link>
          </p>
        </div>
      </section>
    </Page>
  )
}

import { useEffect, useRef, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { RELEASES, REPO, VERSION } from '../lib/site'

const NAV = [
  { to: '/reviewing', label: 'Reviewing' },
  { to: '/mine', label: 'Mine' },
  { to: '/ask', label: 'Ask' },
  { to: '/privacy', label: 'Privacy' },
  { to: '/keys', label: 'Keys' },
] as const

/** Set once the first page has rendered, so later pages know they were navigated to. */
let landed = false

/** Every page: the skip link, the header, the page itself and the footer. */
export function Page({ children }: { children: ReactNode }) {
  const main = useRef<HTMLElement>(null)
  useEffect(() => {
    // After a navigation, start keyboard and screen reader users at the new page, not on the link.
    if (landed) main.current?.focus({ preventScroll: true })
    landed = true
  }, [])
  return (
    <>
      <a className="skip" href="#main">
        Skip to content
      </a>
      <SiteHeader />
      <main id="main" ref={main} tabIndex={-1}>
        {children}
      </main>
      <SiteFooter />
    </>
  )
}

export function Wordmark() {
  return (
    <Link to="/" className="wordmark" aria-label="Mrow, home">
      <svg viewBox="0 0 32 32" width="22" height="22" aria-hidden="true">
        <path fill="currentColor" d="M6 4l7 6.2a12 12 0 0 1 6 0L26 4v12.5A10 10 0 0 1 16 28 10 10 0 0 1 6 16.5z" />
        <circle cx="12.2" cy="17" r="1.6" className="eye" />
        <circle cx="19.8" cy="17" r="1.6" className="eye" />
      </svg>
      <span>Mrow</span>
    </Link>
  )
}

function SiteHeader() {
  return (
    <header className="site-header">
      <Wordmark />
      <nav aria-label="Pages">
        {NAV.map((n) => (
          <Link key={n.to} to={n.to}>
            {n.label}
          </Link>
        ))}
      </nav>
      <Link to="/download" className="btn btn-small">
        Download
      </Link>
    </header>
  )
}

function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="footer-about">
        <Wordmark />
        <p>Free, and it runs on your computer.</p>
        <p className="quiet">
          Not affiliated with GitLab, GitHub or the makers of the agents it can run. Everyone in the illustrations is made up.
        </p>
      </div>
      <nav className="footer-links" aria-label="More">
        {NAV.map((n) => (
          <Link key={n.to} to={n.to} className="text-link">
            {n.label}
          </Link>
        ))}
        <Link to="/download" className="text-link">
          Download
        </Link>
        <a className="text-link" href={RELEASES}>
          Releases
        </a>
        <a className="text-link" href={REPO}>
          Source on GitHub
        </a>
      </nav>
      <p className="quiet footer-version">Version {VERSION}</p>
    </footer>
  )
}

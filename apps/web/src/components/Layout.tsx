import { useEffect, useRef, useState, type ReactNode } from 'react'
import { Link } from '@tanstack/react-router'
import { RELEASES, REPO, VERSION } from '../lib/site'
import { GitHubIcon } from './icons'

const NAV = [
  { to: '/reviewing', label: 'Reviewing' },
  { to: '/mine', label: 'Mine' },
  { to: '/ask', label: 'Ask' },
  { to: '/keys', label: 'Keys' },
  { to: '/privacy', label: 'Privacy' },
  { to: '/changelog', label: 'Changelog' },
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

export function CatMark({ size = 22, className }: { size?: number; className?: string }) {
  return (
    <svg viewBox="0 0 32 32" width={size} height={size} aria-hidden="true" className={className}>
      <path fill="currentColor" d="M6 4l7 6.2a12 12 0 0 1 6 0L26 4v12.5A10 10 0 0 1 16 28 10 10 0 0 1 6 16.5z" />
      <circle cx="12.2" cy="17" r="1.6" className="eye" />
      <circle cx="19.8" cy="17" r="1.6" className="eye" />
    </svg>
  )
}

export function Wordmark() {
  return (
    <Link to="/" className="wordmark" aria-label="Mrow, home">
      <CatMark />
      <span>Mrow</span>
    </Link>
  )
}

function SiteHeader() {
  const [scrolled, setScrolled] = useState(false)
  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 4)
    onScroll()
    window.addEventListener('scroll', onScroll, { passive: true })
    return () => window.removeEventListener('scroll', onScroll)
  }, [])
  return (
    <header className={scrolled ? 'site-header is-scrolled' : 'site-header'}>
      <Wordmark />
      <nav aria-label="Pages">
        {NAV.map((n) => (
          <Link key={n.to} to={n.to}>
            {n.label}
          </Link>
        ))}
      </nav>
      <a className="header-icon" href={REPO} aria-label="Mrow on GitHub">
        <GitHubIcon size={19} />
      </a>
      <Link to="/download" className="btn btn-small">
        Download
      </Link>
    </header>
  )
}

function SiteFooter() {
  return (
    <footer className="site-footer">
      <div className="footer-grid">
        <div className="footer-about">
          <Wordmark />
          <p>Free and open source. It runs on your computer, and nowhere else.</p>
          <p className="quiet">
            Not affiliated with GitHub, GitLab or the makers of the agents it can run. Everyone in the illustrations is made up.
          </p>
          <p className="quiet footer-version">Version {VERSION}</p>
        </div>
        <FooterColumn title="The app">
          <Link to="/reviewing">Reviewing</Link>
          <Link to="/mine">Mine</Link>
          <Link to="/ask">Ask</Link>
          <Link to="/keys">Keys</Link>
        </FooterColumn>
        <FooterColumn title="Get it">
          <Link to="/download">Download</Link>
          <Link to="/changelog">Changelog</Link>
          <a href={RELEASES}>Releases</a>
          <a href="/changelog/rss.xml">RSS feed</a>
        </FooterColumn>
        <FooterColumn title="Project">
          <a href={REPO}>Source on GitHub</a>
          <Link to="/privacy">Privacy</Link>
          <a href={`${REPO}/blob/main/LICENSE`}>MIT License</a>
        </FooterColumn>
      </div>
    </footer>
  )
}

function FooterColumn({ title, children }: { title: string; children: ReactNode }) {
  return (
    <nav className="footer-col" aria-label={title}>
      <h2>{title}</h2>
      {children}
    </nav>
  )
}

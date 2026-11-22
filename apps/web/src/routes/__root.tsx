/// <reference types="vite/client" />
import type { ReactNode } from 'react'
import { createRootRoute, HeadContent, Link, Outlet, Scripts } from '@tanstack/react-router'
import { Page } from '../components/Layout'
import appCss from '../styles.css?url'

// Each page sets its own title, description and canonical address (`seo` in lib/site.ts).
export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'Mrow' },
      { name: 'color-scheme', content: 'light dark' },
      { property: 'og:site_name', content: 'Mrow' },
    ],
    links: [
      { rel: 'preload', href: '/fonts/InterVariable.woff2', as: 'font', type: 'font/woff2', crossOrigin: 'anonymous' },
      { rel: 'preload', href: '/fonts/MrowReading-Regular.woff2', as: 'font', type: 'font/woff2', crossOrigin: 'anonymous' },
      // Bold and mono are above the fold too: the window's titles and refs, and the Linux install command.
      { rel: 'preload', href: '/fonts/MrowReading-Bold.woff2', as: 'font', type: 'font/woff2', crossOrigin: 'anonymous' },
      { rel: 'preload', href: '/fonts/GeistMono-Variable.woff2', as: 'font', type: 'font/woff2', crossOrigin: 'anonymous' },
      { rel: 'stylesheet', href: appCss },
      { rel: 'alternate', type: 'application/rss+xml', title: 'Mrow changelog', href: '/changelog/rss.xml' },
      { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' },
    ],
  }),
  component: RootComponent,
  notFoundComponent: NotFound,
})

/**
 * The home page's intro plays once per session. This runs before the first paint, so a load after
 * the home page has played it starts with the intro off; the home page itself records that it played.
 */
const INTRO = `try{if(sessionStorage.getItem('mrow-intro'))document.documentElement.dataset.intro='seen'}catch(e){}`

function RootComponent() {
  return (
    <RootDocument>
      <Outlet />
    </RootDocument>
  )
}

function RootDocument({ children }: Readonly<{ children: ReactNode }>) {
  return (
    // The intro script adds data-intro to <html> before React hydrates.
    <html lang="en" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: INTRO }} />
        <HeadContent />
      </head>
      <body>
        {children}
        <Scripts />
      </body>
    </html>
  )
}

function NotFound() {
  return (
    <Page>
      <div className="not-found">
        <h1>Nothing waiting here.</h1>
        <p>
          This page doesn’t exist. <Link to="/">Go back to the Mrow home page</Link>.
        </p>
      </div>
    </Page>
  )
}

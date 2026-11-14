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
      { rel: 'stylesheet', href: appCss },
      { rel: 'icon', href: '/favicon.svg', type: 'image/svg+xml' },
    ],
  }),
  component: RootComponent,
  notFoundComponent: NotFound,
})

function RootComponent() {
  return (
    <RootDocument>
      <Outlet />
    </RootDocument>
  )
}

function RootDocument({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <html lang="en">
      <head>
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

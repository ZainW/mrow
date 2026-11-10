/// <reference types="vite/client" />
import type { ReactNode } from 'react'
import { createRootRoute, HeadContent, Outlet, Scripts } from '@tanstack/react-router'
import appCss from '../styles.css?url'

const description =
  'Mrow is a small desktop app for the pull and merge requests waiting on your review. GitLab through glab, GitHub through gh, nothing else.'

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'Mrow: your review queue, on your desktop' },
      { name: 'description', content: description },
      { name: 'color-scheme', content: 'light dark' },
      { property: 'og:title', content: 'Mrow' },
      { property: 'og:description', content: description },
    ],
    links: [
      { rel: 'preload', href: '/fonts/InterVariable.woff2', as: 'font', type: 'font/woff2', crossOrigin: 'anonymous' },
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
    <main className="not-found">
      <h1>Nothing waiting here.</h1>
      <p>
        This page doesn't exist. <a href="/">Go back to the Mrow home page</a>.
      </p>
    </main>
  )
}

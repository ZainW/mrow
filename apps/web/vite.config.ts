import { readFileSync } from 'node:fs'
import { tanstackStart } from '@tanstack/react-start/plugin/vite'
import viteReact from '@vitejs/plugin-react'
import { defineConfig, type Plugin } from 'vite'
import { changelogRss, parseChangelog } from './src/lib/changelog'

// The app's version is always the latest release's: the release workflow bumps it as it tags.
const cargo = readFileSync(new URL('../desktop/Cargo.toml', import.meta.url), 'utf8')
const version = /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1]
if (!version) throw new Error('No version in apps/desktop/Cargo.toml')

const changelog = parseChangelog(readFileSync(new URL('../../CHANGELOG.md', import.meta.url), 'utf8'))
const rss = changelogRss(changelog, { site: 'https://mrow.wania.app', repo: 'https://github.com/ZainW/mrow' })

/** /changelog/rss.xml, built from the same CHANGELOG.md the changelog page reads. */
const changelogFeed: Plugin = {
  name: 'mrow:changelog-rss',
  configureServer(server) {
    server.middlewares.use('/changelog/rss.xml', (_req, res) => {
      res.setHeader('Content-Type', 'application/rss+xml; charset=utf-8')
      res.end(rss)
    })
  },
  generateBundle() {
    if (this.environment.name === 'client') this.emitFile({ type: 'asset', fileName: 'changelog/rss.xml', source: rss })
  },
}

export default defineConfig({
  server: { port: 3000 },
  define: { __MROW_VERSION__: JSON.stringify(version) },
  plugins: [
    // Start's plugin must come before React's. Every page is prerendered from the links between
    // them, so dist/client is plain static assets (Cloudflare, see wrangler.jsonc). Pages are
    // written as reviewing.html rather than reviewing/index.html, so /reviewing is served as is.
    tanstackStart({ prerender: { enabled: true, crawlLinks: true, autoSubfolderIndex: false } }),
    viteReact(),
    changelogFeed,
  ],
})

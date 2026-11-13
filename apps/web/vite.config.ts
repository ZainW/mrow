import { readFileSync } from 'node:fs'
import { tanstackStart } from '@tanstack/react-start/plugin/vite'
import viteReact from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The app's version is always the latest release's: the release workflow bumps it as it tags.
const cargo = readFileSync(new URL('../desktop/Cargo.toml', import.meta.url), 'utf8')
const version = /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1]
if (!version) throw new Error('No version in apps/desktop/Cargo.toml')

export default defineConfig({
  server: { port: 3000 },
  define: { __MROW_VERSION__: JSON.stringify(version) },
  plugins: [
    // Start's plugin must come before React's. Every page is prerendered from the links between
    // them, so dist/client is plain static assets (Cloudflare, see wrangler.jsonc). Pages are
    // written as reviewing.html rather than reviewing/index.html, so /reviewing is served as is.
    tanstackStart({ prerender: { enabled: true, crawlLinks: true, autoSubfolderIndex: false } }),
    viteReact(),
  ],
})

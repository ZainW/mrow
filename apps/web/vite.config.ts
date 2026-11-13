import { tanstackStart } from '@tanstack/react-start/plugin/vite'
import viteReact from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  server: { port: 3000 },
  plugins: [
    // Start's plugin must come before React's. The site is one static page: prerender it so
    // dist/client can be served as plain assets (Cloudflare, see cloudflare.config.ts).
    tanstackStart({ prerender: { enabled: true, crawlLinks: true } }),
    viteReact(),
  ],
})

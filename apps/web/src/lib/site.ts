export const SITE = 'https://mrow.wania.app'
export const REPO = 'https://github.com/ZainW/mrow'
export const RELEASES = `${REPO}/releases`
export const INSTALL = 'curl -fsSL https://mrow.wania.app/install | bash'
export const BUILD_STEPS = ['git clone https://github.com/ZainW/mrow.git', 'cd mrow/apps/desktop', 'scripts/install.sh']
/** apps/desktop/Cargo.toml's version, read at build time; it always equals the latest release. */
export const VERSION = __MROW_VERSION__

export type Build = 'macos-aarch64' | 'linux-x86_64' | 'linux-aarch64'

export const DOWNLOADS: { build: Build; system: string; arch: string; file: string; how: string }[] = [
  {
    build: 'macos-aarch64',
    system: 'macOS',
    arch: 'Apple silicon',
    // The disk image, zipped under a name that carries the version.
    file: `mrow-${VERSION}-macos-aarch64.zip`,
    how: 'Unzip it, open the disk image and drag Mrow into Applications.',
  },
  {
    build: 'linux-x86_64',
    system: 'Linux',
    arch: 'x86_64',
    file: 'mrow-linux-x86_64.tar.gz',
    how: 'A folder with bin/mrow, a desktop entry and an icon, laid out like ~/.local.',
  },
  {
    build: 'linux-aarch64',
    system: 'Linux',
    arch: 'arm64',
    file: 'mrow-linux-aarch64.tar.gz',
    how: 'A folder with bin/mrow, a desktop entry and an icon, laid out like ~/.local.',
  },
]

/** A file in the release this site was built for (VERSION), which is the latest. */
export const downloadUrl = (file: string) => `${RELEASES}/download/v${VERSION}/${file}`

/** A page's title, description and canonical address. */
export function seo({ path, title, description }: { path: string; title: string; description: string }) {
  const url = `${SITE}${path}`
  return {
    meta: [
      { title },
      { name: 'description', content: description },
      { property: 'og:title', content: title },
      { property: 'og:description', content: description },
      { property: 'og:url', content: url },
    ],
    links: [{ rel: 'canonical', href: url }],
  }
}

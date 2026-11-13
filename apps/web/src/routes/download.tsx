import { createFileRoute } from '@tanstack/react-router'
import { Chapter, PageHead } from '../components/Chapter'
import { BuildTerminal, DownloadList, InstallTerminal, ReleasesLink } from '../components/Download'
import { Page } from '../components/Layout'
import { seo, VERSION } from '../lib/site'

export const Route = createFileRoute('/download')({
  head: () =>
    seo({
      path: '/download',
      title: 'Download Mrow',
      description:
        'Download Mrow for macOS on Apple silicon or for Linux on x86_64 and arm64, or install it with one command. Free, and it keeps itself up to date.',
    }),
  component: Download,
})

function Download() {
  return (
    <Page>
      <PageHead title="Download Mrow">
        <p>
          Version {VERSION}, free. Each link is the latest release. Older ones, and what changed in each, are on the{' '}
          <ReleasesLink />.
        </p>
      </PageHead>

      <section className="chapter" aria-label="Downloads">
        <div className="plate plate-text plate-first">
          <DownloadList />
        </div>
        <aside className="note">
          <p>
            The Mac build isn’t notarized. Installed with the command below it opens normally. A downloaded copy is
            blocked the first time you open it: allow it under System Settings → Privacy &amp; Security → Open Anyway.
          </p>
          <p>There’s no build for Intel Macs.</p>
        </aside>
      </section>

      <Chapter id="install" title="Or install it with one command" figureWidth="text" figure={<InstallTerminal />}>
        <p>
          The command downloads the latest release for your computer, checks it against the checksum in the release and
          installs it. On a Mac it puts Mrow in <code>~/Applications</code> and opens it. On Linux it installs{' '}
          <code>mrow</code> with its desktop entry and icon into <code>~/.local</code>.
        </p>
      </Chapter>

      <Chapter id="needs" title="What you need">
        <ul className="needs">
          <li>macOS 15 or later on Apple silicon, or Linux on x86_64 or arm64 with a Vulkan driver</li>
          <li>
            <code>glab</code> signed in for GitLab (<code>glab auth login</code>), <code>gh</code> for GitHub (
            <code>gh auth login</code>), or both
          </li>
        </ul>
        <p>
          On Linux it also needs <code>libxkbcommon-x11-0</code>, and <code>mesa-vulkan-drivers</code> covers Intel, AMD
          and software rendering.
        </p>
      </Chapter>

      <Chapter id="updates" title="It keeps itself up to date">
        <p>
          Every few hours Mrow checks for a new release and downloads it in the background, checked against the signature
          and checksum in that release. It goes in place when you click Restart to Update. Turn that off in Settings →
          Updates, or run <code>mrow --update</code> from a terminal.
        </p>
      </Chapter>

      <Chapter id="source" title="Or build it yourself" figureWidth="text" figure={<BuildTerminal />}>
        <p>
          You need a Rust toolchain (<code>rustup</code>). On Linux you also need <code>libxkbcommon-dev</code>,{' '}
          <code>libwayland-dev</code> and <code>pkg-config</code>, and you run <code>scripts/package-linux.sh --install</code>{' '}
          instead of the last step. Builds from source don’t update themselves.
        </p>
      </Chapter>
    </Page>
  )
}

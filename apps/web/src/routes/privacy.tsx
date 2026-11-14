import { createFileRoute, Link } from '@tanstack/react-router'
import { Chapter, PageHead } from '../components/Chapter'
import { FlowFigure } from '../components/figures'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/privacy')({
  head: () =>
    seo({
      path: '/privacy',
      title: 'What Mrow talks to',
      description:
        'Mrow has no server, no account and no analytics. It reaches GitLab and GitHub through glab and gh, the agent you ask, and GitHub releases for updates.',
    }),
  component: Privacy,
})

function Privacy() {
  return (
    <Page>
      <PageHead title="What Mrow talks to">
        <p>
          Mrow has no server, no sign-up and no analytics. Here is everything it connects to, and when.
        </p>
      </PageHead>

      <Chapter id="forges" title="GitLab and GitHub, through their own tools" figure={<FlowFigure />}>
        <p>
          Every request to GitLab or GitHub goes through the <code>glab</code> and <code>gh</code> tools you already
          use. Their tokens stay where those tools keep them, and Mrow never sees one. Pictures uploaded to a GitLab
          project sit behind your login, so those come through <code>glab</code> too.
        </p>
        <p>
          Avatars and other pictures in a description or comment are downloaded with <code>curl</code> from wherever
          they’re hosted, the way a browser would load them.
        </p>
      </Chapter>

      <Chapter id="ask" title="The agent, when you ask">
        <p>
          Nothing goes to an agent until you open a chat and ask. Then your question goes to the agent you chose, with
          the request’s description, diff and threads, and from the agent to whichever model it uses. The agent can read
          and search your clone; it can’t change it or post anything. <Link to="/ask" className="text-link inline">How
          each agent is held to that</Link>.
        </p>
      </Chapter>

      <Chapter id="updates" title="Updates, every few hours">
        <p>
          Builds from the releases check for a new version shortly after launch and then every six hours. Each check
          downloads <code>latest.json</code> from this project’s GitHub releases with <code>curl</code>, sending nothing
          but a <code>Mrow/&lt;version&gt;</code> user agent. When there’s a new version, Mrow downloads it from the same
          place, checks its signature and checksum, and puts it in place when you click Restart to Update.
        </p>
        <p>Turn it off in Settings → Updates. A build you made from source never checks.</p>
      </Chapter>

      <Chapter id="kept" title="What it keeps">
        <p>
          Your settings, the inbox, and which chat goes with which request, in Mrow’s own folder on your computer
          (<code>~/Library/Application Support/Mrow</code> on macOS, <code>~/.config/mrow</code> on Linux). GitLab uploads
          it has shown and downloaded updates go in its cache folder. None of it is sent anywhere.
        </p>
      </Chapter>
    </Page>
  )
}

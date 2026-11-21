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
        'Mrow has no server, no account and no analytics. It reaches GitHub and GitLab through gh and glab, the agent you ask, and GitHub releases for updates.',
    }),
  component: Privacy,
})

function Privacy() {
  return (
    <Page>
      <PageHead title="What Mrow talks to">
        <p>
          Mrow has no server, no web version, no sign-up and no analytics. It’s an app on your computer and nothing
          else. Here is everything it connects to, and when.
        </p>
      </PageHead>

      <Chapter id="forges" title="GitHub and GitLab, through their own tools" figure={<FlowFigure />}>
        <p>
          Every request to GitHub or GitLab goes through the <code>gh</code> and <code>glab</code> tools you already
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
          Your settings and snoozes, the inbox, and which chat goes with which request, in Mrow’s own folder on your
          computer (<code>~/Library/Application Support/Mrow</code> on macOS, <code>~/.config/mrow</code> on Linux).
        </p>
        <p>
          So it can open with something on screen, Mrow keeps each account’s lists and the changes and threads of the last
          20 requests you opened, up to 64 MB, in its cache folder. GitLab uploads it has shown and downloaded updates go
          there too. None of it is sent anywhere.
        </p>
      </Chapter>
    </Page>
  )
}

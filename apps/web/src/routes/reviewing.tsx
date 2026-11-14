import { createFileRoute, Link } from '@tanstack/react-router'
import { Chapter, KeyList, Keys, PageHead } from '../components/Chapter'
import { AccountsFigure, QueueFigure, ReviewFigure } from '../components/figures'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/reviewing')({
  head: () =>
    seo({
      path: '/reviewing',
      title: 'Reviewing in Mrow',
      description:
        'The reviews waiting on you, sorted by when you were asked, and each review as one page with a contents rail: description, every diff, the conversation and your verdict.',
    }),
  component: Reviewing,
})

function Reviewing() {
  return (
    <Page>
      <PageHead title="The reviews waiting on you">
        <p>
          Mrow keeps the requests you were asked to review in one list, newest ask first, and opens each one as a single
          page you read from top to bottom.
        </p>
      </PageHead>

      <Chapter
        id="queue"
        title="Sorted by when you were asked"
        aside={
          <>
            <p>
              On macOS the Dock badge counts what’s waiting on you. A new review request, or one asked again, sends a
              notification and lands in the <Link to="/mine" className="text-link inline">inbox</Link>.
            </p>
            <p>Mrow refreshes every five minutes (you can change that) and whenever you come back to it.</p>
            <KeyList items={[[['⌘1', '⌘2', '⌘3', '⌘4'], 'To review, Reviewed, Mine, Done']]} />
          </>
        }
        figure={<QueueFigure />}
      >
        <p>
          Most review lists sort by when a request was opened or last touched. Mrow sorts by the moment you were asked to
          look. When someone asks again after pushing fixes, the request comes back to the top of Today, and its row says
          who asked again.
        </p>
        <p>
          The list falls into Today, Yesterday, This week, This month and Older. Anything that has waited on you for more
          than two days shows its age in clay, so the old ones don’t hide under the new.
        </p>
      </Chapter>

      <Chapter
        id="review"
        title="One page, top to bottom"
        aside={
          <KeyList
            items={[
              [['J', 'K'], 'Next or previous part of the page; each file is one'],
              [['⌥⌘1', '⌥⌘2', '⌥⌘3'], 'The top, the changes, the conversation'],
              [['⇧⌘A'], 'Approve'],
              [['⌘↩'], 'Post the comment'],
            ]}
          />
        }
        figure={<ReviewFigure />}
      >
        <p>
          A review is one page: who asked and when, the description rendered the way its author wrote it, every file’s
          diff with syntax highlighting and line numbers, the conversation with a box to add to it, and your verdict at
          the end. Comment threads sit under the lines they’re about.
        </p>
        <p>
          Beside it, a contents rail lists those parts and every file, marks the one you’re reading and jumps to any of
          them. Each file shows how many threads it has, filled in when there’s a reply you haven’t seen. Tick files off
          as viewed while you go, then approve, or revoke an approval you gave.
        </p>
        <p>
          Reading text is set in Charis SIL, a serif made for long reading. The interface is Inter and the code is Geist
          Mono.
        </p>
      </Chapter>

      <Chapter
        id="tabs"
        title="A few open at once"
        aside={
          <KeyList
            items={[
              [['⌘T'], 'Keep in a tab'],
              [['⌘W', '⌥⌘W'], 'Close the tab, close the others'],
              [['⌃Tab', '⇧⌘]', '⇧⌘['], 'Next or previous tab'],
            ]}
          />
        }
      >
        <p>
          A single click on a row previews it in a tab that the next click replaces. Double-click, or press <Keys keys={['⌘T']} />,
          to keep it, and move between the reviews you’re partway through without losing your place in any of them.
        </p>
      </Chapter>

      <Chapter
        id="accounts"
        title="GitLab and GitHub, one switcher"
        aside={
          <>
            <p>
              Sign in once with <code>glab auth login</code> or <code>gh auth login</code>. Mrow uses whatever those tools
              are signed in to.
            </p>
            <KeyList items={[[['⌃1', '⌃2'], 'Switch account (Alt+1, Alt+2 on Linux)']]} />
          </>
        }
        figure={<AccountsFigure />}
      >
        <p>
          Mrow reads GitLab through the <code>glab</code> command-line tool and GitHub through <code>gh</code>. Signed in
          to both? The switcher at the top of the sidebar flips between them, the way you’d switch teams in a chat app.
        </p>
        <p>
          Each account keeps its own lists. Both refresh together, so the badge and notifications cover both. In Settings
          you can turn either off, or point it at a self-hosted GitLab or GitHub Enterprise host.
        </p>
      </Chapter>
    </Page>
  )
}

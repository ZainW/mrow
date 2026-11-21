import { createFileRoute, Link } from '@tanstack/react-router'
import { Chapter, KeyList, Keys, PageHead } from '../components/Chapter'
import { AccountsFigure, FinishFigure, QueueFigure, ReviewFigure } from '../components/figures'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/reviewing')({
  head: () =>
    seo({
      path: '/reviewing',
      title: 'Reviewing in Mrow',
      description:
        'The reviews waiting on you, sorted by when you were asked. Each opens in three tabs, Overview, Changes and Conversation, and you finish it in the app: resolve threads, hold comments, then comment, request changes or approve. Snooze what can wait.',
    }),
  component: Reviewing,
})

function Reviewing() {
  return (
    <Page>
      <PageHead title="The reviews waiting on you">
        <p>
          Mrow keeps the requests you were asked to review in one list, newest ask first. Each one opens in three tabs, and
          you can see it through to the end without going to the browser.
        </p>
      </PageHead>

      <Chapter
        id="queue"
        title="Sorted by when you were asked"
        aside={
          <>
            <p>
              On macOS the Dock badge counts what’s waiting on you. A new review request, or one asked again, sends a
              notification and lands in the <Link to="/mine" hash="inbox" className="text-link inline">inbox</Link>, and so
              does a push to one you asked changes on.
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
        <p>
          Ask for changes and the request gets out of your way. It waits in a folded <em>Waiting on authors</em> at the
          end of To review, out of the count and the badge, until its author pushes. Then it rises to{' '}
          <em>Changes came in</em> at the top, with who pushed and when. <strong>Review Again</strong> brings one back
          without waiting.
        </p>
      </Chapter>

      <Chapter
        id="review"
        title="Overview, Changes, Conversation"
        aside={
          <KeyList
            items={[
              [['⌥⌘1', '⌥⌘2', '⌥⌘3'], 'Overview, Changes, Conversation'],
              [['J', 'K'], 'Next or previous file, in Changes'],
              [['N', 'P'], 'Next or previous open thread'],
              [['V'], 'Mark the file viewed, go to the next'],
              [['R', 'C'], 'Reply to the thread, write a comment'],
              [['⇧⌘A'], 'Approve'],
            ]}
          />
        }
        figure={<ReviewFigure />}
      >
        <p>
          A review is three tabs under a bar that stays put. <strong>Overview</strong> has who asked and when, the
          description rendered the way its author wrote it, and where your review stands, with a button into the first
          file you haven’t viewed. <strong>Changes</strong> lists the files, how many you’ve viewed, then each diff with
          syntax highlighting, line numbers, and the threads under the lines they’re about. <strong>Conversation</strong>{' '}
          has every thread, those on lines too, then a box to add to it. A thread’s file and line take you to that line in
          Changes.
        </p>
        <p>
          The bar holds “Open in GitHub” (or GitLab), Review and Approve on every tab, so a quick approval never takes a
          scroll. Each tab keeps its place when you leave it.
        </p>
        <p>
          The keys walk you through it. <Keys keys={['N']} /> and <Keys keys={['P']} /> go from open thread to open
          thread, <Keys keys={['R']} /> replies to the one you’re on, and <Keys keys={['C']} /> starts a comment.{' '}
          <Keys keys={['V']} /> marks the file viewed and goes on to the next one you haven’t. Past eight files, the list
          groups them by folder, each with how many you’ve viewed and how many lines it changes, folded so the first diff
          still starts on the first screen.
        </p>
        <p>
          Reading text is set in Charis SIL, a serif made for long reading. The interface is Inter and the code is Geist
          Mono.
        </p>
      </Chapter>

      <Chapter
        id="finish"
        title="Finish the review here"
        aside={
          <>
            <p>
              GitHub can’t hold a comment on the whole pull request in a review; the summary covers that. On GitLab,
              Request Changes uses GitLab’s own requested-changes state.
            </p>
            <KeyList
              items={[
                [['⌘↩'], 'Post the comment, or add it to your review once one is started'],
                [['⇧⌘R'], 'Finish the review'],
                [['esc'], 'Close the panel'],
              ]}
            />
          </>
        }
        figure={<FinishFigure />}
      >
        <p>
          Every thread that can be resolved has <strong>Resolve</strong> under it, and <strong>Unresolve</strong> once
          it is. Resolved threads are dimmed and marked, so the open ones stand out.
        </p>
        <p>
          A comment can post at once, or wait. <strong>Start a Review</strong> holds it in a pending review on the forge,
          where only you can see it. After that, <Keys keys={['⌘↩']} /> adds each comment to the review, with <strong>Comment Now</strong>{' '}
          beside it for one that shouldn’t wait. Pending comments show where they’ll land, marked Pending, and you can
          edit or delete them until they go.
        </p>
        <p>
          <strong>Review</strong> in the bar counts what you’re holding and opens “Finish your review”: a summary if you
          want one, then <strong>Comment</strong>, <strong>Request Changes</strong> or <strong>Approve</strong>, and
          everything posts together. Approve is still one press when nothing is held; with comments pending, it opens the
          panel so they don’t go out unseen.
        </p>
      </Chapter>

      <Chapter
        id="snooze"
        title="Snooze what can wait"
        aside={
          <>
            <p>Snoozes last across launches, and end when the request merges or closes.</p>
            <KeyList items={[[['⇧⌘S'], 'Snooze until morning']]} />
          </>
        }
      >
        <p>
          Snooze a request until morning, 9:00 on the next working day, so a Friday snooze comes back on Monday. Or snooze
          it until something changes on it, or mute it until you unmute it. It leaves the list, the count and the badge,
          and waits in a fold at the end of the list that says when each one comes back.
        </p>
        <p>
          A snoozed request doesn’t notify you. The change that ends a snooze does, as any change would. It works the same
          on your own requests in Mine.
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
        title="GitHub and GitLab, one switcher"
        aside={
          <>
            <p>
              Sign in once with <code>gh auth login</code> or <code>glab auth login</code>. Mrow uses whatever those tools
              are signed in to.
            </p>
            <KeyList items={[[['⌃1', '⌃2'], 'Switch account (Alt+1, Alt+2 on Linux)']]} />
          </>
        }
        figure={<AccountsFigure />}
      >
        <p>
          Mrow reads GitHub through the <code>gh</code> command-line tool and GitLab through <code>glab</code>. Signed in
          to both? The switcher at the top of the sidebar flips between them, the way you’d switch teams in a chat app.
        </p>
        <p>
          Each account keeps its own lists. Both refresh together, so the badge and notifications cover both. In Settings
          you can turn either off, or point it at a GitHub Enterprise or self-hosted GitLab host.
        </p>
      </Chapter>
    </Page>
  )
}

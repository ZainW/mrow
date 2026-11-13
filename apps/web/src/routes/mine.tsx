import { createFileRoute } from '@tanstack/react-router'
import { Chapter, KeyList, PageHead } from '../components/Chapter'
import { InboxFigure, MergeFigure, MineFigure, StackFigure } from '../components/figures'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/mine')({
  head: () =>
    seo({
      path: '/mine',
      title: 'Your own requests in Mrow',
      description:
        'The merge and pull requests you opened, sorted by whose move it is, with the next step on each, what’s left before it merges, stacks, and an inbox for what changed.',
    }),
  component: Mine,
})

function Mine() {
  return (
    <Page>
      <PageHead title="Your own requests, by whose move it is">
        <p>
          The Mine list holds the requests you opened. It puts the ones waiting on you first and says, on every row, what
          happens next.
        </p>
      </PageHead>

      <Chapter
        id="list"
        title="Your move first"
        aside={
          <>
            <p>
              The marks above the list filter it: ready to merge, needs you, waiting, draft. Each shows how many it
              holds.
            </p>
            <KeyList items={[[['⌘3'], 'Mine']]} />
          </>
        }
        figure={<MineFigure />}
      >
        <p>
          <em>Your move</em> comes first: the requests that are ready to merge, then the ones with changes asked for, a
          failed pipeline, conflicts or open threads. Below it are <em>Waiting on reviewers</em> and <em>Drafts</em>.
        </p>
        <p>
          Each row says its next step in a line: “Approved by 2 · ready to merge”, “Pipeline failed · test:integration”,
          “Waiting on Tomás · 3 days”. A dot marks the rows where something changed since you last looked.
        </p>
      </Chapter>

      <Chapter id="merge" title="What’s left before it merges" figure={<MergeFigure />}>
        <p>
          Open one of your requests and it starts on that next step, with the button for it. <strong>Merge</strong> asks
          once more before it does anything. <strong>Merge When Ready</strong> turns on the forge’s own auto-merge, so
          GitLab or GitHub merges it once everything passes. <strong>Mark Ready</strong> marks a draft ready for review.
        </p>
        <p>
          The page ends with what’s left before it can merge: approvals, the pipeline, open threads, conflicts, and the
          request below it if it’s part of a stack. Each line that names something goes to it.
        </p>
      </Chapter>

      <Chapter
        id="stacks"
        title="Stacks, merged from the bottom up"
        aside={
          <>
            <p>
              Mrow finds stacks among your own requests, by matching branches within a project. It has to stay open while
              it merges one.
            </p>
            <KeyList items={[[['→', '←'], 'Show every part of the stacks, fold them again']]} />
          </>
        }
        figure={<StackFigure />}
      >
        <p>
          Requests that target each other’s branches show as one group, bottom first. A stack shows the parts that are on
          you and folds the rest into a count, like “3 more · 1 held, 1 waiting, 1 draft”. Click the fold or the stack’s
          name to see every part. A part that’s approved but sits on one that hasn’t merged is held, and says which one
          it waits for.
        </p>
        <p>
          <strong>Merge the Stack</strong> on a ready bottom part merges it, then moves each part above it onto the branch
          the stack lands on and merges it as soon as it’s approved and green. It stops at the first part that needs a
          person, when a step fails, or after 45 minutes of waiting on one part.
        </p>
      </Chapter>

      <Chapter
        id="inbox"
        title="What changed since you last looked"
        aside={
          <>
            <p>
              Settings → Notifications sets each kind to Notify, Inbox or Off. Banners work on macOS and on Linux,
              through the desktop’s notification daemon.
            </p>
            <KeyList items={[[['⇧⌘I'], 'Inbox']]} />
          </>
        }
        figure={<InboxFigure />}
      >
        <p>
          The bell at the top of the sidebar counts what changed: a review asked of you or asked again, and on your own
          requests an approval, changes asked for, a failed pipeline, new threads, ready to merge, merged or closed. Click
          a row to open its request, and mark one or all of them done.
        </p>
        <p>
          Mrow works these out by comparing each refresh with the one before, and remembers that one across launches. The
          very first refresh only takes note of what’s there, and your own approvals and merges don’t count. Events stay
          for 14 days.
        </p>
        <p>
          Notifications come from the same events, one per request. By default a review asked of you or asked again, changes
          asked for, a failed pipeline and ready to merge send one; approvals, new threads, and merged or closed only go in
          the inbox.
        </p>
      </Chapter>
    </Page>
  )
}

import { createFileRoute } from '@tanstack/react-router'
import { Keys, PageHead } from '../components/Chapter'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/keys')({
  head: () =>
    seo({
      path: '/keys',
      title: 'Mrow’s keyboard shortcuts',
      description: 'Every keyboard shortcut in Mrow, for its lists, reviews, tabs and Ask, on macOS and Linux.',
    }),
  component: KeysPage,
})

// apps/desktop/README.md's Keys table and COMMANDS in apps/desktop/src/ui/actions.rs.
const GROUPS: { title: string; keys: [string[], string][] }[] = [
  {
    title: 'Lists',
    keys: [
      [['⌘1', '⌘2', '⌘3', '⌘4'], 'To review, Reviewed, Mine, Done'],
      [['⌃1', '⌃2'], 'Switch account'],
      [['↑', '↓', 'J', 'K'], 'Move through the list'],
      [['↩'], 'Open the selected request in the browser'],
      [['→', '←'], 'In Mine: show every part of the stacks, fold them again'],
      [['⌘K', '⌘F'], 'Filter the list'],
      [['⌘R'], 'Refresh'],
      [['⇧⌘I'], 'Inbox'],
      [['⌃⌘S'], 'Show or hide the sidebar'],
    ],
  },
  {
    title: 'A review',
    keys: [
      [['J', 'K'], 'Next or previous part of the page; each file is one'],
      [['⌥⌘1', '⌥⌘2', '⌥⌘3'], 'The top of the page, the changes, the conversation'],
      [['⌥⌘]', '⌥⌘['], 'Next or previous review'],
      [['⇧⌘A'], 'Approve'],
      [['⌘↩'], 'Post the comment'],
      [['⌘O'], 'Open in GitLab or GitHub'],
      [['⇧⌘C'], 'Copy the link'],
    ],
  },
  {
    title: 'Tabs',
    keys: [
      [['⌘T'], 'Keep in a tab'],
      [['⌘W', '⌥⌘W'], 'Close the tab, close the others'],
      [['⌃Tab', '⇧⌘]', '⇧⌘['], 'Next or previous tab'],
    ],
  },
  {
    title: 'Ask',
    keys: [
      [['⌘L'], 'Open or close the chat'],
      [['⇧⌘L'], 'New chat'],
      [['⌥⌘L'], 'Choose the model and its effort'],
      [['esc'], 'Stop the answer'],
    ],
  },
  { title: 'Mrow', keys: [[['⌘,'], 'Settings']] },
]

function KeysPage() {
  return (
    <Page>
      <PageHead title="Every action has a key">
        <p>
          You can work through a morning’s reviews without touching the trackpad. On Linux, Ctrl stands in for ⌘ and Alt
          for ⌥, and switching accounts is Alt+1 and Alt+2. The keys below are written for the computer you’re reading
          this on.
        </p>
      </PageHead>
      <div className="chapter keys-chapter">
        <table className="shortcuts">
          <caption className="sr-only">Keyboard shortcuts</caption>
          {GROUPS.map((g) => (
            <tbody key={g.title}>
              <tr>
                <th scope="colgroup" colSpan={2}>
                  {g.title}
                </th>
              </tr>
              {g.keys.map(([keys, action]) => (
                <tr key={action}>
                  <td>
                    <Keys keys={keys} />
                  </td>
                  <td>{action}</td>
                </tr>
              ))}
            </tbody>
          ))}
        </table>
      </div>
    </Page>
  )
}

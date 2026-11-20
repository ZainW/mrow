import type { ReactNode } from 'react'
import { createFileRoute, Link } from '@tanstack/react-router'
import { Chapter, KeyList, Keys, PageHead } from '../components/Chapter'
import { DraftFigure } from '../components/figures'
import { Page } from '../components/Layout'
import { seo } from '../lib/site'

export const Route = createFileRoute('/ask')({
  head: () =>
    seo({
      path: '/ask',
      title: 'Ask your own agent about the change, in Mrow',
      description:
        'A chat beside the review with the agent you already use: Cursor, Claude Code, opencode or pi, on your own plan. It reads your clone, drafts review comments, and posts nothing until you do.',
    }),
  component: Ask,
})

const LIMITS: [string, string][] = [
  [
    'Cursor and opencode',
    'Mrow talks to them over the Agent Client Protocol and offers them no file or terminal access of its own. When one asks permission for a tool, Mrow allows reading and searching, declines the rest, and shows each one it declines in the chat.',
  ],
  ['Cursor', 'Chats run in Cursor’s ask mode, which keeps it from running commands its own settings would let it run without asking.'],
  [
    'Claude',
    'Mrow runs Claude Code itself, with only its read and search tools, and without your own settings and MCP servers, whose rules and tools could get around that. It declines reads outside your clone and shows each one in the chat.',
  ],
  [
    'opencode',
    'It runs in a folder of Mrow’s whose settings deny every tool but reading and searching, and keep .env files unread. Your clone is added as a folder it may read.',
  ],
  ['pi', 'It starts with only its read, grep, find and ls tools.'],
]

const NEEDS: [string, ReactNode][] = [
  ['Cursor', <>Cursor’s agent CLI, signed in with <code>agent login</code></>],
  ['Claude', <>Claude Code, signed in with <code>/login</code> in <code>claude</code></>],
  ['opencode', <>opencode, signed in with <code>opencode auth login</code></>],
  ['pi', <>pi, signed in to a provider with <code>pi auth</code></>],
]

function Ask() {
  return (
    <Page>
      <PageHead title="Ask your own agent about the change">
        <p>
          Press <Keys keys={['⌘L']} /> and a chat opens beside the review, with the agent you already use. Ask where
          else a function is called, what a change might break, or for a comment worth leaving. The agent reads your own
          clone; it can’t change anything, and it posts nothing.
        </p>
      </PageHead>

      <Chapter
        id="chat"
        title="Four agents, one chat"
        aside={
          <KeyList
            items={[
              [['⌘L'], 'Open or close the chat'],
              [['⇧⌘L'], 'New chat'],
              [['⌥⌘L'], 'Choose the model and its effort'],
              [['esc'], 'Stop the answer'],
            ]}
          />
        }
      >
        <p>
          Mrow runs the agent you pick: Cursor, Claude Code, opencode or pi. It runs on your own sign-in and plan, with
          the model and effort you choose, and Mrow doesn’t stand between you and the model. There’s no Mrow AI plan,
          and nothing to pay on top of what you already pay for the agent.
        </p>
        <p>
          Your first question carries the description, the
          diff and the threads, and the agent can read and search your local clone as it’s checked out. Mrow finds the
          clone from the remotes in the usual folders, or you point it at one.
        </p>
        <p>
          You get one chat per request. It stays with the agent it started on and is kept across relaunches. The first
          question also carries a short set of writing rules, so answers come back plain.
        </p>
        <p>
          You don’t have to wait for an answer to finish. Send a message while the agent is working and it stops and
          takes that as the next question; esc puts it back in the box.
        </p>
      </Chapter>

      <Chapter id="drafts" title="It drafts, you post" figure={<DraftFigure />} figureWidth="text">
        <p>
          The agent has no way to post. When you ask for review comments, or one would help, it writes each as a draft:
          on a line of a file, or on the whole request. The chat shows every draft as a card. Edit it, post it, or leave
          it where it is.
        </p>
      </Chapter>

      <Chapter
        id="limits"
        title="Read and search, nothing else"
        figureWidth="text"
        figure={
          <dl className="limits">
            {LIMITS.map(([who, how]) => (
              <div key={who}>
                <dt>{who}</dt>
                <dd>{how}</dd>
              </div>
            ))}
          </dl>
        }
      >
        <p>
          Whichever agent you use, it can read and search files in your clone. It can’t edit them or run commands. Each
          agent is held to that its own way.
        </p>
      </Chapter>

      <Chapter
        id="needs"
        title="What you need"
        aside={
          <p>
            Your question and the review go to the agent, and from it to the model it uses. What else Mrow talks to is
            on the <Link to="/privacy" className="text-link inline">Privacy</Link> page.
          </p>
        }
      >
        <ul className="needs">
          {NEEDS.map(([who, what]) => (
            <li key={who}>
              <strong>{who}</strong>: {what}
            </li>
          ))}
        </ul>
        <p>
          Mrow looks for each in the usual install places and on your login shell’s path. Cursor’s path can be set in
          Settings, and the others take a command in <code>settings.json</code>.
        </p>
      </Chapter>
    </Page>
  )
}

// Every name, repository and line of code here is made up for the illustration.

export type Provider = 'gitlab' | 'github'
export type ListId = 'review' | 'reviewed' | 'done'
export type Group = 'Today' | 'Yesterday' | 'This week' | 'Last 14 days'

export type Comment = { author: string; body: string; when: string }
export type Line = {
  kind: 'ctx' | 'add' | 'del'
  old?: number
  new?: number
  code: string
  thread?: Comment[]
}

export type Review = {
  id: string
  ref: string
  title: string
  author: string
  group: Group
  asked: string
  waiting?: string
  unread?: boolean
  rerequested?: boolean
  list: ListId
  outcome?: 'Approved' | 'Commented' | 'Merged' | 'Closed'
  source: string
  target: string
  description: string
  file: string
  diff: Line[]
}

export type Account = {
  provider: Provider
  name: string
  host: string
  handle: string
  initial: string
  noun: string
}

export const accounts: Record<Provider, Account> = {
  gitlab: {
    provider: 'gitlab',
    name: 'GitLab',
    host: 'gitlab.com',
    handle: '@wren',
    initial: 'W',
    noun: 'merge request',
  },
  github: {
    provider: 'github',
    name: 'GitHub',
    host: 'github.com',
    handle: '@wren-a',
    initial: 'W',
    noun: 'pull request',
  },
}

export const reviews: Record<Provider, Review[]> = {
  gitlab: [
    {
      id: 'gl-1',
      ref: 'northwind/billing!311',
      title: 'Round proration to the cent before tax',
      author: 'Priya Raman',
      group: 'Today',
      asked: 'Re-requested 9:40',
      unread: true,
      rerequested: true,
      list: 'review',
      source: 'priya/proration-rounding',
      target: 'main',
      description:
        'Prorated amounts were rounded after tax, so a few invoices came out a cent high. This rounds the line first and adds a test for the case from the support thread.',
      file: 'src/proration.ts',
      diff: [
        { kind: 'ctx', old: 41, new: 41, code: 'export function prorate(line: Line, days: number) {' },
        { kind: 'ctx', old: 42, new: 42, code: '  const share = days / line.periodDays' },
        { kind: 'del', old: 43, code: '  const net = line.amount * share' },
        { kind: 'del', old: 44, code: '  return Math.round(withTax(net) * 100) / 100' },
        {
          kind: 'add',
          new: 43,
          code: '  const net = Math.round(line.amount * share * 100) / 100',
          thread: [
            { author: 'Priya Raman', when: '9:38', body: 'Rounding here now, so tax sees whole cents.' },
          ],
        },
        { kind: 'add', new: 44, code: '  return withTax(net)' },
        { kind: 'ctx', old: 45, new: 45, code: '}' },
      ],
    },
    {
      id: 'gl-2',
      ref: 'northwind/infra!97',
      title: 'Move preview environments to the shared runner pool',
      author: 'Tomás Ortega',
      group: 'Today',
      asked: 'Asked 8:12',
      list: 'review',
      source: 'tomas/shared-runners',
      target: 'main',
      description:
        'Preview jobs had their own runners and sat idle most of the day. They now use the shared pool with a tag, and stop after two hours.',
      file: 'ci/preview.yml',
      diff: [
        { kind: 'ctx', old: 12, new: 12, code: 'preview:' },
        { kind: 'ctx', old: 13, new: 13, code: '  stage: deploy' },
        { kind: 'del', old: 14, code: '  tags: [preview-runner]' },
        { kind: 'add', new: 14, code: '  tags: [shared, docker]' },
        { kind: 'add', new: 15, code: "  timeout: 2h  # stop idle previews" },
        { kind: 'ctx', old: 15, new: 16, code: '  environment:' },
      ],
    },
    {
      id: 'gl-3',
      ref: 'northwind/mobile!1450',
      title: 'Ignore a second tap on the checkout button',
      author: 'June Park',
      group: 'Yesterday',
      asked: 'Asked yesterday',
      list: 'review',
      source: 'june/checkout-double-tap',
      target: 'main',
      description: 'A quick second tap could place the order twice. The button now waits for the first request.',
      file: 'app/CheckoutButton.tsx',
      diff: [
        { kind: 'ctx', old: 18, new: 18, code: 'async function onPress() {' },
        { kind: 'add', new: 19, code: '  if (pending) return' },
        { kind: 'add', new: 20, code: '  setPending(true)' },
        { kind: 'ctx', old: 19, new: 21, code: '  await placeOrder(cart)' },
        { kind: 'add', new: 22, code: '  setPending(false)' },
        { kind: 'ctx', old: 20, new: 23, code: '}' },
      ],
    },
    {
      id: 'gl-4',
      ref: 'northwind/billing!305',
      title: 'Retry webhook delivery with backoff',
      author: 'Sam Okafor',
      group: 'This week',
      asked: 'Asked Tuesday',
      waiting: '4d',
      list: 'review',
      source: 'sam/webhook-retries',
      target: 'main',
      description: 'Failed webhooks are retried five times, doubling the wait each time, before they land in the dead-letter list.',
      file: 'src/webhooks/deliver.ts',
      diff: [
        { kind: 'ctx', old: 7, new: 7, code: 'for (let attempt = 0; attempt < 5; attempt++) {' },
        { kind: 'del', old: 8, code: '  await sleep(1000)' },
        { kind: 'add', new: 8, code: '  await sleep(1000 * 2 ** attempt)' },
        { kind: 'ctx', old: 9, new: 9, code: '  if (await send(hook)) return' },
        { kind: 'ctx', old: 10, new: 10, code: '}' },
      ],
    },
    {
      id: 'gl-5',
      ref: 'northwind/docs!88',
      title: 'Document the refund states',
      author: 'Priya Raman',
      group: 'Yesterday',
      asked: 'Asked yesterday',
      list: 'reviewed',
      outcome: 'Approved',
      source: 'priya/refund-docs',
      target: 'main',
      description: 'Adds a page that walks through pending, partial and settled refunds.',
      file: 'docs/refunds.md',
      diff: [
        { kind: 'add', new: 1, code: '# Refund states' },
        { kind: 'add', new: 2, code: '' },
        { kind: 'add', new: 3, code: 'A refund starts as pending and settles within five days.' },
      ],
    },
    {
      id: 'gl-6',
      ref: 'northwind/infra!92',
      title: 'Pin Terraform to 1.9',
      author: 'Tomás Ortega',
      group: 'Last 14 days',
      asked: 'Merged last week',
      list: 'done',
      outcome: 'Merged',
      source: 'tomas/pin-terraform',
      target: 'main',
      description: 'Pins the Terraform version so plans match between laptops and CI.',
      file: '.terraform-version',
      diff: [
        { kind: 'del', old: 1, code: 'latest' },
        { kind: 'add', new: 1, code: '1.9.8' },
      ],
    },
  ],
  github: [
    {
      id: 'gh-1',
      ref: 'acme/web#482',
      title: 'Keep the filter when switching tabs',
      author: 'Lena Fischer',
      group: 'Today',
      asked: 'Asked 10:05',
      unread: true,
      list: 'review',
      source: 'lena/sticky-filter',
      target: 'main',
      description:
        'Switching from Open to Closed cleared whatever you had typed in the filter. The query now lives in the URL, so it survives tabs and reloads.',
      file: 'src/issues/IssueList.tsx',
      diff: [
        { kind: 'ctx', old: 22, new: 22, code: 'export function IssueList({ tab }: Props) {' },
        { kind: 'del', old: 23, code: "  const [query, setQuery] = useState('')" },
        {
          kind: 'add',
          new: 23,
          code: "  const [query, setQuery] = useSearchParam('q')",
          thread: [
            { author: 'Lena Fischer', when: '10:02', body: 'Same hook the settings page uses.' },
            { author: 'You', when: '10:20', body: 'Nice, does it debounce the URL writes?' },
          ],
        },
        { kind: 'ctx', old: 24, new: 24, code: '  const issues = useIssues(tab, query)' },
        { kind: 'ctx', old: 25, new: 25, code: '  return <List items={issues} />' },
      ],
    },
    {
      id: 'gh-2',
      ref: 'acme/api#1207',
      title: 'Paginate the invoices endpoint',
      author: 'Marcus Bell',
      group: 'Yesterday',
      asked: 'Re-requested yesterday',
      rerequested: true,
      list: 'review',
      source: 'marcus/invoice-pages',
      target: 'main',
      description: 'Returns 50 invoices at a time with a cursor, instead of every invoice the account has ever had.',
      file: 'api/invoices.py',
      diff: [
        { kind: 'ctx', old: 30, new: 30, code: 'def list_invoices(account, cursor=None):' },
        { kind: 'del', old: 31, code: '    return Invoice.all(account)' },
        { kind: 'add', new: 31, code: '    page = Invoice.after(account, cursor).limit(50)' },
        { kind: 'add', new: 32, code: '    return page, page.next_cursor()' },
      ],
    },
    {
      id: 'gh-3',
      ref: 'tinyco/docs#58',
      title: 'Fix broken anchors in the install guide',
      author: 'Ada Lin',
      group: 'This week',
      asked: 'Asked Monday',
      waiting: '3d',
      list: 'review',
      source: 'ada/anchors',
      target: 'main',
      description: 'Three headings were renamed last month and the links to them were not.',
      file: 'guide/install.md',
      diff: [
        { kind: 'del', old: 14, code: 'See [requirements](#before-you-start).' },
        { kind: 'add', new: 14, code: 'See [requirements](#what-you-need).' },
      ],
    },
    {
      id: 'gh-4',
      ref: 'acme/web#476',
      title: 'Use relative dates in the activity feed',
      author: 'Lena Fischer',
      group: 'This week',
      asked: 'Asked Monday',
      list: 'reviewed',
      outcome: 'Commented',
      source: 'lena/relative-dates',
      target: 'main',
      description: 'Shows "2 hours ago" instead of a full timestamp, with the exact time on hover.',
      file: 'src/feed/Entry.tsx',
      diff: [
        { kind: 'del', old: 9, code: '  <time>{entry.at.toISOString()}</time>' },
        { kind: 'add', new: 9, code: '  <time title={exact(entry.at)}>{ago(entry.at)}</time>' },
      ],
    },
    {
      id: 'gh-5',
      ref: 'acme/api#1199',
      title: 'Drop Node 18 from CI',
      author: 'Marcus Bell',
      group: 'Last 14 days',
      asked: 'Merged last week',
      list: 'done',
      outcome: 'Merged',
      source: 'marcus/node-22',
      target: 'main',
      description: 'Node 18 is past end of life. CI now runs on 22 and 24.',
      file: '.github/workflows/ci.yml',
      diff: [
        { kind: 'del', old: 11, code: '        node: [18, 20, 22]' },
        { kind: 'add', new: 11, code: '        node: [22, 24]' },
      ],
    },
  ],
}

export const listLabels: Record<ListId, string> = {
  review: 'To review',
  reviewed: 'Reviewed',
  done: 'Done',
}

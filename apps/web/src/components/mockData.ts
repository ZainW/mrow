// Every name, repository and line of code here is made up for the illustration.

export type Provider = 'gitlab' | 'github'
export type ListId = 'review' | 'reviewed' | 'mine' | 'done'
export type Group = 'Today' | 'Yesterday' | 'This week' | 'Older'
/** Where one of your own requests stands, as the Mine list sorts them. */
export type Standing = 'ready' | 'needs' | 'waiting' | 'quiet' | 'held' | 'draft' | 'merged'

export type Comment = { author: string; body: string; when: string; unseen?: boolean }
export type Line = {
  kind: 'ctx' | 'add' | 'del'
  old?: number
  new?: number
  code: string
  thread?: Comment[]
}
export type DiffFile = { path: string; lines: Line[] }
export type Reviewer = { name: string; state: 'approved' | 'pending' | 'changes' }
export type Check = { label: string; detail: string; state: 'ok' | 'bad' | 'wait' }

export type Review = {
  id: string
  /** "fieldnotes/ledger" */
  project: string
  /** "!214" or "#482" */
  number: string
  title: string
  author: string
  list: ListId
  /** How long ago: "3h", "1d". */
  age: string
  group?: Group
  /** Waiting on you for more than two days. */
  stale?: boolean
  /** The line under a review's title: who asked, or what you said. */
  reason?: string
  again?: boolean
  unread?: boolean
  draft?: boolean
  pipeline: 'passed' | 'failed' | 'running'
  source: string
  target: string
  reviewers: Reviewer[]
  description: string[]
  files: DiffFile[]
  conversation: Comment[]
  outcome?: 'Approved' | 'Commented' | 'Merged' | 'Closed'
  // Your own requests
  standing?: Standing
  step?: string
  why?: string
  stack?: string
  checks?: Check[]
  /** What Ask drafts in the chat, on a line of the first file. */
  draftLine?: number
  draftComment?: string
}

export type Account = {
  provider: Provider
  name: string
  host: string
  handle: string
  noun: string
  cli: string
}

export type InboxEvent = {
  id: string
  review: string
  what: string
  tone: 'ok' | 'bad' | 'ask'
  when: string
  fresh: boolean
}

export const accounts: Record<Provider, Account> = {
  gitlab: { provider: 'gitlab', name: 'GitLab', host: 'gitlab.com', handle: '@robin', noun: 'merge request', cli: 'glab' },
  github: { provider: 'github', name: 'GitHub', host: 'github.com', handle: '@robin-o', noun: 'pull request', cli: 'gh' },
}

export const listLabels: Record<ListId, string> = {
  review: 'To review',
  reviewed: 'Reviewed',
  mine: 'Mine',
  done: 'Done',
}

const JOB_RS: DiffFile = {
  path: 'src/export/job.rs',
  lines: [
    { kind: 'ctx', old: 12, new: 12, code: '/// Writes a ledger out as CSV.' },
    { kind: 'ctx', old: 13, new: 13, code: 'pub struct ExportJob {' },
    { kind: 'ctx', old: 14, new: 14, code: '    ledger: LedgerId,' },
    { kind: 'del', old: 15, code: '    cursor: Option<RowId>,' },
    {
      kind: 'add',
      new: 15,
      code: '    cursor: Cursor,',
      thread: [
        { author: 'Tomás Varga', when: '2 hours ago', body: 'Should the flush be optional? On a local disk this is a lot of fsyncs.' },
        { author: 'Maya Lindqvist', when: '1 hour ago', body: 'It flushes per batch now, not per row, so 200 a million.', unseen: true },
      ],
    },
    { kind: 'add', new: 16, code: '    batch: Batch,' },
    { kind: 'ctx', old: 16, new: 17, code: '}' },
  ],
}

const EXPORTS_MD: DiffFile = {
  path: 'docs/exports.md',
  lines: [
    { kind: 'ctx', old: 8, new: 8, code: '## Large ledgers' },
    { kind: 'del', old: 9, code: 'Exports over 1 GB can time out.' },
    { kind: 'add', new: 9, code: 'Exports run as a job. The API answers with its id at once.' },
  ],
}

export const reviews: Record<Provider, Review[]> = {
  gitlab: [
    {
      id: 'gl-214',
      project: 'fieldnotes/ledger',
      number: '!214',
      title: 'Stream ledger exports in batches on a read replica',
      author: 'Maya Lindqvist',
      list: 'review',
      group: 'Today',
      age: '3h',
      reason: 'Maya asked for your review',
      unread: true,
      pipeline: 'passed',
      source: 'maya/batched-exports',
      target: 'main',
      reviewers: [
        { name: 'You', state: 'pending' },
        { name: 'Tomás Varga', state: 'approved' },
      ],
      description: [
        'Ledger exports used to stream every row through one writer, so a 2 GB export held the lock for minutes and the dashboard timed out behind it.',
        'This moves the export onto its own read replica connection and writes CSV in batches of 5,000 rows. The API answers with a job id at once; the file lands in the bucket when it’s ready.',
      ],
      files: [JOB_RS, EXPORTS_MD],
      conversation: [{ author: 'Tomás Varga', when: '2 hours ago', body: 'Looks good once the flush question is settled.' }],
      draftLine: 15,
      draftComment:
        'The cursor comes from the replica, so rows written in the last second or two before a batch can be skipped. Could the job take its first cursor from the primary?',
    },
    {
      id: 'gl-88',
      project: 'fieldnotes/hooks',
      number: '!88',
      title: 'Retry webhook deliveries with jittered backoff',
      author: 'Devika Rao',
      list: 'review',
      group: 'Yesterday',
      age: '1d',
      reason: 'Devika asked again',
      again: true,
      unread: true,
      pipeline: 'running',
      source: 'devika/jitter',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Failed deliveries are retried five times, doubling the wait each time with a little jitter, before they land in the dead-letter list.'],
      files: [
        {
          path: 'src/deliver.rs',
          lines: [
            { kind: 'ctx', old: 7, new: 7, code: 'for attempt in 0..5 {' },
            { kind: 'del', old: 8, code: '    sleep(Duration::from_secs(1)).await;' },
            { kind: 'add', new: 8, code: '    sleep(backoff(attempt) + jitter()).await;' },
            { kind: 'ctx', old: 9, new: 9, code: '    if send(&hook).await.is_ok() { return Ok(()) }' },
            { kind: 'ctx', old: 10, new: 10, code: '}' },
          ],
        },
      ],
      conversation: [{ author: 'Devika Rao', when: '1 day ago', body: 'Pushed the jitter you asked for. Asking again.' }],
    },
    {
      id: 'gl-301',
      project: 'fieldnotes/billing',
      number: '!301',
      title: 'Draft: move invoice PDFs to the new renderer',
      author: 'Samuel Achterberg',
      list: 'review',
      group: 'This week',
      age: '3d',
      stale: true,
      reason: 'Samuel asked for your review',
      draft: true,
      pipeline: 'passed',
      source: 'samuel/pdf-renderer',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Invoices render through the new layout engine. Totals and tax lines are unchanged; fonts are embedded now.'],
      files: [
        {
          path: 'src/invoice/pdf.rs',
          lines: [
            { kind: 'del', old: 21, code: '    let doc = legacy::render(&invoice)?;' },
            { kind: 'add', new: 21, code: '    let doc = layout::render(&invoice, Fonts::Embedded)?;' },
          ],
        },
      ],
      conversation: [],
    },
    {
      id: 'gl-77',
      project: 'fieldnotes/hooks',
      number: '!77',
      title: 'Log the signature header on rejected webhooks',
      author: 'Devika Rao',
      list: 'reviewed',
      group: 'Yesterday',
      age: '1d',
      reason: 'You approved',
      outcome: 'Approved',
      pipeline: 'passed',
      source: 'devika/log-signature',
      target: 'main',
      reviewers: [{ name: 'You', state: 'approved' }],
      description: ['Rejected deliveries now log which signature header they carried, so a misconfigured sender is easy to spot.'],
      files: [
        {
          path: 'src/verify.rs',
          lines: [
            { kind: 'ctx', old: 30, new: 30, code: 'if !valid {' },
            { kind: 'add', new: 31, code: '    warn!(header = %sig.name(), "rejected webhook");' },
            { kind: 'ctx', old: 31, new: 32, code: '    return Err(Rejected);' },
          ],
        },
      ],
      conversation: [],
    },
    {
      id: 'gl-209',
      project: 'fieldnotes/ledger',
      number: '!209',
      title: 'Name the currency in ledger CSV headers',
      author: 'Tomás Varga',
      list: 'reviewed',
      group: 'This week',
      age: '4d',
      reason: 'You commented',
      outcome: 'Commented',
      pipeline: 'passed',
      source: 'tomas/currency-headers',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['The amount column reads "amount (EUR)" instead of "amount", so a spreadsheet of two ledgers can’t mix them up.'],
      files: [
        {
          path: 'src/export/csv.rs',
          lines: [
            { kind: 'del', old: 4, code: '    "amount",' },
            { kind: 'add', new: 4, code: '    &format!("amount ({})", ledger.currency),' },
          ],
        },
      ],
      conversation: [{ author: 'You', when: '4 days ago', body: 'Can the old header stay behind a flag for a release?' }],
    },
    // Mine
    {
      id: 'gl-231',
      project: 'fieldnotes/ledger',
      number: '!231',
      title: 'Drop the legacy export endpoint',
      author: 'You',
      list: 'mine',
      age: '12m',
      standing: 'ready',
      step: 'Approved by 2 · ready to merge',
      why: 'Nothing blocks the merge.',
      pipeline: 'passed',
      source: 'robin/drop-legacy-export',
      target: 'main',
      reviewers: [
        { name: 'Tomás Varga', state: 'approved' },
        { name: 'Samuel Achterberg', state: 'approved' },
      ],
      description: ['Clients before 4.2 were the last ones on the synchronous export. They’re gone, so the endpoint can go too.'],
      files: [
        {
          path: 'src/export/routes.rs',
          lines: [
            { kind: 'ctx', old: 33, new: 33, code: '        .route("/exports/:id", get(status))' },
            { kind: 'del', old: 34, code: '        // The old synchronous export, for clients before 4.2.' },
            { kind: 'del', old: 35, code: '        .route("/export.csv", get(legacy::download))' },
            { kind: 'ctx', old: 36, new: 34, code: '}' },
          ],
        },
      ],
      conversation: [{ author: 'Samuel Achterberg', when: '1 hour ago', body: 'Checked the access logs. Nothing has called it in a month.' }],
      checks: [
        { label: 'Approvals', detail: '2 approved', state: 'ok' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
        { label: 'Conflicts', detail: 'none', state: 'ok' },
      ],
    },
    {
      id: 'gl-240',
      project: 'fieldnotes/ledger',
      number: '!240',
      title: 'Ledger paging 1: add cursor columns',
      author: 'You',
      list: 'mine',
      age: '25m',
      standing: 'ready',
      step: 'Approved by 2 · ready to merge',
      why: 'Nothing blocks the merge. !241 moves onto main when this merges.',
      stack: 'Ledger paging',
      pipeline: 'passed',
      source: 'robin/paging-1',
      target: 'main',
      reviewers: [
        { name: 'Maya Lindqvist', state: 'approved' },
        { name: 'Samuel Achterberg', state: 'approved' },
      ],
      description: ['Cursor paging for the ledger, in parts small enough to review in one sitting. This one adds the columns.'],
      files: [
        {
          path: 'migrations/0042_cursor.sql',
          lines: [
            { kind: 'add', new: 1, code: 'ALTER TABLE entries ADD COLUMN cursor BIGINT;' },
            { kind: 'add', new: 2, code: 'CREATE INDEX entries_cursor ON entries (ledger_id, cursor);' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '2 approved', state: 'ok' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
        { label: 'Conflicts', detail: 'none', state: 'ok' },
      ],
    },
    {
      id: 'gl-241',
      project: 'fieldnotes/ledger',
      number: '!241',
      title: 'Ledger paging 2: paginate the endpoint',
      author: 'You',
      list: 'mine',
      age: '3h',
      standing: 'held',
      step: 'Approved by 2 · held until !240 merges',
      why: 'It’s ready, but it targets the branch of !240. Merge that first.',
      stack: 'Ledger paging',
      pipeline: 'passed',
      source: 'robin/paging-2',
      target: 'robin/paging-1',
      reviewers: [
        { name: 'Maya Lindqvist', state: 'approved' },
        { name: 'Samuel Achterberg', state: 'approved' },
      ],
      description: ['The entries endpoint takes a cursor and returns 500 rows at a time.'],
      files: [
        {
          path: 'src/ledger/page.rs',
          lines: [
            { kind: 'del', old: 9, code: '    let rows = entries.all(ledger).await?;' },
            { kind: 'add', new: 9, code: '    let rows = entries.after(ledger, cursor).limit(500).await?;' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '2 approved', state: 'ok' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
        { label: 'Stack', detail: 'waits for !240', state: 'wait' },
      ],
    },
    {
      id: 'gl-242',
      project: 'fieldnotes/ledger',
      number: '!242',
      title: 'Ledger paging 3: switch the client to cursors',
      author: 'You',
      list: 'mine',
      age: '1d',
      standing: 'waiting',
      step: 'Waiting on Samuel · 1 day',
      why: 'Samuel hasn’t reviewed it yet.',
      stack: 'Ledger paging',
      pipeline: 'passed',
      source: 'robin/paging-3',
      target: 'robin/paging-2',
      reviewers: [{ name: 'Samuel Achterberg', state: 'pending' }],
      description: ['The web client follows the cursor instead of counting pages.'],
      files: [
        {
          path: 'web/src/ledger.ts',
          lines: [
            { kind: 'del', old: 14, code: '  const page = await api.entries({ page: n })' },
            { kind: 'add', new: 14, code: '  const page = await api.entries({ after: cursor })' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '0 approved · waiting on Samuel Achterberg', state: 'wait' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Stack', detail: 'waits for !241', state: 'wait' },
      ],
    },
    {
      id: 'gl-243',
      project: 'fieldnotes/ledger',
      number: '!243',
      title: 'Ledger paging 4: remove offset paging',
      author: 'You',
      list: 'mine',
      age: '1d',
      standing: 'draft',
      step: 'Draft · nobody asked yet',
      why: 'It’s a draft. Mark it ready when the parts below it are in.',
      stack: 'Ledger paging',
      draft: true,
      pipeline: 'passed',
      source: 'robin/paging-4',
      target: 'robin/paging-3',
      reviewers: [],
      description: ['Deletes the offset parameters once nothing sends them.'],
      files: [
        {
          path: 'src/ledger/page.rs',
          lines: [
            { kind: 'del', old: 3, code: '    pub offset: Option<u32>,' },
            { kind: 'ctx', old: 4, new: 3, code: '    pub after: Option<Cursor>,' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: 'nobody asked yet', state: 'wait' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Stack', detail: 'waits for !242', state: 'wait' },
      ],
    },
    {
      id: 'gl-92',
      project: 'fieldnotes/hooks',
      number: '!92',
      title: 'Cache tenant settings per request',
      author: 'You',
      list: 'mine',
      age: '40m',
      standing: 'needs',
      step: 'Pipeline failed · test:integration',
      why: 'The test:integration job failed on the last push.',
      pipeline: 'failed',
      source: 'robin/tenant-cache',
      target: 'main',
      reviewers: [{ name: 'Devika Rao', state: 'pending' }],
      description: ['Tenant settings were read three times per delivery. They’re read once per request now and kept on the context.'],
      files: [
        {
          path: 'src/tenant.rs',
          lines: [
            { kind: 'del', old: 18, code: '    let settings = Settings::load(&db, tenant).await?;' },
            { kind: 'add', new: 18, code: '    let settings = ctx.settings(tenant).await?;' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '0 approved · waiting on Devika Rao', state: 'wait' },
        { label: 'Pipeline', detail: 'test:integration failed', state: 'bad' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
        { label: 'Conflicts', detail: 'none', state: 'ok' },
      ],
    },
    {
      id: 'gl-90',
      project: 'fieldnotes/hooks',
      number: '!90',
      title: 'Verify webhook signatures before queueing',
      author: 'You',
      list: 'mine',
      age: '2h',
      standing: 'needs',
      step: 'Maya asked for changes · 2 threads open',
      why: 'Maya asked for changes. Answer her two threads, push, and ask again.',
      pipeline: 'passed',
      source: 'robin/verify-early',
      target: 'main',
      reviewers: [{ name: 'Maya Lindqvist', state: 'changes' }],
      description: ['Unsigned deliveries are dropped at the door instead of after they’ve been queued.'],
      files: [
        {
          path: 'src/receive.rs',
          lines: [
            { kind: 'ctx', old: 11, new: 11, code: 'pub async fn receive(req: Request) -> Response {' },
            {
              kind: 'add',
              new: 12,
              code: '    verify(&req)?;',
              thread: [{ author: 'Maya Lindqvist', when: '2 hours ago', body: 'This returns a 500 for a bad signature. It should be a 401.' }],
            },
            { kind: 'ctx', old: 12, new: 13, code: '    queue.push(req.into()).await' },
          ],
        },
      ],
      conversation: [{ author: 'Maya Lindqvist', when: '2 hours ago', body: 'Two things inline. The approach is right.' }],
      checks: [
        { label: 'Approvals', detail: '0 approved · Maya Lindqvist asked for changes', state: 'bad' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: '2 open', state: 'bad' },
        { label: 'Conflicts', detail: 'none', state: 'ok' },
      ],
    },
    {
      id: 'gl-307',
      project: 'fieldnotes/billing',
      number: '!307',
      title: 'Stream audit log exports',
      author: 'You',
      list: 'mine',
      age: '3d',
      standing: 'quiet',
      step: 'Waiting on Tomás · 3 days',
      why: 'Tomás was asked three days ago and hasn’t said anything yet.',
      pipeline: 'passed',
      source: 'robin/audit-stream',
      target: 'main',
      reviewers: [{ name: 'Tomás Varga', state: 'pending' }],
      description: ['The audit log export uses the same batched writer as ledger exports.'],
      files: [
        {
          path: 'src/audit/export.rs',
          lines: [
            { kind: 'del', old: 5, code: '    for row in rows { out.write_row(&row)?; }' },
            { kind: 'add', new: 5, code: '    ExportJob::new(log, Batch::rows(5_000)).run(out).await?;' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '0 approved · waiting on Tomás Varga', state: 'wait' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
      ],
    },
    {
      id: 'gl-198',
      project: 'fieldnotes/ledger',
      number: '!198',
      title: 'Pin the Postgres image in CI',
      author: 'Tomás Varga',
      list: 'done',
      age: '5d',
      outcome: 'Merged',
      pipeline: 'passed',
      source: 'tomas/pin-postgres',
      target: 'main',
      reviewers: [{ name: 'You', state: 'approved' }],
      description: ['CI pulled the latest Postgres and broke on a minor release. It’s pinned to 17.2.'],
      files: [
        {
          path: '.gitlab-ci.yml',
          lines: [
            { kind: 'del', old: 3, code: '  image: postgres:latest' },
            { kind: 'add', new: 3, code: '  image: postgres:17.2' },
          ],
        },
      ],
      conversation: [],
    },
  ],
  github: [
    {
      id: 'gh-482',
      project: 'acme/web',
      number: '#482',
      title: 'Keep the filter when switching tabs',
      author: 'Lena Fischer',
      list: 'review',
      group: 'Today',
      age: '2h',
      reason: 'Lena asked for your review',
      unread: true,
      pipeline: 'passed',
      source: 'lena/sticky-filter',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: [
        'Switching from Open to Closed cleared whatever you had typed in the filter. The query now lives in the URL, so it survives tabs and reloads.',
      ],
      files: [
        {
          path: 'src/issues/IssueList.tsx',
          lines: [
            { kind: 'ctx', old: 22, new: 22, code: 'export function IssueList({ tab }: Props) {' },
            { kind: 'del', old: 23, code: "  const [query, setQuery] = useState('')" },
            {
              kind: 'add',
              new: 23,
              code: "  const [query, setQuery] = useSearchParam('q')",
              thread: [{ author: 'Lena Fischer', when: '2 hours ago', body: 'Same hook the settings page uses.' }],
            },
            { kind: 'ctx', old: 24, new: 24, code: '  const issues = useIssues(tab, query)' },
          ],
        },
      ],
      conversation: [],
      draftLine: 23,
      draftComment: 'useSearchParam writes the URL on every keystroke. Worth debouncing, or the back button fills up with half-typed queries.',
    },
    {
      id: 'gh-1207',
      project: 'acme/api',
      number: '#1207',
      title: 'Paginate the invoices endpoint',
      author: 'Marcus Bell',
      list: 'review',
      group: 'Yesterday',
      age: '1d',
      reason: 'Marcus asked again',
      again: true,
      pipeline: 'passed',
      source: 'marcus/invoice-pages',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Returns 50 invoices at a time with a cursor, instead of every invoice the account has ever had.'],
      files: [
        {
          path: 'api/invoices.py',
          lines: [
            { kind: 'ctx', old: 30, new: 30, code: 'def list_invoices(account, cursor=None):' },
            { kind: 'del', old: 31, code: '    return Invoice.all(account)' },
            { kind: 'add', new: 31, code: '    page = Invoice.after(account, cursor).limit(50)' },
            { kind: 'add', new: 32, code: '    return page, page.next_cursor()' },
          ],
        },
      ],
      conversation: [],
    },
    {
      id: 'gh-58',
      project: 'tinyco/docs',
      number: '#58',
      title: 'Fix broken anchors in the install guide',
      author: 'Ada Lin',
      list: 'review',
      group: 'This week',
      age: '3d',
      stale: true,
      reason: 'Ada asked for your review',
      pipeline: 'passed',
      source: 'ada/anchors',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Three headings were renamed last month and the links to them were not.'],
      files: [
        {
          path: 'guide/install.md',
          lines: [
            { kind: 'del', old: 14, code: 'See [requirements](#before-you-start).' },
            { kind: 'add', new: 14, code: 'See [requirements](#what-you-need).' },
          ],
        },
      ],
      conversation: [],
    },
    {
      id: 'gh-476',
      project: 'acme/web',
      number: '#476',
      title: 'Use relative dates in the activity feed',
      author: 'Lena Fischer',
      list: 'reviewed',
      group: 'This week',
      age: '4d',
      reason: 'You commented',
      outcome: 'Commented',
      pipeline: 'passed',
      source: 'lena/relative-dates',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Shows “2 hours ago” instead of a full timestamp, with the exact time on hover.'],
      files: [
        {
          path: 'src/feed/Entry.tsx',
          lines: [
            { kind: 'del', old: 9, code: '  <time>{entry.at.toISOString()}</time>' },
            { kind: 'add', new: 9, code: '  <time title={exact(entry.at)}>{ago(entry.at)}</time>' },
          ],
        },
      ],
      conversation: [{ author: 'You', when: '4 days ago', body: 'Should “yesterday” say the time too?' }],
    },
    {
      id: 'gh-490',
      project: 'acme/web',
      number: '#490',
      title: 'Show the filter in the page title',
      author: 'You',
      list: 'mine',
      age: '1h',
      standing: 'ready',
      step: 'Approved by 1 · ready to merge',
      why: 'Nothing blocks the merge.',
      pipeline: 'passed',
      source: 'robin/title-filter',
      target: 'main',
      reviewers: [{ name: 'Lena Fischer', state: 'approved' }],
      description: ['The tab title reads “Issues: is:open label:bug” so a row of tabs tells you which is which.'],
      files: [
        {
          path: 'src/issues/IssueList.tsx',
          lines: [{ kind: 'add', new: 25, code: '  useTitle(query ? `Issues: ${query}` : "Issues")' }],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '1 approved', state: 'ok' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
        { label: 'Conflicts', detail: 'none', state: 'ok' },
      ],
    },
    {
      id: 'gh-1212',
      project: 'acme/api',
      number: '#1212',
      title: 'Add an index for invoice lookups',
      author: 'You',
      list: 'mine',
      age: '5h',
      standing: 'waiting',
      step: 'Waiting on Marcus · 5 hours',
      why: 'Marcus hasn’t reviewed it yet.',
      pipeline: 'passed',
      source: 'robin/invoice-index',
      target: 'main',
      reviewers: [{ name: 'Marcus Bell', state: 'pending' }],
      description: ['Lookups by account and date scanned the whole table.'],
      files: [
        {
          path: 'migrations/0117_invoice_index.py',
          lines: [{ kind: 'add', new: 8, code: "        migrations.AddIndex('invoice', ['account', 'issued_at'])," }],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '0 approved · waiting on Marcus Bell', state: 'wait' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
      ],
    },
    {
      id: 'gh-493',
      project: 'acme/web',
      number: '#493',
      title: 'Try the new date picker on the reports page',
      author: 'You',
      list: 'mine',
      age: '2d',
      standing: 'draft',
      step: 'Draft · nobody asked yet',
      why: 'It’s a draft. Mark it ready when you want reviews.',
      draft: true,
      pipeline: 'passed',
      source: 'robin/date-picker',
      target: 'main',
      reviewers: [],
      description: ['Swaps the two date inputs for one range picker.'],
      files: [
        {
          path: 'src/reports/Range.tsx',
          lines: [
            { kind: 'del', old: 4, code: '  <DateInput value={from} /> <DateInput value={to} />' },
            { kind: 'add', new: 4, code: '  <RangePicker from={from} to={to} />' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: 'nobody asked yet', state: 'wait' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
      ],
    },
    {
      id: 'gh-1199',
      project: 'acme/api',
      number: '#1199',
      title: 'Drop Node 18 from CI',
      author: 'Marcus Bell',
      list: 'done',
      age: '6d',
      outcome: 'Merged',
      pipeline: 'passed',
      source: 'marcus/node-22',
      target: 'main',
      reviewers: [{ name: 'You', state: 'approved' }],
      description: ['Node 18 is past end of life. CI runs on 22 and 24.'],
      files: [
        {
          path: '.github/workflows/ci.yml',
          lines: [
            { kind: 'del', old: 11, code: '        node: [18, 20, 22]' },
            { kind: 'add', new: 11, code: '        node: [22, 24]' },
          ],
        },
      ],
      conversation: [],
    },
  ],
}

export const inbox: Record<Provider, InboxEvent[]> = {
  gitlab: [
    { id: 'e1', review: 'gl-231', what: 'Ready to merge', tone: 'ok', when: '12m', fresh: true },
    { id: 'e2', review: 'gl-92', what: 'Pipeline failed · test:integration', tone: 'bad', when: '40m', fresh: true },
    { id: 'e3', review: 'gl-90', what: 'Maya Lindqvist asked for changes', tone: 'bad', when: '2h', fresh: true },
    { id: 'e4', review: 'gl-214', what: 'Maya Lindqvist asked for your review', tone: 'ask', when: '3h', fresh: true },
    { id: 'e5', review: 'gl-240', what: 'Samuel Achterberg approved', tone: 'ok', when: '5h', fresh: false },
    { id: 'e6', review: 'gl-88', what: 'Devika Rao asked again', tone: 'ask', when: '1d', fresh: false },
  ],
  github: [
    { id: 'e7', review: 'gh-490', what: 'Ready to merge', tone: 'ok', when: '1h', fresh: true },
    { id: 'e8', review: 'gh-482', what: 'Lena Fischer asked for your review', tone: 'ask', when: '2h', fresh: true },
    { id: 'e9', review: 'gh-1207', what: 'Marcus Bell asked again', tone: 'ask', when: '1d', fresh: false },
  ],
}

/** "ledger!214": the short reference the rows show. */
export function shortRef(r: Review) {
  return `${r.project.split('/').pop()}${r.number}`
}

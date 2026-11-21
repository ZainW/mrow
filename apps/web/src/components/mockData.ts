// Every name, repository and line of code here is made up for the illustration.

export type Provider = 'github' | 'gitlab'
export type ListId = 'review' | 'reviewed' | 'mine' | 'done'
export type Group = 'Today' | 'Yesterday' | 'This week' | 'Older'
/** Where one of your own requests stands, as the Mine list sorts them. */
export type Standing = 'ready' | 'needs' | 'waiting' | 'quiet' | 'held' | 'draft' | 'merged'

/** `pending`: held in your review, seen by nobody else until you submit it. */
export type Comment = { author: string; body: string; when: string; unseen?: boolean; pending?: boolean }
export type Line = {
  kind: 'ctx' | 'add' | 'del'
  old?: number
  new?: number
  code: string
  thread?: Comment[]
  resolved?: boolean
}
export type DiffFile = { path: string; lines: Line[] }
export type Reviewer = { name: string; state: 'approved' | 'pending' | 'changes' }
export type Check = { label: string; detail: string; state: 'ok' | 'bad' | 'wait' }

export type Review = {
  id: string
  /** "plover/web" */
  project: string
  /** "#482" or "!214" */
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
  /** In To review, a request you asked changes on: `came` once its author pushed, `waiting` until then. */
  turn?: 'came' | 'waiting'
  /** Held out of the list: until a time, until the request changes, or muted. */
  snooze?: 'change' | 'mute' | { until: string }
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
  github: { provider: 'github', name: 'GitHub', host: 'github.com', handle: '@robin-o', noun: 'pull request', cli: 'gh' },
  gitlab: { provider: 'gitlab', name: 'GitLab', host: 'gitlab.com', handle: '@robin', noun: 'merge request', cli: 'glab' },
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
        { author: 'You', when: '', body: 'A one-line comment there would settle it for the next reader too.', pending: true },
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
    {
      kind: 'add',
      new: 9,
      code: 'Exports run as a job. The API answers with its id at once.',
      thread: [{ author: 'Tomás Varga', when: 'yesterday', body: 'Worth saying where the file ends up?' }],
      resolved: true,
    },
  ],
}

export const reviews: Record<Provider, Review[]> = {
  gitlab: [
    {
      id: 'gl-93',
      project: 'fieldnotes/ledger',
      number: '!219',
      title: 'Round ledger totals half to even',
      author: 'Tomás Varga',
      list: 'review',
      turn: 'came',
      age: '35m',
      reason: 'Tomás pushed new commits',
      unread: true,
      pipeline: 'running',
      source: 'tomas/round-half-even',
      target: 'main',
      reviewers: [{ name: 'You', state: 'changes' }],
      description: ['Totals rounded half up, so a ledger of many .5 amounts drifted upward. They round half to even now, as the bank statements do.'],
      files: [
        {
          path: 'src/ledger/total.rs',
          lines: [
            { kind: 'ctx', old: 52, new: 52, code: 'pub fn total(entries: &[Entry]) -> Money {' },
            { kind: 'del', old: 53, code: '    sum(entries).round()' },
            { kind: 'add', new: 53, code: '    sum(entries).round_half_even()' },
          ],
        },
      ],
      conversation: [
        { author: 'You', when: '2 days ago', body: 'Does this match how the CSV export rounds? The two should agree.' },
        { author: 'Tomás Varga', when: '35 minutes ago', body: 'The export calls the same function now.', unseen: true },
      ],
    },
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
    {
      id: 'gl-205',
      project: 'fieldnotes/hooks',
      number: '!85',
      title: 'Cap webhook payloads at 1 MB',
      author: 'Devika Rao',
      list: 'review',
      turn: 'waiting',
      age: '2d',
      pipeline: 'passed',
      source: 'devika/payload-cap',
      target: 'main',
      reviewers: [{ name: 'You', state: 'changes' }],
      description: ['Deliveries larger than 1 MB are refused at the door with a 413 instead of tying up a worker.'],
      files: [
        {
          path: 'src/receive.rs',
          lines: [{ kind: 'add', new: 19, code: '    if body.len() > MAX_PAYLOAD { return Err(TooLarge) }' }],
        },
      ],
      conversation: [{ author: 'You', when: '2 days ago', body: 'Check the Content-Length header first, so a big body is never read at all.' }],
    },
    {
      id: 'gl-261',
      project: 'fieldnotes/billing',
      number: '!296',
      title: 'Show the tax line before the total',
      author: 'Samuel Achterberg',
      list: 'review',
      turn: 'waiting',
      age: '3d',
      pipeline: 'passed',
      source: 'samuel/tax-before-total',
      target: 'main',
      reviewers: [{ name: 'You', state: 'changes' }],
      description: ['Invoices list the tax line above the total, so the arithmetic reads top to bottom.'],
      files: [
        {
          path: 'src/invoice/layout.rs',
          lines: [
            { kind: 'del', old: 40, code: '    rows.push(total_row(&invoice));' },
            { kind: 'add', new: 40, code: '    rows.extend([tax_row(&invoice), total_row(&invoice)]);' },
          ],
        },
      ],
      conversation: [{ author: 'You', when: '3 days ago', body: 'Zero-tax invoices would show an empty tax line. Skip it then.' }],
    },
    {
      id: 'gl-255',
      project: 'fieldnotes/ledger',
      number: '!222',
      title: 'Add an index on ledger entry dates',
      author: 'Maya Lindqvist',
      list: 'review',
      group: 'Yesterday',
      age: '1d',
      reason: 'Maya asked for your review',
      snooze: 'change',
      pipeline: 'passed',
      source: 'maya/entry-date-index',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Date-range reports scanned the whole entries table. An index on the date column turns them into a range read.'],
      files: [
        {
          path: 'migrations/0112_entry_dates.sql',
          lines: [{ kind: 'add', new: 1, code: 'CREATE INDEX entries_on_date ON entries (booked_on);' }],
        },
      ],
      conversation: [],
    },
    {
      id: 'gl-248',
      project: 'fieldnotes/hooks',
      number: '!80',
      title: 'Rename the dead-letter list',
      author: 'Devika Rao',
      list: 'review',
      group: 'This week',
      age: '4d',
      reason: 'Devika asked for your review',
      snooze: 'mute',
      pipeline: 'passed',
      source: 'devika/rename-dead-letters',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['The dead-letter list is called “undelivered” in the dashboard and in the docs.'],
      files: [
        {
          path: 'src/deliver.rs',
          lines: [
            { kind: 'del', old: 11, code: '    dead_letters.push(hook);' },
            { kind: 'add', new: 11, code: '    undelivered.push(hook);' },
          ],
        },
      ],
      conversation: [],
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
      id: 'gh-1196',
      project: 'plover/api',
      number: '#1196',
      title: 'Reject expired API tokens at the edge',
      author: 'Priya Natarajan',
      list: 'review',
      turn: 'came',
      age: '25m',
      reason: 'Priya pushed new commits',
      unread: true,
      pipeline: 'running',
      source: 'priya/expired-tokens',
      target: 'main',
      reviewers: [{ name: 'You', state: 'changes' }],
      description: ['Tokens past their expiry used to reach the handlers, which each checked it a little differently. The edge now answers 401 before any of them run.'],
      files: [
        {
          path: 'api/auth.py',
          lines: [
            { kind: 'ctx', old: 41, new: 41, code: 'def authenticate(request):' },
            { kind: 'add', new: 42, code: '    if token.expires_at <= now():' },
            { kind: 'add', new: 43, code: '        raise Unauthorized("token expired")' },
            { kind: 'ctx', old: 42, new: 44, code: '    return token.account' },
          ],
        },
      ],
      conversation: [
        { author: 'You', when: '3 days ago', body: 'The clock should come from one place, or the tests will drift.' },
        { author: 'Priya Natarajan', when: '25 minutes ago', body: 'Pushed: now() is injected, and the test freezes it.', unseen: true },
      ],
    },
    {
      id: 'gh-482',
      project: 'plover/web',
      number: '#482',
      title: 'Keep the issue filter in the URL',
      author: 'Lena Fischer',
      list: 'review',
      group: 'Today',
      age: '2h',
      reason: 'Lena asked for your review',
      unread: true,
      pipeline: 'passed',
      source: 'lena/filter-in-url',
      target: 'main',
      reviewers: [
        { name: 'You', state: 'pending' },
        { name: 'Marcus Bell', state: 'approved' },
      ],
      description: [
        'Switching from Open to Closed cleared whatever you had typed in the filter, and so did a reload. The query now lives in the URL as ?q=, so it survives both, and a filtered list can be sent as a link.',
        'useSearchParam is the hook the settings page already uses. It replaces the history entry instead of adding one, so the back button still leaves the page in one press.',
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
              thread: [
                { author: 'Marcus Bell', when: '2 hours ago', body: 'Does the filter come back when you open an issue and press back?' },
                { author: 'Lena Fischer', when: '1 hour ago', body: 'It does. The issue page links back with the same search string.', unseen: true },
                { author: 'You', when: '', body: 'Worth a test for that round trip, since it’s the one people will hit.', pending: true },
              ],
            },
            { kind: 'ctx', old: 24, new: 24, code: '  const issues = useIssues(tab, query)' },
            { kind: 'add', new: 25, code: '  const href = (t: Tab) => withSearch(`/issues/${t}`, { q: query })' },
            { kind: 'ctx', old: 25, new: 26, code: '  return (' },
          ],
        },
        {
          path: 'src/lib/useSearchParam.ts',
          lines: [
            { kind: 'ctx', old: 8, new: 8, code: 'export function useSearchParam(name: string) {' },
            { kind: 'ctx', old: 9, new: 9, code: '  const params = useSearchParams()' },
            { kind: 'del', old: 10, code: '  const set = (v: string) => navigate({ search: { [name]: v } })' },
            {
              kind: 'add',
              new: 10,
              code: '  const set = (v: string) => navigate({ search: { ...params, [name]: v || undefined }, replace: true })',
              thread: [
                { author: 'Marcus Bell', when: 'yesterday', body: 'Should an empty filter drop the parameter instead of leaving ?q= behind?' },
                { author: 'Lena Fischer', when: 'yesterday', body: 'Done. It’s left out when empty.' },
              ],
              resolved: true,
            },
          ],
        },
        {
          path: 'src/issues/IssueList.test.tsx',
          lines: [
            { kind: 'add', new: 41, code: "  it('keeps the filter across tabs', async () => {" },
            { kind: 'add', new: 42, code: "    render(<IssueList tab=\"open\" />, { url: '/issues/open?q=label:bug' })" },
            { kind: 'add', new: 43, code: "    await user.click(screen.getByRole('tab', { name: 'Closed' }))" },
            { kind: 'add', new: 44, code: "    expect(screen.getByRole('searchbox')).toHaveValue('label:bug')" },
            { kind: 'add', new: 45, code: '  })' },
          ],
        },
      ],
      conversation: [{ author: 'Marcus Bell', when: '2 hours ago', body: 'Approved. Switching tabs keeps the filter for me locally.' }],
      draftLine: 23,
      draftComment:
        'Every keystroke writes the URL, and each write runs useIssues again. Worth debouncing the fetch, or a long query sends a request per letter.',
    },
    {
      id: 'gh-1207',
      project: 'plover/api',
      number: '#1207',
      title: 'Paginate the invoices endpoint',
      author: 'Marcus Bell',
      list: 'review',
      group: 'Yesterday',
      age: '1d',
      reason: 'Marcus asked again',
      again: true,
      unread: true,
      pipeline: 'running',
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
      conversation: [{ author: 'Marcus Bell', when: '1 day ago', body: 'The cursor is opaque now, as you suggested. Asking again.' }],
    },
    {
      id: 'gh-58',
      project: 'plover/docs',
      number: '#58',
      title: 'Rewrite the install guide for the new CLI',
      author: 'Ada Lin',
      list: 'review',
      group: 'This week',
      age: '3d',
      stale: true,
      reason: 'Ada asked for your review',
      draft: true,
      pipeline: 'passed',
      source: 'ada/install-guide',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['The guide still told people to download a zip. It walks through the CLI installer now, and the old steps move to an appendix.'],
      files: [
        {
          path: 'guide/install.md',
          lines: [
            { kind: 'del', old: 14, code: 'Download the zip from the releases page and unpack it.' },
            { kind: 'add', new: 14, code: 'Run the installer, then check it with plover --version.' },
          ],
        },
      ],
      conversation: [],
    },
    {
      id: 'gh-1203',
      project: 'plover/api',
      number: '#1203',
      title: 'Return 404 for archived accounts',
      author: 'Marcus Bell',
      list: 'reviewed',
      group: 'Yesterday',
      age: '1d',
      reason: 'You approved',
      outcome: 'Approved',
      pipeline: 'passed',
      source: 'marcus/archived-404',
      target: 'main',
      reviewers: [{ name: 'You', state: 'approved' }],
      description: ['Archived accounts answered with an empty 200, so clients cached nothing as if it were real.'],
      files: [
        {
          path: 'api/accounts.py',
          lines: [
            { kind: 'ctx', old: 18, new: 18, code: '    account = Account.get(account_id)' },
            { kind: 'add', new: 19, code: '    if account.archived:' },
            { kind: 'add', new: 20, code: '        raise NotFound()' },
          ],
        },
      ],
      conversation: [],
    },
    {
      id: 'gh-476',
      project: 'plover/web',
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
      id: 'gh-1188',
      project: 'plover/web',
      number: '#471',
      title: 'Debounce the issue search box',
      author: 'Tobias Weil',
      list: 'review',
      turn: 'waiting',
      age: '2d',
      pipeline: 'passed',
      source: 'tobias/debounce-search',
      target: 'main',
      reviewers: [{ name: 'You', state: 'changes' }],
      description: ['Typing in the search box waited for a pause of 250 ms before it ran a query, instead of one per key.'],
      files: [
        {
          path: 'src/issues/SearchBox.tsx',
          lines: [
            { kind: 'del', old: 12, code: '  onChange={(e) => run(e.target.value)}' },
            { kind: 'add', new: 12, code: '  onChange={(e) => debounced(e.target.value)}' },
          ],
        },
      ],
      conversation: [{ author: 'You', when: '2 days ago', body: 'Cancel the pending call on unmount, or it fires into a page that has gone.' }],
    },
    {
      id: 'gh-1181',
      project: 'plover/docs',
      number: '#55',
      title: 'Document the export flags',
      author: 'Ada Lin',
      list: 'review',
      turn: 'waiting',
      age: '3d',
      pipeline: 'passed',
      source: 'ada/export-flags',
      target: 'main',
      reviewers: [{ name: 'You', state: 'changes' }],
      description: ['Every flag of plover export, with an example each and the default it falls back to.'],
      files: [
        {
          path: 'guide/export.md',
          lines: [{ kind: 'add', new: 18, code: '`--since <date>` limits the export to rows changed after that day.' }],
        },
      ],
      conversation: [{ author: 'You', when: '3 days ago', body: 'Which one wins when --since and --all are both given?' }],
    },
    {
      id: 'gh-1192',
      project: 'plover/api',
      number: '#1192',
      title: 'Add a rate limit to the export route',
      author: 'Marcus Bell',
      list: 'review',
      group: 'Today',
      age: '4h',
      reason: 'Marcus asked for your review',
      snooze: { until: 'tomorrow at 9:00' },
      pipeline: 'passed',
      source: 'marcus/export-limit',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Exports are capped at ten a minute per account, with a Retry-After header on the answer that says no.'],
      files: [
        {
          path: 'api/exports.py',
          lines: [{ kind: 'add', new: 14, code: '@limit("10/minute", key=account_id)' }],
        },
      ],
      conversation: [],
    },
    {
      id: 'gh-1179',
      project: 'plover/web',
      number: '#468',
      title: 'Move the avatar sizes into tokens',
      author: 'Lena Fischer',
      list: 'review',
      group: 'Yesterday',
      age: '1d',
      reason: 'Lena asked for your review',
      snooze: 'change',
      pipeline: 'passed',
      source: 'lena/avatar-tokens',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['The three avatar sizes were pixel values in four files. They are tokens now, so a resize is one edit.'],
      files: [
        {
          path: 'src/ui/Avatar.tsx',
          lines: [
            { kind: 'del', old: 6, code: '  const px = small ? 20 : 28' },
            { kind: 'add', new: 6, code: '  const px = small ? tokens.avatarSm : tokens.avatarMd' },
          ],
        },
      ],
      conversation: [],
    },
    {
      id: 'gh-1170',
      project: 'plover/docs',
      number: '#52',
      title: 'Fix the links in the changelog',
      author: 'Ada Lin',
      list: 'review',
      group: 'This week',
      age: '4d',
      reason: 'Ada asked for your review',
      snooze: 'mute',
      pipeline: 'passed',
      source: 'ada/changelog-links',
      target: 'main',
      reviewers: [{ name: 'You', state: 'pending' }],
      description: ['Release links pointed at the tag page of the wrong repository. They point at this one now.'],
      files: [
        {
          path: 'CHANGELOG.md',
          lines: [
            { kind: 'del', old: 3, code: '[0.9.1]: https://example.com/plover-old/releases/0.9.1' },
            { kind: 'add', new: 3, code: '[0.9.1]: https://example.com/plover/releases/0.9.1' },
          ],
        },
      ],
      conversation: [],
    },
    // Mine
    {
      id: 'gh-490',
      project: 'plover/web',
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
      id: 'gh-500',
      project: 'plover/web',
      number: '#500',
      title: 'Saved views 1: store views per user',
      author: 'You',
      list: 'mine',
      age: '30m',
      standing: 'ready',
      step: 'Approved by 2 · ready to merge',
      why: 'Nothing blocks the merge. #501 moves onto main when this merges.',
      stack: 'Saved views',
      pipeline: 'passed',
      source: 'robin/views-1',
      target: 'main',
      reviewers: [
        { name: 'Lena Fischer', state: 'approved' },
        { name: 'Ada Lin', state: 'approved' },
      ],
      description: ['Saved views for the issue list, in parts small enough to review in one sitting. This one adds the table and the endpoint that writes to it.'],
      files: [
        {
          path: 'migrations/0058_saved_views.sql',
          lines: [
            { kind: 'add', new: 1, code: 'CREATE TABLE saved_views (id BIGSERIAL PRIMARY KEY, user_id BIGINT, name TEXT, query TEXT);' },
            { kind: 'add', new: 2, code: 'CREATE INDEX saved_views_user ON saved_views (user_id);' },
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
      id: 'gh-501',
      project: 'plover/web',
      number: '#501',
      title: 'Saved views 2: list them in the sidebar',
      author: 'You',
      list: 'mine',
      age: '4h',
      standing: 'held',
      step: 'Approved by 2 · held until #500 merges',
      why: 'It’s ready, but it targets the branch of #500. Merge that first.',
      stack: 'Saved views',
      pipeline: 'passed',
      source: 'robin/views-2',
      target: 'robin/views-1',
      reviewers: [
        { name: 'Lena Fischer', state: 'approved' },
        { name: 'Ada Lin', state: 'approved' },
      ],
      description: ['Your saved views show under the tabs in the sidebar, newest first.'],
      files: [
        {
          path: 'src/sidebar/SavedViews.tsx',
          lines: [
            { kind: 'add', new: 12, code: '  const views = useSavedViews()' },
            { kind: 'add', new: 13, code: '  return <NavList items={views.map(toLink)} />' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '2 approved', state: 'ok' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
        { label: 'Stack', detail: 'waits for #500', state: 'wait' },
      ],
    },
    {
      id: 'gh-502',
      project: 'plover/web',
      number: '#502',
      title: 'Saved views 3: share a view by link',
      author: 'You',
      list: 'mine',
      age: '1d',
      standing: 'waiting',
      step: 'Waiting on Ada · 1 day',
      why: 'Ada hasn’t reviewed it yet.',
      stack: 'Saved views',
      pipeline: 'passed',
      source: 'robin/views-3',
      target: 'robin/views-2',
      reviewers: [{ name: 'Ada Lin', state: 'pending' }],
      description: ['Copy Link on a saved view gives a URL with its query, so someone else can open the same list.'],
      files: [
        {
          path: 'src/sidebar/SavedViews.tsx',
          lines: [
            { kind: 'del', old: 13, code: '  return <NavList items={views.map(toLink)} />' },
            { kind: 'add', new: 13, code: '  return <NavList items={views.map(toLink)} menu={copyLink} />' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '0 approved · waiting on Ada Lin', state: 'wait' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Stack', detail: 'waits for #501', state: 'wait' },
      ],
    },
    {
      id: 'gh-503',
      project: 'plover/web',
      number: '#503',
      title: 'Saved views 4: remove the old bookmarks',
      author: 'You',
      list: 'mine',
      age: '1d',
      standing: 'draft',
      step: 'Draft · nobody asked yet',
      why: 'It’s a draft. Mark it ready when the parts below it are in.',
      stack: 'Saved views',
      draft: true,
      pipeline: 'passed',
      source: 'robin/views-4',
      target: 'robin/views-3',
      reviewers: [],
      description: ['Deletes bookmarks once saved views cover everything they did.'],
      files: [
        {
          path: 'src/issues/Bookmarks.tsx',
          lines: [
            { kind: 'del', old: 1, code: "import { useBookmarks } from '../lib/bookmarks'" },
            { kind: 'del', old: 2, code: 'export function Bookmarks() {' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: 'nobody asked yet', state: 'wait' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Stack', detail: 'waits for #502', state: 'wait' },
      ],
    },
    {
      id: 'gh-1215',
      project: 'plover/api',
      number: '#1215',
      title: 'Stream the invoices CSV',
      author: 'You',
      list: 'mine',
      age: '45m',
      standing: 'needs',
      step: 'Pipeline failed · test (3.12)',
      why: 'The test (3.12) job failed on the last push.',
      pipeline: 'failed',
      source: 'robin/stream-csv',
      target: 'main',
      reviewers: [{ name: 'Marcus Bell', state: 'pending' }],
      description: ['The CSV export built the whole file in memory first. It streams rows as they’re read now.'],
      files: [
        {
          path: 'api/export.py',
          lines: [
            { kind: 'del', old: 22, code: '    return Response(render_csv(rows))' },
            { kind: 'add', new: 22, code: '    return StreamingResponse(csv_rows(rows), media_type="text/csv")' },
          ],
        },
      ],
      conversation: [],
      checks: [
        { label: 'Approvals', detail: '0 approved · waiting on Marcus Bell', state: 'wait' },
        { label: 'Pipeline', detail: 'test (3.12) failed', state: 'bad' },
        { label: 'Threads', detail: 'none open', state: 'ok' },
        { label: 'Conflicts', detail: 'none', state: 'ok' },
      ],
    },
    {
      id: 'gh-1210',
      project: 'plover/api',
      number: '#1210',
      title: 'Rate-limit the public search endpoint',
      author: 'You',
      list: 'mine',
      age: '2h',
      standing: 'needs',
      step: 'Jonah asked for changes · 2 threads open',
      why: 'Jonah asked for changes. Answer his two threads, push, and ask again.',
      pipeline: 'passed',
      source: 'robin/search-limit',
      target: 'main',
      reviewers: [{ name: 'Jonah Okafor', state: 'changes' }],
      description: ['Search answers 60 requests a minute per caller, and a 429 with Retry-After past that.'],
      files: [
        {
          path: 'api/search.py',
          lines: [
            { kind: 'ctx', old: 9, new: 9, code: 'def search(request):' },
            {
              kind: 'add',
              new: 10,
              code: '    limiter.check(request.remote_addr, per_minute=60)',
              thread: [
                {
                  author: 'Jonah Okafor',
                  when: '2 hours ago',
                  body: 'Everyone behind one office proxy shares this limit. Could it key on the token when there is one?',
                },
              ],
            },
            {
              kind: 'add',
              new: 11,
              code: '    results = index.query(request.args["q"])',
              thread: [{ author: 'Jonah Okafor', when: '2 hours ago', body: 'A missing q is a 500 here. It should be a 400.' }],
            },
          ],
        },
      ],
      conversation: [{ author: 'Jonah Okafor', when: '2 hours ago', body: 'Two things inline. The limit itself looks right.' }],
      checks: [
        { label: 'Approvals', detail: '0 approved · Jonah Okafor asked for changes', state: 'bad' },
        { label: 'Pipeline', detail: 'passed', state: 'ok' },
        { label: 'Threads', detail: '2 open', state: 'bad' },
        { label: 'Conflicts', detail: 'none', state: 'ok' },
      ],
    },
    {
      id: 'gh-1212',
      project: 'plover/api',
      number: '#1212',
      title: 'Add an index for invoice lookups',
      author: 'You',
      list: 'mine',
      age: '3d',
      standing: 'quiet',
      step: 'Waiting on Marcus · 3 days',
      why: 'Marcus was asked three days ago and hasn’t said anything yet.',
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
      id: 'gh-1199',
      project: 'plover/api',
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
  github: [
    { id: 'e13', review: 'gh-1196', what: 'Priya Natarajan pushed new commits', tone: 'ask', when: '25m', fresh: true },
    { id: 'e7', review: 'gh-500', what: 'Ready to merge', tone: 'ok', when: '30m', fresh: true },
    { id: 'e8', review: 'gh-1215', what: 'Pipeline failed · test (3.12)', tone: 'bad', when: '45m', fresh: true },
    { id: 'e9', review: 'gh-1210', what: 'Jonah Okafor asked for changes', tone: 'bad', when: '2h', fresh: true },
    { id: 'e10', review: 'gh-482', what: 'Lena Fischer asked for your review', tone: 'ask', when: '2h', fresh: true },
    { id: 'e11', review: 'gh-490', what: 'Lena Fischer approved', tone: 'ok', when: '5h', fresh: false },
    { id: 'e12', review: 'gh-1207', what: 'Marcus Bell asked again', tone: 'ask', when: '1d', fresh: false },
  ],
  gitlab: [
    { id: 'e1', review: 'gl-231', what: 'Ready to merge', tone: 'ok', when: '12m', fresh: true },
    { id: 'e14', review: 'gl-93', what: 'Tomás Varga pushed new commits', tone: 'ask', when: '35m', fresh: true },
    { id: 'e2', review: 'gl-92', what: 'Pipeline failed · test:integration', tone: 'bad', when: '40m', fresh: true },
    { id: 'e3', review: 'gl-90', what: 'Maya Lindqvist asked for changes', tone: 'bad', when: '2h', fresh: true },
    { id: 'e4', review: 'gl-214', what: 'Maya Lindqvist asked for your review', tone: 'ask', when: '3h', fresh: true },
    { id: 'e5', review: 'gl-240', what: 'Samuel Achterberg approved', tone: 'ok', when: '5h', fresh: false },
    { id: 'e6', review: 'gl-88', what: 'Devika Rao asked again', tone: 'ask', when: '1d', fresh: false },
  ],
}

/** "web#482": the short reference the rows show. */
export function shortRef(r: Review) {
  return `${r.project.split('/').pop()}${r.number}`
}

export type ChangeItem = { text: string; pr?: number }
export type Release = { version: string; date: string; anchor: string; sections: { title: string; items: ChangeItem[] }[] }

const HEADING = /^##\s+v?(\d+\.\d+\.\d+\S*)\s+\((\d{4}-\d{2}-\d{2})\)\s*$/
const PR_SUFFIX = /\s*\(#(\d+)\)\s*$/

/** The repo's CHANGELOG.md (git-cliff output), newest release first. Pure, so vite.config.ts can use it too. */
export function parseChangelog(md: string): Release[] {
  const releases: Release[] = []
  let release: Release | undefined
  let section: Release['sections'][number] | undefined
  for (const line of md.split(/\r?\n/)) {
    const heading = HEADING.exec(line)
    const [, version, date] = heading ?? []
    if (version && date) {
      release = { version, date, anchor: `v${version.replaceAll('.', '-')}`, sections: [] }
      releases.push(release)
      section = undefined
    } else if (release && line.startsWith('### ')) {
      section = { title: line.slice(4).trim(), items: [] }
      release.sections.push(section)
    } else if (section && line.startsWith('- ')) {
      const body = line.slice(2).trim()
      const pr = PR_SUFFIX.exec(body)
      section.items.push(pr ? { text: body.slice(0, pr.index), pr: Number(pr[1]) } : { text: body })
    }
  }
  return releases
}

/** A release date as "November 16, 2026". */
export function formatDate(iso: string) {
  return new Date(`${iso}T00:00:00Z`).toLocaleDateString('en-US', { year: 'numeric', month: 'long', day: 'numeric', timeZone: 'UTC' })
}

const xml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;')

/** An RSS 2.0 feed with one entry per release. */
export function changelogRss(releases: Release[], { site, repo }: { site: string; repo: string }): string {
  const items = releases.map((r) => {
    const body = r.sections
      .map(
        (s) =>
          `<h3>${xml(s.title)}</h3><ul>${s.items
            .map((i) => `<li>${xml(i.text)}${i.pr ? ` (<a href="${repo}/pull/${i.pr}">#${i.pr}</a>)` : ''}</li>`)
            .join('')}</ul>`,
      )
      .join('')
    return `    <item>
      <title>Mrow ${r.version}</title>
      <link>${site}/changelog#${r.anchor}</link>
      <guid isPermaLink="true">${site}/changelog#${r.anchor}</guid>
      <pubDate>${new Date(`${r.date}T00:00:00Z`).toUTCString()}</pubDate>
      <description>${xml(body)}</description>
    </item>`
  })
  const built = releases[0] ? new Date(`${releases[0].date}T00:00:00Z`).toUTCString() : new Date(0).toUTCString()
  return `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
  <channel>
    <title>Mrow changelog</title>
    <link>${site}/changelog</link>
    <description>What changed in each release of Mrow.</description>
    <language>en</language>
    <lastBuildDate>${built}</lastBuildDate>
    <atom:link href="${site}/changelog/rss.xml" rel="self" type="application/rss+xml" />
${items.join('\n')}
  </channel>
</rss>
`
}

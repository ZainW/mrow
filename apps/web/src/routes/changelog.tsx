import { createFileRoute } from '@tanstack/react-router'
import { PageHead } from '../components/Chapter'
import { RssIcon } from '../components/icons'
import { Page } from '../components/Layout'
import { formatDate } from '../lib/changelog'
import { LOG } from '../lib/releases'
import { RELEASES, REPO, seo } from '../lib/site'

export const Route = createFileRoute('/changelog')({
  head: () =>
    seo({
      path: '/changelog',
      title: 'Changelog: Mrow',
      description: 'What changed in each release of Mrow, newest first.',
    }),
  component: ChangelogPage,
})

function ChangelogPage() {
  return (
    <Page>
      <PageHead title="Changelog">
        <p>
          What changed in each release of Mrow, newest first. Release builds update themselves in the background.{' '}
          <a href="/changelog/rss.xml" className="text-link inline">
            <RssIcon className="log-rss" /> Subscribe by RSS
          </a>
        </p>
      </PageHead>
      <div className="chapter log-chapter">
        <div className="log-list">
          {LOG.map((r) => (
            <section key={r.version} id={r.anchor} className="log-release" aria-labelledby={`${r.anchor}-title`}>
              <header>
                <h2 id={`${r.anchor}-title`}>
                  <a href={`${RELEASES}/tag/v${r.version}`} className="log-version">
                    {r.version}
                  </a>
                </h2>
                <time dateTime={r.date}>{formatDate(r.date)}</time>
              </header>
              {r.sections.map((s) => (
                <div key={s.title}>
                  <h3>{s.title}</h3>
                  <ul>
                    {s.items.map((i) => (
                      <li key={i.text}>
                        {i.text}
                        {i.pr && (
                          <>
                            {' '}
                            <a href={`${REPO}/pull/${i.pr}`} className="log-pr">
                              #{i.pr}
                            </a>
                          </>
                        )}
                      </li>
                    ))}
                  </ul>
                </div>
              ))}
            </section>
          ))}
        </div>
      </div>
    </Page>
  )
}

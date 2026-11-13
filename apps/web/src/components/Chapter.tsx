import type { ReactNode } from 'react'
import { useKeyLabel } from '../lib/keys'

/** A page's heading and opening paragraph, in the reading column. */
export function PageHead({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header className="chapter page-head">
      <h1>{title}</h1>
      {children && <div className="prose lede-prose">{children}</div>}
    </header>
  )
}

/** A reading column with optional margin notes and a wider figure. */
export function Chapter({
  id,
  title,
  children,
  aside,
  figure,
  figureWidth = 'wide',
}: {
  id: string
  title: string
  children: ReactNode
  aside?: ReactNode
  figure?: ReactNode
  figureWidth?: 'wide' | 'text'
}) {
  return (
    <section id={id} className="chapter" aria-labelledby={`${id}-title`}>
      <h2 id={`${id}-title`}>{title}</h2>
      <div className="prose">{children}</div>
      {aside && <aside className="note">{aside}</aside>}
      {figure && <div className={`plate plate-${figureWidth}`}>{figure}</div>}
    </section>
  )
}

/** Shortcuts, written with Mac glyphs and shown the Linux way on Linux. */
export function Keys({ keys }: { keys: string[] }) {
  const label = useKeyLabel()
  return (
    <span className="keys">
      {keys.map((k) => (
        <kbd key={k}>{label(k)}</kbd>
      ))}
    </span>
  )
}

/** Keys and what they do, for a margin note. */
export function KeyList({ items }: { items: [string[], string][] }) {
  return (
    <dl className="note-keylist">
      {items.map(([keys, action]) => (
        <div key={action}>
          <dt>
            <Keys keys={keys} />
          </dt>
          <dd>{action}</dd>
        </div>
      ))}
    </dl>
  )
}

import type { ReactNode } from 'react'

// A deliberately tiny highlighter for the illustrative diffs: enough to show
// keywords, strings, numbers and comments in the app's soft code colours.
const KEYWORDS = new Set([
  'const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'await', 'async',
  'export', 'import', 'from', 'new', 'throw', 'try', 'catch', 'type', 'interface', 'true',
  'false', 'null', 'undefined', 'of', 'in', 'def', 'class', 'with', 'as',
])

const TOKEN =
  /(\/\/.*$|#.*$)|('(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*"|`(?:[^`\\]|\\.)*`)|(\b\d[\d_.]*\b)|([A-Za-z_$][\w$]*)/g

export function highlight(code: string): ReactNode[] {
  const out: ReactNode[] = []
  let last = 0
  let key = 0
  for (const m of code.matchAll(TOKEN)) {
    const start = m.index
    if (start > last) out.push(code.slice(last, start))
    const [text, comment, str, num, word] = m
    if (comment) out.push(<span key={key++} className="tk-comment">{comment}</span>)
    else if (str) out.push(<span key={key++} className="tk-string">{str}</span>)
    else if (num) out.push(<span key={key++} className="tk-number">{num}</span>)
    else if (word && KEYWORDS.has(word)) out.push(<span key={key++} className="tk-keyword">{word}</span>)
    else if (word && /^[A-Z]/.test(word)) out.push(<span key={key++} className="tk-type">{word}</span>)
    else out.push(text)
    last = start + text.length
  }
  if (last < code.length) out.push(code.slice(last))
  return out
}

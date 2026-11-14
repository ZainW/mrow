import { useLocalBuild } from './platform'

const MODS = new Set(['⌃', '⌥', '⇧', '⌘'])

/** A shortcut written with Mac glyphs, as Linux writes it: "⇧⌘A" is "Ctrl+Shift+A" (`off_mac` in the app). */
export function offMac(mac: string) {
  // The account switch is the one key that moves: Ctrl+1 is "To review" off macOS.
  if (mac === '⌃1' || mac === '⌃2') return `Alt+${mac.slice(1)}`
  if (mac === 'esc') return 'Esc'
  let out = ''
  const chars = [...mac]
  for (let i = 0; i < chars.length; i++) {
    const c = chars[i] ?? ''
    if (!MODS.has(c)) {
      out += c === '↩' ? 'Enter' : c
      continue
    }
    let ctrl = false
    let alt = false
    let shift = false
    while (MODS.has(chars[i] ?? '')) {
      const g = chars[i]
      if (g === '⌃' || g === '⌘') ctrl = true
      else if (g === '⌥') alt = true
      else shift = true
      i++
    }
    out += (ctrl ? 'Ctrl+' : '') + (alt ? 'Alt+' : '') + (shift ? 'Shift+' : '')
    const k = chars[i]
    if (k !== undefined) out += k === '↩' ? 'Enter' : k.toUpperCase()
  }
  return out
}

/** Shortcuts for the visitor's computer: Mac glyphs, unless it's Linux. */
export function useKeyLabel() {
  const linux = useLocalBuild()?.startsWith('linux')
  return (mac: string) => (linux ? offMac(mac) : mac)
}

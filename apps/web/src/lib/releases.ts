import changelog from '../../../../CHANGELOG.md?raw'
import { parseChangelog } from './changelog'

/** Every release, newest first, from the repo's CHANGELOG.md. */
export const LOG = parseChangelog(changelog)
const [latest] = LOG
if (!latest) throw new Error('CHANGELOG.md has no releases')
export const LATEST = latest

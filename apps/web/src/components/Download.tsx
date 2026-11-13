import { Link } from '@tanstack/react-router'
import { useLocalBuild } from '../lib/platform'
import { BUILD_STEPS, DOWNLOADS, INSTALL, RELEASES, VERSION, downloadUrl } from '../lib/site'
import type { Build } from '../lib/site'
import { CopyButton } from './CopyButton'

/**
 * The download itself, for the visitor's computer once we know it: the disk image on a Mac, the
 * install command on Linux, and a link to every download otherwise. Each takes the same space, so
 * nothing moves when the server's neutral version is swapped after hydration.
 */
export function GetMrow() {
  const local = useLocalBuild()
  const archive = DOWNLOADS.find((d) => d.build === local)
  return (
    <div className="get">
      <div className="get-main">
        {local === 'macos-aarch64' ? (
          <a className="btn btn-big" href={downloadUrl('mrow-macos-aarch64.dmg')}>
            <DownloadIcon /> Download for macOS
          </a>
        ) : local && archive ? (
          <div className="oneline">
            <code aria-label="Install command">{INSTALL}</code>
            <CopyButton text={INSTALL} />
          </div>
        ) : (
          <Link className="btn btn-big" to="/download">
            <DownloadIcon /> Download
          </Link>
        )}
      </div>
      <p className="get-note">
        {local === 'macos-aarch64' ? (
          <>
            Version {VERSION}, for Apple silicon on macOS 15 or later.{' '}
            <Link className="text-link inline" to="/download">
              Other downloads
            </Link>
          </>
        ) : local && archive ? (
          <>
            Paste it in a terminal to install version {VERSION} for Linux {archive.arch}, or get{' '}
            <a className="text-link inline" href={downloadUrl(archive.file)}>
              the archive
            </a>
            .{' '}
            <Link className="text-link inline" to="/download">
              All downloads
            </Link>
          </>
        ) : (
          <>Version {VERSION}, free, for macOS 15 or later on Apple silicon and for Linux on x86_64 or arm64.</>
        )}
      </p>
    </div>
  )
}

/** Every build, with the visitor's own marked. */
export function DownloadList() {
  const local = useLocalBuild()
  return (
    <ul className="downloads">
      {DOWNLOADS.map((d) => (
        <DownloadRow key={d.build} {...d} local={d.build === local} />
      ))}
    </ul>
  )
}

function DownloadRow({ system, arch, file, how, local }: { build: Build; system: string; arch: string; file: string; how: string; local: boolean }) {
  return (
    <li>
      <a className={local ? 'download is-local' : 'download'} href={downloadUrl(file)} aria-describedby={local ? 'local-note' : undefined}>
        <span className="download-system">
          {system} <span className="download-arch">{arch}</span>
          {local && (
            <span className="download-local" id="local-note">
              Your computer
            </span>
          )}
        </span>
        <span className="download-how">{how}</span>
        <span className="download-file">{file}</span>
        <span className="download-go">
          <DownloadIcon /> Download
        </span>
      </a>
    </li>
  )
}

export function InstallTerminal() {
  return (
    <div className="terminal">
      <div className="terminal-head">
        <span>Terminal</span>
        <CopyButton text={INSTALL} />
      </div>
      <pre className="command">
        <code>{INSTALL}</code>
      </pre>
    </div>
  )
}

export function BuildTerminal() {
  return (
    <div className="terminal">
      <div className="terminal-head">
        <span>Terminal</span>
        <CopyButton text={BUILD_STEPS.join('\n')} label="Copy all" />
      </div>
      <ol className="steps">
        {BUILD_STEPS.map((s) => (
          <li key={s}>
            <code>{s}</code>
          </li>
        ))}
      </ol>
    </div>
  )
}

export function ReleasesLink({ children = 'releases page' }: { children?: string }) {
  return (
    <a className="text-link inline" href={RELEASES}>
      {children}
    </a>
  )
}

function DownloadIcon() {
  return (
    <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" className="dl-icon">
      <path d="M8 2.5v8M4.5 7.5 8 11l3.5-3.5M3 13.5h10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

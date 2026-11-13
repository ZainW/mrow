import { useEffect, useState } from 'react'
import type { Build } from './site'

/**
 * The build that fits the visitor's computer, worked out after hydration so the server render stays
 * neutral. Undefined on phones, Windows, or when we can't tell; an Intel Mac reads as Apple silicon.
 */
export function useLocalBuild() {
  const [build, setBuild] = useState<Build>()
  useEffect(() => {
    const nav = navigator as Navigator & {
      userAgentData?: {
        platform: string
        mobile?: boolean
        getHighEntropyValues?: (hints: string[]) => Promise<{ architecture?: string }>
      }
    }
    const ua = navigator.userAgent
    const platform = nav.userAgentData?.platform ?? navigator.platform
    // An iPad asks for the desktop site and says it's a Mac; it has a touch screen, a Mac doesn't.
    if (/android|iphone|ipad/i.test(ua) || nav.userAgentData?.mobile || (/mac/i.test(platform) && navigator.maxTouchPoints > 1)) return
    if (/mac/i.test(platform)) {
      setBuild('macos-aarch64')
    } else if (/linux/i.test(platform)) {
      setBuild(/aarch64|arm/i.test(platform + ua) ? 'linux-aarch64' : 'linux-x86_64')
      nav.userAgentData?.getHighEntropyValues?.(['architecture']).then(
        ({ architecture }) => {
          if (architecture) setBuild(architecture === 'arm' ? 'linux-aarch64' : 'linux-x86_64')
        },
        () => {},
      )
    }
  }, [])
  return build
}

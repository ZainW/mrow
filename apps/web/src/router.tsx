import { createRouter } from '@tanstack/react-router'
import { routeTree } from './routeTree.gen'

export function getRouter() {
  return createRouter({
    routeTree,
    // A new page starts at the top and Back returns to where you were, both without scrolling there.
    scrollRestoration: true,
    scrollRestorationBehavior: 'instant',
    defaultPreload: 'intent',
  })
}

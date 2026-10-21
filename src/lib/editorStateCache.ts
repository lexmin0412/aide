import type { EditorState } from "@codemirror/state"

/**
 * Parked CodeMirror states for open tabs, keyed by theme + content revision +
 * path. While a tab is open, its parked state is authoritative: the editor no
 * longer mirrors every keystroke into React state, so `tab.content` can lag
 * behind the live document. Reusing the parked state on tab switch therefore
 * preserves edits, undo history and scroll position.
 *
 * The cache is bounded (LRU, 16 entries) so long editing sessions do not retain
 * one full document per ever-opened file.
 */
interface CacheEntry {
  path: string
  state: EditorState
}

const MAX_ENTRIES = 16
const cache = new Map<string, CacheEntry>()

export function stateCacheKey(path: string, isDark: boolean, rev = 0): string {
  return `${isDark ? "d" : "l"}:${rev}:${path}`
}

/** Look up a parked state, refreshing its LRU position. */
export function getCachedState(key: string): EditorState | undefined {
  const entry = cache.get(key)
  if (!entry) return undefined
  cache.delete(key)
  cache.set(key, entry)
  return entry.state
}

/**
 * Most recently used parked state for a path, ignoring the theme/revision part
 * of the key. Used when a dirty tab is re-mounted after a theme switch: the live
 * document in the parked state is newer than the `tab.content` snapshot.
 */
export function getLatestStateForPath(path: string): EditorState | undefined {
  let match: EditorState | undefined
  for (const entry of cache.values()) {
    if (entry.path === path) match = entry.state
  }
  return match
}

export function setCachedState(key: string, path: string, state: EditorState) {
  cache.delete(key)
  cache.set(key, { path, state })
  while (cache.size > MAX_ENTRIES) {
    const oldest = cache.keys().next().value
    if (oldest === undefined) break
    cache.delete(oldest)
  }
}

/** Drop parked states for a closed/deleted path (and optionally its subtree). */
export function evictEditorState(path: string, options?: { subtree?: boolean }) {
  for (const [key, entry] of cache) {
    if (entry.path === path || (options?.subtree && entry.path.startsWith(path + "/"))) {
      cache.delete(key)
    }
  }
}

/**
 * Derived search state shared by the command palette and the registry dialog.
 *
 * Debounced searches settle asynchronously, so results and the "searching"
 * indicator are derived from the current query plus the last settled response
 * instead of being written to state synchronously inside an effect.
 */
export const MIN_SEARCH_LENGTH = 2

export interface SettledSearch<T> {
  /** Normalized query the results belong to. */
  query: string
  results: T[]
}

export function normalizeSearchQuery(query: string): string {
  return query.trim()
}

export function shouldSearch(query: string): boolean {
  return normalizeSearchQuery(query).length >= MIN_SEARCH_LENGTH
}

/**
 * Results to render for the current query: nothing until the query is long
 * enough and a response for exactly this query has settled.
 */
export function searchResults<T>(query: string, settled: SettledSearch<T>): T[] {
  if (!shouldSearch(query)) return []
  return settled.query === normalizeSearchQuery(query) ? settled.results : []
}

/** True while the current (searchable) query has no settled response yet. */
export function isSearchPending(query: string, settled: SettledSearch<unknown>): boolean {
  if (!shouldSearch(query)) return false
  return settled.query !== normalizeSearchQuery(query)
}

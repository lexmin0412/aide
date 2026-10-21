import type { McpDrift } from "../types"

/** Cleanup results split into updated vs skipped tool counts. */
export interface DriftCleanupSummary {
  updated: number
  skipped: number
}

/** Drift entries to render in the banner, plus an overflow count. */
export interface DriftBannerView {
  tools: McpDrift[]
  hiddenCount: number
  total: number
}

/** A tool contributes drift when either side of the config differs. */
export function toolHasDrift(tool: McpDrift): boolean {
  return tool.missing.length > 0 || tool.leftover.length > 0
}

/** True when any tool config differs from the central server list. */
export function hasMcpDrift(drift: McpDrift[]): boolean {
  return drift.some(toolHasDrift)
}

/**
 * Pick the drift entries worth showing in the banner. The list is capped so a
 * large number of affected tools cannot push the server grid off screen.
 */
export function selectDriftTools(drift: McpDrift[], limit = 3): DriftBannerView {
  const affected = drift.filter(toolHasDrift)
  const max = Math.max(0, limit)
  return {
    tools: affected.slice(0, max),
    hiddenCount: Math.max(0, affected.length - max),
    total: affected.length,
  }
}

/** Split per-tool cleanup results into updated vs skipped counts. */
export function summarizeCleanup(
  results: { skipped: boolean }[],
): DriftCleanupSummary {
  const skipped = results.filter((r) => r.skipped).length
  return { updated: results.length - skipped, skipped }
}

import type { RestoreResult, SnapshotMeta } from "../types"
import { normalizeTimestamp } from "./health"

/** Label used when the user creates a snapshot without typing one. */
export const DEFAULT_SNAPSHOT_LABEL = "manual"

/** Most recent snapshots first, capped for the list view (newest 5). */
export function latestSnapshots(snapshots: SnapshotMeta[], limit = 5): SnapshotMeta[] {
  const max = Math.max(0, limit)
  return [...snapshots].sort((a, b) => b.created_at - a.created_at).slice(0, max)
}

/**
 * Localized "created at" label. Accepts unix seconds or milliseconds; returns
 * an empty string for missing timestamps. Tests pass an explicit locale/time
 * zone so assertions stay deterministic.
 */
export function formatSnapshotTime(createdAt: number, locale?: string, timeZone?: string): string {
  const ms = normalizeTimestamp(createdAt)
  if (!ms) return ""
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone,
  }).format(new Date(ms))
}

export interface RestoreSummary {
  restoredFiles: number
  skillDrift: number
  hasDrift: boolean
  safetySnapshotId: string
}

/** Condense a restore result for the toast/summary line. */
export function summarizeRestoreResult(result: RestoreResult): RestoreSummary {
  return {
    restoredFiles: result.restored_files,
    skillDrift: result.skill_drift,
    hasDrift: result.skill_drift > 0,
    safetySnapshotId: result.safety_snapshot.id,
  }
}

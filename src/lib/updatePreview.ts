import type { SkillUpdatePreview } from "../types"

/** Condensed counts for the update-review dialog. */
export interface UpdatePreviewSummary {
  added: number
  changed: number
  removed: number
  total: number
  hasChanges: boolean
}

/** Summarize a skill update preview (pure; used by the review dialog). */
export function summarizeUpdatePreview(preview: SkillUpdatePreview): UpdatePreviewSummary {
  const added = preview.added.length
  const changed = preview.changed.length
  const removed = preview.removed.length
  return {
    added,
    changed,
    removed,
    total: added + changed + removed,
    hasChanges: added + changed + removed > 0,
  }
}

/** Short commit label (7 chars), or null when the backend has no commit. */
export function shortCommit(commit: string | null | undefined): string | null {
  if (!commit) return null
  return commit.slice(0, 7)
}

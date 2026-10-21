/**
 * Persistence helpers for the FileTree's expanded-directory state.
 */
export function treeExpandedStorageKey(rootPath: string): string {
  return `filetree-expanded-${rootPath}`
}

/**
 * Parse a persisted expanded-directory list. Corrupt or non-array values are
 * treated as "nothing expanded" instead of throwing.
 */
export function parseExpandedDirs(raw: string | null): string[] {
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((entry): entry is string => typeof entry === "string")
  } catch {
    // Corrupt localStorage value (user edited it or an older format): ignore.
    return []
  }
}

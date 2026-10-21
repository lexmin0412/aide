import type { ProjectConfig, ProjectSyncResult, ProjectToolStatus } from "../types"

/**
 * Pure selectors/builders for the project profile dialog. Kept free of React and
 * Tauri so the picker rules (defaults, filtering, sync summary) are unit-tested.
 */

/** Tools that can host a project-level skills directory. */
export function projectCapableTools(tools: ProjectToolStatus[]): ProjectToolStatus[] {
  return tools.filter((tool) => tool.project_skills.trim().length > 0)
}

/** Detected tools first, then alphabetical, for the tools picker. */
export function sortProjectTools(tools: ProjectToolStatus[]): ProjectToolStatus[] {
  return [...tools].sort((a, b) => {
    if (a.detected_globally !== b.detected_globally) return a.detected_globally ? -1 : 1
    return a.name.localeCompare(b.name)
  })
}

/**
 * Default tool selection for a project without a saved profile: pre-check the
 * tools that are already detected globally (they are the ones a user expects to
 * manage), and never include tools with no project skills directory.
 */
export function defaultSelectedTools(tools: ProjectToolStatus[]): string[] {
  return projectCapableTools(tools)
    .filter((tool) => tool.detected_globally)
    .map((tool) => tool.key)
}

/** Case-insensitive skill picker filter over name/display name. */
export function filterProjectSkills<T extends { name: string; display_name: string }>(
  skills: T[],
  query: string
): T[] {
  const q = query.trim().toLowerCase()
  if (!q) return skills
  return skills.filter(
    (skill) => skill.name.toLowerCase().includes(q) || skill.display_name.toLowerCase().includes(q)
  )
}

/** Immutable add/remove used by every checkbox list. */
export function toggleSelection(values: string[], value: string): string[] {
  return values.includes(value) ? values.filter((entry) => entry !== value) : [...values, value]
}

/** Config payload for "save & apply" and "clear profile". */
export function buildProjectConfig(
  skills: string[],
  mcp: string[],
  tools: string[],
  version = 1
): ProjectConfig {
  return { version, skills: [...skills], mcp: [...mcp], tools: [...tools] }
}

export interface ProjectSyncSummary {
  linked: number
  removed: number
  toolsWritten: number
  removedMcpEntries: number
  warnings: number
  errors: number
  hasIssues: boolean
}

/** Condense a sync result for the toast and the compact result list. */
export function summarizeSyncResult(result: ProjectSyncResult): ProjectSyncSummary {
  return {
    linked: result.skills.linked,
    removed: result.skills.removed,
    toolsWritten: result.mcp.tools_written,
    removedMcpEntries: result.mcp.removed,
    warnings: result.warnings.length,
    errors: result.errors.length,
    hasIssues: result.errors.length > 0,
  }
}

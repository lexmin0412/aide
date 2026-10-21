import type { BundleImportResult, BundlePreview } from "../types"
import { formatBytes } from "./health"

/**
 * Pure selectors for the environment-bundle section. Kept free of React/Tauri
 * so the confirmation dialog counts and summaries are unit-tested.
 */

export interface BundlePreviewSummary {
  newSkills: number
  conflictingSkills: number
  newMcp: number
  conflictingMcp: number
  newProfiles: number
  conflictingProfiles: number
  totalNew: number
  totalConflicts: number
  redactedCount: number
  warningCount: number
  hasConflicts: boolean
  /** True when the bundle carries no skills, MCP servers or profiles at all. */
  isEmpty: boolean
}

export function summarizeBundlePreview(preview: BundlePreview): BundlePreviewSummary {
  const newSkills = preview.new_skills.length
  const conflictingSkills = preview.conflicting_skills.length
  const newMcp = preview.new_mcp_servers.length
  const conflictingMcp = preview.conflicting_mcp_servers.length
  const newProfiles = preview.new_profiles.length
  const conflictingProfiles = preview.conflicting_profiles.length
  const totalNew = newSkills + newMcp + newProfiles
  const totalConflicts = conflictingSkills + conflictingMcp + conflictingProfiles
  const hasContent =
    preview.skill_count > 0 ||
    newMcp + conflictingMcp > 0 ||
    newProfiles + conflictingProfiles > 0
  return {
    newSkills,
    conflictingSkills,
    newMcp,
    conflictingMcp,
    newProfiles,
    conflictingProfiles,
    totalNew,
    totalConflicts,
    redactedCount: preview.redacted_secrets.length,
    warningCount: preview.warnings.length,
    hasConflicts: totalConflicts > 0,
    isEmpty: !hasContent,
  }
}

export type BundleCategory = "skills" | "mcp" | "profiles"

export interface BundleCategorySummary {
  category: BundleCategory
  newCount: number
  conflictingCount: number
}

/** New vs conflicting counts per category, in display order. */
export function describeConflicts(preview: BundlePreview): BundleCategorySummary[] {
  return [
    {
      category: "skills",
      newCount: preview.new_skills.length,
      conflictingCount: preview.conflicting_skills.length,
    },
    {
      category: "mcp",
      newCount: preview.new_mcp_servers.length,
      conflictingCount: preview.conflicting_mcp_servers.length,
    },
    {
      category: "profiles",
      newCount: preview.new_profiles.length,
      conflictingCount: preview.conflicting_profiles.length,
    },
  ]
}

export interface BundleImportSummary {
  added: number
  replaced: number
  total: number
  safetySnapshot: string
  warningCount: number
}

export function summarizeBundleImport(result: BundleImportResult): BundleImportSummary {
  const added = result.skills_added.length + result.mcp_added.length + result.profiles_added.length
  const replaced =
    result.skills_replaced.length + result.mcp_replaced.length + result.profiles_replaced.length
  return {
    added,
    replaced,
    total: added + replaced,
    safetySnapshot: result.safety_snapshot,
    warningCount: result.warnings.length,
  }
}

/** Bundle sizes use the same human-readable units as the audit log. */
export function formatBundleBytes(bytes: number): string {
  return formatBytes(bytes)
}

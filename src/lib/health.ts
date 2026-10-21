import type { DoctorReport, DoctorToolReport, McpDrift } from "../types"
import { toolHasDrift } from "./mcpDrift"

/**
 * Pure selectors for the Health page. Kept free of React/Tauri so the page
 * logic (what counts as an issue, which tools need attention) is unit-tested.
 */

/** Tool statuses that do not require user action. */
const HEALTHY_TOOL_STATUSES = new Set(["synced", "compatible", "not_installed"])

export function countToolStatuses(tools: DoctorToolReport[]): Record<string, number> {
  const counts: Record<string, number> = {}
  for (const tool of tools) {
    counts[tool.status] = (counts[tool.status] ?? 0) + 1
  }
  return counts
}

/** Tools whose skill link is stale or needs a merge. */
export function toolsNeedingAttention(tools: DoctorToolReport[]): DoctorToolReport[] {
  return tools.filter((tool) => !HEALTHY_TOOL_STATUSES.has(tool.status))
}

/** MCP drift entries that a cleanup run would rewrite. */
export function driftToolsNeedingCleanup(drift: McpDrift[]): McpDrift[] {
  return drift.filter(toolHasDrift)
}

/** Total number of independently actionable issues in a report. */
export function countReportIssues(report: DoctorReport): number {
  let issues = report.skills.broken.length
  if (report.skills.updates_available > 0) issues += report.skills.updates_available
  if (report.skills.update_error) issues += 1
  issues += toolsNeedingAttention(report.tools).length
  issues += driftToolsNeedingCleanup(report.mcp.drift).length
  issues += report.mcp.errors.length
  issues += report.models.missing_targets.length
  issues += report.models.errors.length
  issues += report.configs.length
  return issues
}

export function isReportHealthy(report: DoctorReport): boolean {
  return countReportIssues(report) === 0
}

/**
 * Normalize the report timestamp to milliseconds. The backend may report unix
 * seconds (contract) or milliseconds; both are accepted.
 */
export function normalizeTimestamp(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 0
  return value < 1e12 ? value * 1000 : value
}

/** Human-readable byte size for the audit log line. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B"
  const units = ["B", "KB", "MB", "GB"]
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  const rounded = unit === 0 ? String(Math.round(value)) : value.toFixed(value >= 10 ? 0 : 1)
  return `${rounded} ${units[unit]}`
}

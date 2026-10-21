import { describe, expect, it } from "vitest"
import {
  describeConflicts,
  formatBundleBytes,
  summarizeBundleImport,
  summarizeBundlePreview,
} from "@/lib/bundles"
import type { BundleImportResult, BundlePreview } from "@/types"

function preview(overrides: Partial<BundlePreview> = {}): BundlePreview {
  return {
    format_version: 1,
    created_at: 1_700_000_000,
    aide_version: "0.3.0",
    includes_secrets: false,
    skill_count: 2,
    new_skills: ["alpha"],
    conflicting_skills: ["beta"],
    new_mcp_servers: ["server-a"],
    conflicting_mcp_servers: [],
    new_profiles: [],
    conflicting_profiles: ["p1"],
    redacted_secrets: ["mcp:server-a/env/API_KEY"],
    warnings: ["hash mismatch"],
    ...overrides,
  }
}

function importResult(overrides: Partial<BundleImportResult> = {}): BundleImportResult {
  return {
    skills_added: ["a"],
    skills_replaced: ["b"],
    mcp_added: ["s1", "s2"],
    mcp_replaced: [],
    profiles_added: [],
    profiles_replaced: ["p1"],
    safety_snapshot: "20240101-120000-abcd",
    warnings: ["kept existing"],
    ...overrides,
  }
}

describe("summarizeBundlePreview", () => {
  it("counts new vs conflicting entries across categories", () => {
    const summary = summarizeBundlePreview(preview())
    expect(summary).toEqual({
      newSkills: 1,
      conflictingSkills: 1,
      newMcp: 1,
      conflictingMcp: 0,
      newProfiles: 0,
      conflictingProfiles: 1,
      totalNew: 2,
      totalConflicts: 2,
      redactedCount: 1,
      warningCount: 1,
      hasConflicts: true,
      isEmpty: false,
    })
  })

  it("marks an empty bundle", () => {
    const summary = summarizeBundlePreview(
      preview({
        skill_count: 0,
        new_skills: [],
        conflicting_skills: [],
        new_mcp_servers: [],
        conflicting_mcp_servers: [],
        new_profiles: [],
        conflicting_profiles: [],
        redacted_secrets: [],
        warnings: [],
      }),
    )
    expect(summary.isEmpty).toBe(true)
    expect(summary.hasConflicts).toBe(false)
    expect(summary.warningCount).toBe(0)
  })

  it("treats an MCP-only bundle as non-empty", () => {
    const summary = summarizeBundlePreview(
      preview({
        skill_count: 0,
        new_skills: [],
        conflicting_skills: [],
        new_profiles: [],
        conflicting_profiles: [],
      }),
    )
    expect(summary.isEmpty).toBe(false)
  })
})

describe("describeConflicts", () => {
  it("returns one row per category in display order", () => {
    expect(describeConflicts(preview())).toEqual([
      { category: "skills", newCount: 1, conflictingCount: 1 },
      { category: "mcp", newCount: 1, conflictingCount: 0 },
      { category: "profiles", newCount: 0, conflictingCount: 1 },
    ])
  })
})

describe("summarizeBundleImport", () => {
  it("sums added and replaced entries and keeps the safety snapshot id", () => {
    const summary = summarizeBundleImport(importResult())
    expect(summary).toEqual({
      added: 3,
      replaced: 2,
      total: 5,
      safetySnapshot: "20240101-120000-abcd",
      warningCount: 1,
    })
  })
})

describe("formatBundleBytes", () => {
  it("reuses the shared byte formatter", () => {
    expect(formatBundleBytes(0)).toBe("0 B")
    expect(formatBundleBytes(1536)).toBe("1.5 KB")
  })
})

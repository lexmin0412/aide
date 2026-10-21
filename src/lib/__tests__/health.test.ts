import { describe, expect, it } from "vitest"
import {
  countReportIssues,
  countToolStatuses,
  driftToolsNeedingCleanup,
  formatBytes,
  isReportHealthy,
  normalizeTimestamp,
  toolsNeedingAttention,
} from "@/lib/health"
import type { DoctorReport, DoctorToolReport } from "@/types"

function tool(overrides: Partial<DoctorToolReport> & { key: string }): DoctorToolReport {
  return {
    name: overrides.key,
    status: "synced",
    global_skills: `~/${overrides.key}/skills`,
    ...overrides,
  }
}

function report(overrides: Partial<DoctorReport> = {}): DoctorReport {
  return {
    generated_at: 1_700_000_000,
    skills: { total: 3, linked: 3, broken: [], updates_available: 0, update_error: null },
    tools: [tool({ key: "claude_code" }), tool({ key: "cursor", status: "compatible" })],
    mcp: { servers: 2, disabled: 0, drift: [], errors: [] },
    models: { profiles: 1, missing_targets: [], errors: [] },
    configs: [],
    agent: { enabled: true, binary_available: true },
    audit: { path: "~/.aide/audit.jsonl", size_bytes: 1024 },
    ...overrides,
  }
}

describe("countReportIssues / isReportHealthy", () => {
  it("reports a clean environment as healthy", () => {
    expect(countReportIssues(report())).toBe(0)
    expect(isReportHealthy(report())).toBe(true)
  })

  it("sums every issue category", () => {
    const dirty = report({
      skills: { total: 3, linked: 2, broken: ["a", "b"], updates_available: 3, update_error: "boom" },
      tools: [tool({ key: "cursor", status: "ready" })],
      mcp: {
        servers: 2,
        disabled: 0,
        drift: [{ tool_key: "cursor", tool_name: "Cursor", missing: ["srv"], leftover: [] }],
        errors: ["bad config"],
      },
      models: { profiles: 1, missing_targets: ["claude_code"], errors: ["sync failed"] },
      configs: [{ path: "~/.aide/mcp.json", error: "invalid JSON" }],
    })
    // 2 broken + 3 updates + 1 update error + 1 tool + 1 drift + 1 mcp error
    // + 1 model target + 1 model error + 1 config = 12
    expect(countReportIssues(dirty)).toBe(12)
    expect(isReportHealthy(dirty)).toBe(false)
  })

  it("ignores the agent access choice and audit size", () => {
    const disabledAgent = report({ agent: { enabled: false, binary_available: true } })
    expect(countReportIssues(disabledAgent)).toBe(0)
  })
})

describe("toolsNeedingAttention", () => {
  it("ignores synced, compatible and undetected tools", () => {
    const tools = [
      tool({ key: "a" }),
      tool({ key: "b", status: "compatible" }),
      tool({ key: "c", status: "not_installed" }),
    ]
    expect(toolsNeedingAttention(tools)).toEqual([])
  })

  it("returns tools with pending merges or pending sync", () => {
    const tools = [
      tool({ key: "a", status: "ready" }),
      tool({ key: "b", status: "has_content" }),
      tool({ key: "c", status: "synced" }),
    ]
    expect(toolsNeedingAttention(tools).map((entry) => entry.key)).toEqual(["a", "b"])
  })
})

describe("countToolStatuses", () => {
  it("counts tools per status token", () => {
    const tools = [
      tool({ key: "a" }),
      tool({ key: "b" }),
      tool({ key: "c", status: "ready" }),
      tool({ key: "d", status: "not_installed" }),
    ]
    expect(countToolStatuses(tools)).toEqual({ synced: 2, ready: 1, not_installed: 1 })
  })
})

describe("driftToolsNeedingCleanup", () => {
  it("keeps only tools whose config differs from the central list", () => {
    const drift = [
      { tool_key: "a", tool_name: "A", missing: [], leftover: [] },
      { tool_key: "b", tool_name: "B", missing: ["srv"], leftover: [] },
      { tool_key: "c", tool_name: "C", missing: [], leftover: ["old"] },
    ]
    expect(driftToolsNeedingCleanup(drift).map((entry) => entry.tool_key)).toEqual(["b", "c"])
  })
})

describe("normalizeTimestamp", () => {
  it("treats small values as unix seconds and large as milliseconds", () => {
    expect(normalizeTimestamp(1_700_000_000)).toBe(1_700_000_000_000)
    expect(normalizeTimestamp(1_700_000_000_000)).toBe(1_700_000_000_000)
    expect(normalizeTimestamp(0)).toBe(0)
    expect(normalizeTimestamp(Number.NaN)).toBe(0)
  })
})

describe("formatBytes", () => {
  it("formats byte sizes with sensible units", () => {
    expect(formatBytes(0)).toBe("0 B")
    expect(formatBytes(-1)).toBe("0 B")
    expect(formatBytes(999)).toBe("999 B")
    expect(formatBytes(1536)).toBe("1.5 KB")
    expect(formatBytes(10 * 1024 * 1024)).toBe("10 MB")
  })
})

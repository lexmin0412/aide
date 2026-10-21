import { describe, expect, it } from "vitest"
import {
  buildProjectConfig,
  defaultSelectedTools,
  filterProjectSkills,
  projectCapableTools,
  sortProjectTools,
  summarizeSyncResult,
  toggleSelection,
} from "@/lib/projectProfile"
import type { ProjectSyncResult, ProjectToolStatus } from "@/types"

function tool(overrides: Partial<ProjectToolStatus> & { key: string }): ProjectToolStatus {
  return {
    name: overrides.key,
    project_skills: `.${overrides.key}/skills`,
    detected_globally: false,
    supports_mcp: false,
    ...overrides,
  }
}

describe("projectCapableTools / defaultSelectedTools", () => {
  const tools = [
    tool({ key: "detected", detected_globally: true }),
    tool({ key: "not-detected" }),
    tool({ key: "native", project_skills: "", detected_globally: true }),
    tool({ key: "detected-2", detected_globally: true }),
  ]

  it("only keeps tools with a non-empty project skills directory", () => {
    expect(projectCapableTools(tools).map((entry) => entry.key)).toEqual(["detected", "not-detected", "detected-2"])
  })

  it("pre-checks detected tools that support project skills", () => {
    expect(defaultSelectedTools(tools)).toEqual(["detected", "detected-2"])
  })
})

describe("sortProjectTools", () => {
  it("puts detected tools first, then sorts by name", () => {
    const tools = [
      tool({ key: "z", name: "Zed" }),
      tool({ key: "a", name: "Alpha", detected_globally: true }),
      tool({ key: "m", name: "Mid" }),
      tool({ key: "b", name: "Beta", detected_globally: true }),
    ]
    expect(sortProjectTools(tools).map((entry) => entry.name)).toEqual(["Alpha", "Beta", "Mid", "Zed"])
  })
})

describe("filterProjectSkills", () => {
  const skills = [
    { name: "rust-tooling", display_name: "Rust Tooling" },
    { name: "markdown-preview", display_name: "Markdown Preview" },
  ]

  it("matches name and display name case-insensitively", () => {
    expect(filterProjectSkills(skills, "RUST").map((entry) => entry.name)).toEqual(["rust-tooling"])
    expect(filterProjectSkills(skills, "preview").map((entry) => entry.name)).toEqual(["markdown-preview"])
  })

  it("keeps the full list for an empty query", () => {
    expect(filterProjectSkills(skills, "   ")).toEqual(skills)
  })
})

describe("toggleSelection", () => {
  it("adds new values and removes existing ones without mutating", () => {
    const initial = ["a"]
    expect(toggleSelection(initial, "b")).toEqual(["a", "b"])
    expect(toggleSelection(initial, "a")).toEqual([])
    expect(initial).toEqual(["a"])
  })
})

describe("buildProjectConfig", () => {
  it("copies selections and uses version 1 by default", () => {
    const skills = ["a"]
    const config = buildProjectConfig(skills, ["m"], ["t"])
    expect(config).toEqual({ version: 1, skills: ["a"], mcp: ["m"], tools: ["t"] })
    expect(config.skills).not.toBe(skills)
  })
})

describe("summarizeSyncResult", () => {
  it("counts link/tool changes and flags errors", () => {
    const result: ProjectSyncResult = {
      skills: { linked: 3, removed: 1, missing: ["gone"] },
      mcp: { tools_written: 2, removed: 1, unsupported: ["x"] },
      warnings: ["w1", "w2"],
      errors: ["boom"],
    }
    expect(summarizeSyncResult(result)).toEqual({
      linked: 3,
      removed: 1,
      toolsWritten: 2,
      removedMcpEntries: 1,
      warnings: 2,
      errors: 1,
      hasIssues: true,
    })
  })

  it("marks a result without errors as issue-free", () => {
    const result: ProjectSyncResult = {
      skills: { linked: 0, removed: 0, missing: [] },
      mcp: { tools_written: 0, removed: 0, unsupported: [] },
      warnings: [],
      errors: [],
    }
    expect(summarizeSyncResult(result).hasIssues).toBe(false)
  })
})

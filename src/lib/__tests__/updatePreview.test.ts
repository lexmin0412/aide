import { describe, expect, it } from "vitest"
import { shortCommit, summarizeUpdatePreview } from "@/lib/updatePreview"
import type { SkillUpdatePreview } from "@/types"

function preview(overrides: Partial<SkillUpdatePreview> = {}): SkillUpdatePreview {
  return {
    skill: "demo",
    id: "owner/repo/demo",
    installed_commit: "aaaaaaaaaaaaaaaa",
    latest_commit: "bbbbbbbbbbbbbbbb",
    added: [],
    removed: [],
    changed: [],
    local_modified: false,
    ...overrides,
  }
}

describe("summarizeUpdatePreview", () => {
  it("counts added/changed/removed files", () => {
    const summary = summarizeUpdatePreview(
      preview({ added: ["a.md"], changed: ["b.md", "c.md"], removed: ["d.md"] }),
    )
    expect(summary).toEqual({ added: 1, changed: 2, removed: 1, total: 4, hasChanges: true })
  })

  it("flags a preview without changes", () => {
    expect(summarizeUpdatePreview(preview()).hasChanges).toBe(false)
    expect(summarizeUpdatePreview(preview()).total).toBe(0)
  })
})

describe("shortCommit", () => {
  it("returns the 7-character prefix or null", () => {
    expect(shortCommit("abcdef123456")).toBe("abcdef1")
    expect(shortCommit("")).toBeNull()
    expect(shortCommit(null)).toBeNull()
    expect(shortCommit(undefined)).toBeNull()
  })
})

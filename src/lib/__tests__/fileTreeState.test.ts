import { describe, expect, it } from "vitest"
import { parseExpandedDirs, treeExpandedStorageKey } from "@/lib/fileTreeState"

describe("fileTreeState", () => {
  it("parses a persisted list and keeps only string entries", () => {
    expect(parseExpandedDirs('["/a","/b"]')).toEqual(["/a", "/b"])
    expect(parseExpandedDirs('["/a", 42, null]')).toEqual(["/a"])
  })

  it("treats missing, corrupt and non-array values as nothing expanded", () => {
    expect(parseExpandedDirs(null)).toEqual([])
    expect(parseExpandedDirs("")).toEqual([])
    expect(parseExpandedDirs("{not json")).toEqual([])
    expect(parseExpandedDirs('{"a":1}')).toEqual([])
  })

  it("namespaces the storage key by tree root", () => {
    expect(treeExpandedStorageKey("/skills/a")).toBe("filetree-expanded-/skills/a")
  })
})

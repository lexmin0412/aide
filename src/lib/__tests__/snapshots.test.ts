import { describe, expect, it } from "vitest"
import {
  DEFAULT_SNAPSHOT_LABEL,
  formatSnapshotTime,
  latestSnapshots,
  summarizeRestoreResult,
} from "@/lib/snapshots"
import type { RestoreResult, SnapshotMeta } from "@/types"

function snapshot(overrides: Partial<SnapshotMeta> & { id: string }): SnapshotMeta {
  return {
    created_at: 1_700_000_000,
    label: overrides.id,
    files: [],
    skill_count: 0,
    size_bytes: 0,
    ...overrides,
  }
}

describe("latestSnapshots", () => {
  it("sorts newest first and caps the list", () => {
    const snapshots = [
      snapshot({ id: "old", created_at: 100 }),
      snapshot({ id: "newest", created_at: 300 }),
      snapshot({ id: "middle", created_at: 200 }),
    ]
    expect(latestSnapshots(snapshots, 2).map((entry) => entry.id)).toEqual(["newest", "middle"])
  })

  it("does not mutate the input and accepts a zero limit", () => {
    const snapshots = [snapshot({ id: "a", created_at: 1 }), snapshot({ id: "b", created_at: 2 })]
    latestSnapshots(snapshots, 1)
    expect(snapshots.map((entry) => entry.id)).toEqual(["a", "b"])
    expect(latestSnapshots(snapshots, 0)).toEqual([])
  })
})

describe("formatSnapshotTime", () => {
  it("formats unix seconds and milliseconds identically in UTC", () => {
    const seconds = formatSnapshotTime(1_700_000_000, "en-US", "UTC")
    const millis = formatSnapshotTime(1_700_000_000_000, "en-US", "UTC")
    expect(seconds).toContain("Nov 14, 2023")
    expect(millis).toBe(seconds)
  })

  it("returns an empty string for missing timestamps", () => {
    expect(formatSnapshotTime(0)).toBe("")
    expect(formatSnapshotTime(Number.NaN)).toBe("")
  })
})

describe("summarizeRestoreResult", () => {
  it("counts restored files and flags skill drift", () => {
    const result: RestoreResult = {
      restored_files: 7,
      skill_drift: 2,
      safety_snapshot: snapshot({ id: "safety" }),
    }
    expect(summarizeRestoreResult(result)).toEqual({
      restoredFiles: 7,
      skillDrift: 2,
      hasDrift: true,
      safetySnapshotId: "safety",
    })
  })

  it("treats zero drift as issue-free", () => {
    const result: RestoreResult = {
      restored_files: 0,
      skill_drift: 0,
      safety_snapshot: snapshot({ id: "safety" }),
    }
    expect(summarizeRestoreResult(result).hasDrift).toBe(false)
  })
})

describe("DEFAULT_SNAPSHOT_LABEL", () => {
  it("is the documented manual default", () => {
    expect(DEFAULT_SNAPSHOT_LABEL).toBe("manual")
  })
})

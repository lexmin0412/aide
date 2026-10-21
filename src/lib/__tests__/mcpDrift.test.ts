import { describe, expect, it } from "vitest";

import type { McpDrift } from "@/types";
import {
  hasMcpDrift,
  selectDriftTools,
  summarizeCleanup,
} from "@/lib/mcpDrift";

const drift = (
  name: string,
  missing: string[] = [],
  leftover: string[] = [],
): McpDrift => ({
  tool_key: name.toLowerCase().replace(/\s+/g, "_"),
  tool_name: name,
  missing,
  leftover,
});

describe("mcpDrift", () => {
  it("detects drift only when a tool has missing or leftover servers", () => {
    expect(hasMcpDrift([])).toBe(false);
    expect(hasMcpDrift([drift("Claude Code")])).toBe(false);
    expect(hasMcpDrift([drift("Claude Code", ["filesystem"])])).toBe(true);
    expect(hasMcpDrift([drift("Codex"), drift("Cursor", [], ["old"])])).toBe(
      true,
    );
  });

  it("caps visible tools and reports the hidden remainder", () => {
    const all = [
      drift("A", ["one"]),
      drift("Clean"),
      drift("B", [], ["gone"]),
      drift("C", ["two"]),
      drift("D", ["three"]),
    ];

    const view = selectDriftTools(all);
    expect(view.tools.map((t) => t.tool_name)).toEqual(["A", "B", "C"]);
    expect(view.hiddenCount).toBe(1);
    expect(view.total).toBe(4);

    const limited = selectDriftTools(all, 2);
    expect(limited.tools).toHaveLength(2);
    expect(limited.hiddenCount).toBe(2);

    const none = selectDriftTools(all, 0);
    expect(none.tools).toHaveLength(0);
    expect(none.hiddenCount).toBe(4);
  });

  it("counts updated and skipped cleanup results", () => {
    expect(summarizeCleanup([])).toEqual({ updated: 0, skipped: 0 });
    expect(
      summarizeCleanup([
        { skipped: false },
        { skipped: true },
        { skipped: false },
      ]),
    ).toEqual({ updated: 2, skipped: 1 });
  });
});

import { describe, expect, it } from "vitest"
import {
  isSearchPending,
  normalizeSearchQuery,
  searchResults,
  shouldSearch,
  type SettledSearch,
} from "@/lib/searchState"

const settled = (query: string, results: string[]): SettledSearch<string> => ({ query, results })

describe("searchState", () => {
  it("normalizes and gates queries by the minimum length", () => {
    expect(normalizeSearchQuery("  markdown  ")).toBe("markdown")
    expect(shouldSearch(" a ")).toBe(false)
    expect(shouldSearch(" ab ")).toBe(true)
  })

  it("only exposes results settled for the current query", () => {
    expect(searchResults("md", settled("md", ["a"]))).toEqual(["a"])
    expect(searchResults("mdx", settled("md", ["a"]))).toEqual([])
    expect(searchResults("m", settled("m", ["a"]))).toEqual([])
  })

  it("reports pending only for searchable queries without settled results", () => {
    expect(isSearchPending("md", settled("md", []))).toBe(false)
    expect(isSearchPending("mdx", settled("md", []))).toBe(true)
    expect(isSearchPending("m", settled("", []))).toBe(false)
  })
})

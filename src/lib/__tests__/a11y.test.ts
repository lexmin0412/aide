import { describe, expect, it } from "vitest"
import { isActivationKey } from "@/lib/a11y"

describe("isActivationKey", () => {
  it("accepts Enter and Space (including the legacy Spacebar name)", () => {
    expect(isActivationKey("Enter")).toBe(true)
    expect(isActivationKey(" ")).toBe(true)
    expect(isActivationKey("Spacebar")).toBe(true)
  })

  it("rejects navigation and modifier keys", () => {
    expect(isActivationKey("ArrowDown")).toBe(false)
    expect(isActivationKey("Escape")).toBe(false)
    expect(isActivationKey("a")).toBe(false)
  })
})

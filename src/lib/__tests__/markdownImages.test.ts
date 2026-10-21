import { describe, expect, it, vi } from "vitest"
import { renderMarkdown } from "@/lib/markdown"

describe("renderMarkdown resolveImageSrc", () => {
  it("rewrites relative image sources through the option", () => {
    const resolveImageSrc = vi.fn((src: string) => `asset://localhost${src}`)
    const html = renderMarkdown("![preview](./img.png)", { resolveImageSrc })
    expect(resolveImageSrc).toHaveBeenCalledWith("./img.png")
    expect(html).toContain('<img alt="preview" src="asset://localhost./img.png" />')
  })

  it("keeps the raw source when no option is provided", () => {
    expect(renderMarkdown("![preview](./img.png)")).toContain('<img alt="preview" src="./img.png" />')
  })

  it("does not rewrite remote or anchor image sources by itself", () => {
    const resolveImageSrc = vi.fn((src: string) => src)
    const html = renderMarkdown("![logo](https://example.com/logo.png)", { resolveImageSrc })
    expect(resolveImageSrc).toHaveBeenCalledWith("https://example.com/logo.png")
    expect(html).toContain('src="https://example.com/logo.png"')
  })

  it("escapes the resolved source and inline alt text", () => {
    const html = renderMarkdown('![a"b](./x.png)', {
      resolveImageSrc: () => "/tmp/a&b.png",
    })
    expect(html).toContain('alt="a&quot;b"')
    expect(html).toContain('src="/tmp/a&amp;b.png"')
  })

  it("applies the option inside tables and blockquotes", () => {
    const table = renderMarkdown(["| icon |", "| --- |", "| ![i](./i.png) |"].join("\n"), {
      resolveImageSrc: (src) => `/root/${src}`,
    })
    expect(table).toContain('src="/root/./i.png"')

    const quote = renderMarkdown("> ![q](./q.png)", {
      resolveImageSrc: (src) => `/root/${src}`,
    })
    expect(quote).toContain('src="/root/./q.png"')
  })
})

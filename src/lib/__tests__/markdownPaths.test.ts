import { describe, expect, it } from "vitest"
import {
  classifyHref,
  dirOf,
  isAbsolutePath,
  normalizePath,
  resolveImagePath,
  resolveRelativePath,
} from "@/lib/markdownPaths"

describe("classifyHref", () => {
  it("detects external URLs (http, https and protocol-relative)", () => {
    expect(classifyHref("https://example.com/docs")).toBe("external")
    expect(classifyHref("http://example.com/a?b=1#c")).toBe("external")
    expect(classifyHref("//cdn.example.com/logo.png")).toBe("external")
    expect(classifyHref("file:///Users/me/a.png")).toBe("external")
  })

  it("detects anchors, email and empty hrefs", () => {
    expect(classifyHref("#section")).toBe("anchor")
    expect(classifyHref("  #top  ")).toBe("anchor")
    expect(classifyHref("mailto:a@example.com")).toBe("email")
    expect(classifyHref("")).toBe("empty")
    expect(classifyHref("   ")).toBe("empty")
  })

  it("detects relative and absolute local paths", () => {
    expect(classifyHref("img.png")).toBe("relative")
    expect(classifyHref("./img.png")).toBe("relative")
    expect(classifyHref("../notes/a.md")).toBe("relative")
    expect(classifyHref("/Users/me/a.md")).toBe("absolute")
    expect(classifyHref("C:\\skills\\a.md")).toBe("absolute")
    expect(classifyHref("\\\\server\\share\\a.md")).toBe("absolute")
  })

  it("flags other URL schemes as unsupported", () => {
    expect(classifyHref("javascript:alert(1)")).toBe("unsupported")
    expect(classifyHref("ftp://example.com/a")).toBe("unsupported")
  })
})

describe("isAbsolutePath", () => {
  it("accepts POSIX, Windows drive and UNC paths only", () => {
    expect(isAbsolutePath("/a/b")).toBe(true)
    expect(isAbsolutePath("C:/a/b")).toBe(true)
    expect(isAbsolutePath("C:\\a\\b")).toBe(true)
    expect(isAbsolutePath("\\\\server\\share")).toBe(true)
    expect(isAbsolutePath("a/b")).toBe(false)
    expect(isAbsolutePath("./a/b")).toBe(false)
  })
})

describe("normalizePath", () => {
  it("collapses dot segments and keeps absolute roots", () => {
    expect(normalizePath("/a/b/../c")).toBe("/a/c")
    expect(normalizePath("/a/./b//c")).toBe("/a/b/c")
    expect(normalizePath("/../x")).toBe("/x")
    expect(normalizePath("a/b/../c")).toBe("a/c")
  })
})

describe("resolveRelativePath", () => {
  const base = "/skills/demo"

  it("resolves links against the current file directory", () => {
    expect(resolveRelativePath(base, "img.png")).toBe("/skills/demo/img.png")
    expect(resolveRelativePath(base, "./img.png")).toBe("/skills/demo/img.png")
    expect(resolveRelativePath(base, "../other/SKILL.md")).toBe("/skills/other/SKILL.md")
    expect(resolveRelativePath(`${base}/`, "./img.png")).toBe("/skills/demo/img.png")
  })

  it("returns normalized absolute paths unchanged", () => {
    expect(resolveRelativePath(base, "/abs/x.png")).toBe("/abs/x.png")
    expect(resolveRelativePath(base, "/abs/../x.png")).toBe("/x.png")
  })

  it("decodes URI escapes and drops fragments/queries", () => {
    expect(resolveRelativePath(base, "img%20one.png")).toBe("/skills/demo/img one.png")
    expect(resolveRelativePath(base, "file.md#section")).toBe("/skills/demo/file.md")
    expect(resolveRelativePath(base, "file.md?raw=1")).toBe("/skills/demo/file.md")
    expect(resolveRelativePath(base, "bad%escape.png")).toBe("/skills/demo/bad%escape.png")
  })

  it("never escapes the filesystem root", () => {
    expect(resolveRelativePath(base, "../../../../../etc/passwd")).toBe("/etc/passwd")
  })

  it("keeps the Windows separator for drive paths", () => {
    expect(resolveRelativePath("C:\\skills\\demo", "img.png")).toBe("C:\\skills\\demo\\img.png")
    expect(resolveRelativePath("C:\\skills\\demo", "..\\other\\a.md")).toBe("C:\\skills\\other\\a.md")
    expect(resolveRelativePath("C:\\skills\\demo", "D:/x.png")).toBe("D:\\x.png")
  })
})

describe("dirOf", () => {
  it("returns the parent directory of a rendered file", () => {
    expect(dirOf("/skills/demo/SKILL.md")).toBe("/skills/demo")
    expect(dirOf("/SKILL.md")).toBe("/")
    expect(dirOf("C:\\skills\\demo\\SKILL.md")).toBe("C:\\skills\\demo")
    expect(dirOf("C:\\SKILL.md")).toBe("C:\\")
    expect(dirOf("relative.md")).toBe("")
  })
})

describe("resolveImagePath", () => {
  it("resolves local image sources only", () => {
    expect(resolveImagePath("./img.png", "/skills/demo")).toBe("/skills/demo/img.png")
    expect(resolveImagePath("/abs/img.png", "/skills/demo")).toBe("/abs/img.png")
    expect(resolveImagePath("https://example.com/img.png", "/skills/demo")).toBeNull()
    expect(resolveImagePath("#anchor", "/skills/demo")).toBeNull()
    expect(resolveImagePath("data:image/png;base64,AAAA", "/skills/demo")).toBeNull()
  })
})

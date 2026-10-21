/**
 * Pure helpers for resolving markdown links/images that point at local files.
 *
 * Markdown previews run inside the Tauri webview: relative hrefs would resolve
 * against the app origin (and navigate the app away), so anchors are classified
 * here and resolved against the current file's directory before being opened in
 * the editor or rewritten to an asset URL.
 */
export type HrefKind = "anchor" | "external" | "email" | "absolute" | "relative" | "empty" | "unsupported"

const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/

/** Classify a markdown anchor href. */
export function classifyHref(href: string): HrefKind {
  const value = href.trim()
  if (!value) return "empty"
  if (value.startsWith("#")) return "anchor"
  // Protocol-relative URLs are external too.
  if (value.startsWith("//")) return "external"
  if (/^https?:\/\//i.test(value)) return "external"
  if (/^mailto:/i.test(value)) return "email"
  // file:// URLs are handed to the OS opener rather than the webview.
  if (/^file:/i.test(value)) return "external"
  // Check absolute paths before the generic scheme pattern: a Windows drive
  // prefix ("C:\...") also looks like a one-letter URL scheme.
  if (isAbsolutePath(value)) return "absolute"
  if (SCHEME_RE.test(value)) return "unsupported"
  return "relative"
}

/** True for POSIX, Windows drive and UNC absolute paths (not URLs). */
export function isAbsolutePath(value: string): boolean {
  return value.startsWith("/") || /^[a-zA-Z]:[\\/]/.test(value) || value.startsWith("\\\\")
}

function stripQueryAndFragment(value: string): string {
  const hashIndex = value.indexOf("#")
  const withoutHash = hashIndex === -1 ? value : value.slice(0, hashIndex)
  const queryIndex = withoutHash.indexOf("?")
  return queryIndex === -1 ? withoutHash : withoutHash.slice(0, queryIndex)
}

function decodePath(value: string): string {
  try {
    return decodeURIComponent(value)
  } catch {
    // Malformed escape sequences: keep the raw value.
    return value
  }
}

function separatorFor(baseDir: string): "/" | "\\" {
  if (/^[a-zA-Z]:[\\/]/.test(baseDir)) return "\\"
  return baseDir.includes("\\") && !baseDir.includes("/") ? "\\" : "/"
}

/** Collapse ".", ".." and duplicate separators; ".." never escapes the root. */
export function normalizePath(path: string, separator: "/" | "\\" = "/"): string {
  const driveMatch = /^([a-zA-Z]:)[\\/]/.exec(path)
  const drive = driveMatch ? driveMatch[1] : ""
  const rest = drive ? path.slice(driveMatch![0].length) : path
  const absolute = drive !== "" || rest.startsWith("/") || rest.startsWith("\\")
  const parts: string[] = []
  for (const segment of rest.split(/[\\/]+/)) {
    if (!segment || segment === ".") continue
    if (segment === "..") {
      if (parts.length > 0) parts.pop()
      continue
    }
    parts.push(segment)
  }
  const joined = parts.join(separator)
  if (drive) return drive + separator + joined
  return absolute ? separator + joined : joined
}

/**
 * Resolve a markdown href against the directory of the rendered file. Absolute
 * paths are normalized and returned as-is; query string/fragment and URI
 * escapes are removed/decoded because they are meaningless for local files.
 */
export function resolveRelativePath(baseDir: string, href: string): string {
  const separator = separatorFor(baseDir)
  const cleaned = decodePath(stripQueryAndFragment(href.trim()))
  if (isAbsolutePath(cleaned)) return normalizePath(cleaned, separator)
  const base = baseDir.replace(/[\\/]+$/, "")
  if (!cleaned) return normalizePath(base, separator)
  return normalizePath(`${base}${separator}${cleaned}`, separator)
}

/** Directory part of a file path ("/a/b.md" -> "/a"). */
export function dirOf(filePath: string): string {
  const normalized = filePath.replace(/[\\/]+$/, "")
  const index = Math.max(normalized.lastIndexOf("/"), normalized.lastIndexOf("\\"))
  if (index < 0) return ""
  const dir = normalized.slice(0, index)
  if (!dir) return "/"
  if (/^[a-zA-Z]:$/.test(dir)) return dir + "\\"
  return dir
}

/**
 * Absolute local path for a markdown `<img src>`, or null when the source is
 * remote/anchor and must be left untouched.
 */
export function resolveImagePath(src: string, baseDir: string): string | null {
  const kind = classifyHref(src)
  if (kind === "relative" || kind === "absolute") return resolveRelativePath(baseDir, src)
  return null
}

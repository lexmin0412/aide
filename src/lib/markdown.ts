// Minimal markdown renderer for the SKILL.md preview pane. Escapes all HTML
// first, then applies a small, well-understood subset of markdown.

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
}

function unescapeHtml(s: string): string {
  return s
    .replace(/&quot;/g, '"')
    .replace(/&gt;/g, ">")
    .replace(/&lt;/g, "<")
    .replace(/&amp;/g, "&")
}

export interface RenderMarkdownOptions {
  /**
   * Rewrite an `<img src>` before it is emitted (the preview uses it to turn
   * relative file paths into Tauri asset URLs). Remote/anchor sources are left
   * untouched by the caller.
   */
  resolveImageSrc?: (src: string) => string
}

function renderImage(alt: string, escapedSrc: string, options?: RenderMarkdownOptions): string {
  const rawSrc = unescapeHtml(escapedSrc)
  const resolved = options?.resolveImageSrc?.(rawSrc) ?? rawSrc
  return `<img alt="${alt}" src="${escapeHtml(resolved)}" />`
}

function renderInline(text: string, options?: RenderMarkdownOptions): string {
  let out = escapeHtml(text)
  out = out.replace(/`([^`]+)`/g, "<code>$1</code>")
  out = out.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, (_match, alt: string, src: string) =>
    renderImage(alt, src, options)
  )
  out = out.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2" rel="noopener noreferrer">$1</a>')
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
  out = out.replace(/(^|[^*])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
  return out
}

// GitHub-flavored table detection. A table is a row containing at least one
// pipe immediately followed by a delimiter row of dashes with optional colons.
const TABLE_DELIMITER_ROW = /^\s*\|?\s*:?-+:?\s*(?:\|\s*:?-+:?\s*)*\|?\s*$/

function splitTableRow(line: string): string[] {
  let row = line.trim()
  if (row.startsWith("|")) row = row.slice(1)
  if (row.endsWith("|") && !row.endsWith("\\|")) row = row.slice(0, -1)
  const cells: string[] = []
  let cell = ""
  for (let k = 0; k < row.length; k++) {
    if (row[k] === "\\" && row[k + 1] === "|") {
      cell += "|"
      k++
      continue
    }
    if (row[k] === "|") {
      cells.push(cell.trim())
      cell = ""
      continue
    }
    cell += row[k]
  }
  cells.push(cell.trim())
  return cells
}

function isTableStart(lines: string[], index: number): boolean {
  const header = lines[index]
  const delimiter = lines[index + 1]
  if (!header || !delimiter || !header.includes("|")) return false
  if (!TABLE_DELIMITER_ROW.test(delimiter)) return false
  // GFM requires the header and delimiter rows to have the same cell count.
  return splitTableRow(header).length === splitTableRow(delimiter).length
}

function renderTable(
  headerRow: string,
  delimiterRow: string,
  bodyRows: string[],
  options?: RenderMarkdownOptions
): string {
  const header = splitTableRow(headerRow)
  const alignments = splitTableRow(delimiterRow).map((spec) => {
    const left = spec.startsWith(":")
    const right = spec.endsWith(":")
    if (left && right) return "center"
    if (right) return "right"
    if (left) return "left"
    return ""
  })
  const cell = (tag: "th" | "td", text: string, index: number) => {
    const align = alignments[index]
    const attr = align ? ` style="text-align: ${align}"` : ""
    return `<${tag}${attr}>${renderInline(text, options)}</${tag}>`
  }
  const head = `<thead><tr>${header.map((text, idx) => cell("th", text, idx)).join("")}</tr></thead>`
  const body = bodyRows.length
    ? `<tbody>${bodyRows
        .map((row) => {
          const cells = splitTableRow(row)
          const normalized = header.map((_, idx) => cells[idx] ?? "")
          return `<tr>${normalized.map((text, idx) => cell("td", text, idx)).join("")}</tr>`
        })
        .join("")}</tbody>`
    : ""
  return `<div class="md-table-wrap"><table>${head}${body}</table></div>`
}

export function stripFrontmatter(content: string): string {
  const trimmed = content.trim()
  if (!trimmed.startsWith("---")) return content
  const rest = trimmed.slice(3)
  const end = rest.indexOf("---")
  if (end === -1) return content
  return rest.slice(end + 3).trim()
}

export function renderMarkdown(content: string, options: RenderMarkdownOptions = {}): string {
  const source = stripFrontmatter(content)
  const lines = source.split("\n")
  const out: string[] = []
  let i = 0

  const closeList = (stack: string[]) => {
    while (stack.length) out.push(stack.pop() === "ol" ? "</ol>" : "</ul>")
  }

  const listStack: string[] = []

  while (i < lines.length) {
    const line = lines[i]

    // Fenced code block
    if (line.trimStart().startsWith("```")) {
      closeList(listStack)
      const lang = line.trim().slice(3).trim()
      const body: string[] = []
      i++
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) {
        body.push(lines[i])
        i++
      }
      i++ // skip closing fence
      const cls = lang ? ` class="language-${escapeHtml(lang)}"` : ""
      out.push(`<pre><code${cls}>${escapeHtml(body.join("\n"))}</code></pre>`)
      continue
    }

    // Heading
    const heading = line.match(/^(#{1,6})\s+(.*)$/)
    if (heading) {
      closeList(listStack)
      const level = heading[1].length
      out.push(`<h${level}>${renderInline(heading[2], options)}</h${level}>`)
      i++
      continue
    }

    // Horizontal rule
    if (/^\s*(-{3,}|\*{3,})\s*$/.test(line)) {
      closeList(listStack)
      out.push("<hr />")
      i++
      continue
    }

    // Blockquote
    if (line.trimStart().startsWith(">")) {
      closeList(listStack)
      const body: string[] = []
      while (i < lines.length && lines[i].trimStart().startsWith(">")) {
        body.push(lines[i].trimStart().replace(/^>\s?/, ""))
        i++
      }
      out.push(`<blockquote>${renderMarkdown(body.join("\n"), options)}</blockquote>`)
      continue
    }

    // Unordered list
    if (/^\s*[-*+]\s+/.test(line)) {
      if (listStack[listStack.length - 1] !== "ul") {
        closeList(listStack)
        out.push("<ul>")
        listStack.push("ul")
      }
      out.push(`<li>${renderInline(line.replace(/^\s*[-*+]\s+/, ""), options)}</li>`)
      i++
      continue
    }

    // Ordered list
    if (/^\s*\d+\.\s+/.test(line)) {
      if (listStack[listStack.length - 1] !== "ol") {
        closeList(listStack)
        out.push("<ol>")
        listStack.push("ol")
      }
      out.push(`<li>${renderInline(line.replace(/^\s*\d+\.\s+/, ""), options)}</li>`)
      i++
      continue
    }

    closeList(listStack)

    // Table (GFM): header row + delimiter row + optional body rows
    if (isTableStart(lines, i)) {
      const header = lines[i]
      const delimiter = lines[i + 1]
      i += 2
      const body: string[] = []
      while (
        i < lines.length &&
        lines[i].trim() &&
        lines[i].includes("|") &&
        !lines[i].trimStart().startsWith("```")
      ) {
        body.push(lines[i])
        i++
      }
      out.push(renderTable(header, delimiter, body, options))
      continue
    }

    // Blank line
    if (!line.trim()) {
      i++
      continue
    }

    // Paragraph: gather until blank line
    const para: string[] = []
    while (
      i < lines.length &&
      lines[i].trim() &&
      !isTableStart(lines, i) &&
      !/^(#{1,6})\s+/.test(lines[i]) &&
      !lines[i].trimStart().startsWith("```") &&
      !lines[i].trimStart().startsWith(">") &&
      !/^\s*[-*+]\s+/.test(lines[i]) &&
      !/^\s*\d+\.\s+/.test(lines[i])
    ) {
      para.push(lines[i])
      i++
    }
    out.push(`<p>${renderInline(para.join("\n"), options).replace(/\n/g, "<br />")}</p>`)
  }

  closeList(listStack)
  return out.join("\n")
}

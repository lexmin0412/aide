// Minimal markdown renderer for the SKILL.md preview pane. Escapes all HTML
// first, then applies a small, well-understood subset of markdown.

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
}

function renderInline(text: string): string {
  let out = escapeHtml(text)
  out = out.replace(/`([^`]+)`/g, "<code>$1</code>")
  out = out.replace(/!\[([^\]]*)\]\(([^)\s]+)\)/g, '<img alt="$1" src="$2" />')
  out = out.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, '<a href="$2" rel="noopener noreferrer">$1</a>')
  out = out.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
  out = out.replace(/(^|[^*])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
  return out
}

export function stripFrontmatter(content: string): string {
  const trimmed = content.trim()
  if (!trimmed.startsWith("---")) return content
  const rest = trimmed.slice(3)
  const end = rest.indexOf("---")
  if (end === -1) return content
  return rest.slice(end + 3).trim()
}

export function renderMarkdown(content: string): string {
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
      out.push(`<h${level}>${renderInline(heading[2])}</h${level}>`)
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
      out.push(`<blockquote>${renderMarkdown(body.join("\n"))}</blockquote>`)
      continue
    }

    // Unordered list
    if (/^\s*[-*+]\s+/.test(line)) {
      if (listStack[listStack.length - 1] !== "ul") {
        closeList(listStack)
        out.push("<ul>")
        listStack.push("ul")
      }
      out.push(`<li>${renderInline(line.replace(/^\s*[-*+]\s+/, ""))}</li>`)
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
      out.push(`<li>${renderInline(line.replace(/^\s*\d+\.\s+/, ""))}</li>`)
      i++
      continue
    }

    closeList(listStack)

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
      !/^(#{1,6})\s+/.test(lines[i]) &&
      !lines[i].trimStart().startsWith("```") &&
      !lines[i].trimStart().startsWith(">") &&
      !/^\s*[-*+]\s+/.test(lines[i]) &&
      !/^\s*\d+\.\s+/.test(lines[i])
    ) {
      para.push(lines[i])
      i++
    }
    out.push(`<p>${renderInline(para.join("\n")).replace(/\n/g, "<br />")}</p>`)
  }

  closeList(listStack)
  return out.join("\n")
}

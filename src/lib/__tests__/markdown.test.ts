import { describe, expect, it } from "vitest";
import { renderMarkdown, stripFrontmatter } from "@/lib/markdown";

describe("stripFrontmatter", () => {
  it("removes a leading YAML frontmatter block", () => {
    const source = "---\ntitle: Test\n---\n# Hello\n\nworld";
    expect(stripFrontmatter(source)).toBe("# Hello\n\nworld");
  });

  it("leaves content without frontmatter untouched", () => {
    expect(stripFrontmatter("# Hello")).toBe("# Hello");
    expect(stripFrontmatter("plain text")).toBe("plain text");
  });
});

describe("renderMarkdown", () => {
  it("escapes raw HTML instead of rendering it", () => {
    const html = renderMarkdown('<script>alert("x")</script>');
    expect(html).not.toContain("<script>");
    expect(html).toContain("&lt;script&gt;");
    expect(html).toContain("&lt;/script&gt;");
  });

  it("renders GFM tables", () => {
    const html = renderMarkdown(
      ["| Name | Value |", "| --- | --- |", "| alpha | 1 |", "| beta | 2 |"].join("\n"),
    );
    expect(html).toContain("<table>");
    expect(html).toContain("<th>Name</th>");
    expect(html).toContain("<td>alpha</td>");
    expect(html).toContain("<td>2</td>");
  });

  it("strips frontmatter before rendering", () => {
    const html = renderMarkdown("---\ntitle: Test\n---\n# Hello");
    expect(html).toContain("<h1>Hello</h1>");
    expect(html).not.toContain("title: Test");
  });

  it("escapes HTML inside fenced code blocks", () => {
    const html = renderMarkdown(['```html', '<b>bold</b>', '```'].join("\n"));
    expect(html).toContain('<pre><code class="language-html">');
    expect(html).toContain("&lt;b&gt;bold&lt;/b&gt;");
    expect(html).not.toContain("<b>bold</b>");
  });
});

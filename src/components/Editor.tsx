import { useEffect, useRef, useMemo, useState } from "react"
import { EditorView, keymap } from "@codemirror/view"
import { EditorState, Compartment, type Extension } from "@codemirror/state"
import { basicSetup } from "codemirror"
import { json, jsonParseLinter } from "@codemirror/lang-json"
import { markdown } from "@codemirror/lang-markdown"
import { oneDark } from "@codemirror/theme-one-dark"
import { linter, lintGutter } from "@codemirror/lint"
import { indentOnInput, LanguageDescription } from "@codemirror/language"
import { languages } from "@codemirror/language-data"
import { Eye, Code } from "lucide-react"
import { useTranslation } from "react-i18next"
import { renderMarkdown } from "@/lib/markdown"
import type { EditorTab } from "../types"

interface EditorProps {
  tab: EditorTab
  onChange: (path: string, content: string) => void
  onSave: (path: string, content: string) => void
}

// Language support is swapped in through this compartment so lazily-imported
// grammars can be attached after the view already exists.
const languageCompartment = new Compartment()

function isMarkdownFile(filename: string) {
  const name = filename.toLowerCase()
  return name.endsWith(".md") || name.endsWith(".markdown")
}

// Languages that need eager loading (or custom config / linting). Everything
// else is resolved through @codemirror/language-data by filename.
function detectLanguage(filename: string): Extension | null {
  if (isMarkdownFile(filename)) return markdown({ codeLanguages: languages })

  const ext = filename.split(".").pop()?.toLowerCase()
  switch (ext) {
    case "json":
    case "jsonc":
      return json()
    default:
      return null
  }
}

function detectExtensions(filename: string, readonly: boolean) {
  const exts = [
    basicSetup,
    indentOnInput(),
    EditorView.theme({
      "&": { height: "100%" },
      ".cm-scroller": { overflow: "auto" },
    }),
  ]

  const lang = detectLanguage(filename)
  exts.push(languageCompartment.of(lang ?? []))

  if (filename.endsWith(".json") || filename.endsWith(".jsonc")) {
    exts.push(lintGutter())
    exts.push(linter(jsonParseLinter()))
  }

  if (readonly) {
    exts.push(EditorView.editable.of(false))
  }

  return exts
}

// Undo history and scroll survive tab switches: states are parked here keyed by
// theme + file path and reused when the on-disk/tab content still matches.
const stateCache = new Map<string, EditorState>()

const lightTheme = EditorView.theme(
  {
    "&": { backgroundColor: "var(--background)", color: "var(--foreground)" },
    ".cm-gutters": {
      backgroundColor: "var(--muted)",
      color: "var(--muted-foreground)",
      border: "none",
    },
    ".cm-activeLine": { backgroundColor: "var(--muted)" },
    ".cm-activeLineGutter": { backgroundColor: "var(--muted)" },
    ".cm-cursor": { borderLeftColor: "var(--foreground)" },
  },
  { dark: false }
)

// Blends oneDark's flat gray into the app's indigo card surface so the dark
// editor reads as part of the UI instead of a foreign panel.
const darkBlendTheme = EditorView.theme({
  "&": { backgroundColor: "var(--card)" },
  ".cm-gutters": {
    backgroundColor: "var(--card)",
    color: "var(--muted-foreground)",
    border: "none",
  },
  ".cm-activeLine": { backgroundColor: "oklch(0.68 0.2 285 / 7%)" },
  ".cm-activeLineGutter": { backgroundColor: "oklch(0.68 0.2 285 / 7%)" },
})

function ImageViewer({ tab }: { tab: EditorTab }) {
  return (
    <div className="h-full flex items-center justify-center p-4 overflow-auto bg-[repeating-conic-gradient(#8881_0%_25%,transparent_0%_50%)] bg-[length:16px_16px]">
      <img src={tab.content} alt={tab.name} className="max-w-full max-h-full object-contain rounded bg-background" />
    </div>
  )
}

function MarkdownPreview({ tab }: { tab: EditorTab }) {
  const html = useMemo(() => renderMarkdown(tab.content), [tab.content])
  return (
    <div
      className="h-full overflow-y-auto px-8 py-6 text-sm leading-relaxed markdown-preview"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}

function TextViewer({ tab, onChange, onSave }: EditorProps) {
  const editorRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const onChangeRef = useRef(onChange)
  const onSaveRef = useRef(onSave)
  const [preview, setPreview] = useState(false)
  const { t } = useTranslation()
  const [isDark, setIsDark] = useState(() => document.documentElement.classList.contains("dark"))
  onChangeRef.current = onChange
  onSaveRef.current = onSave

  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains("dark"))
    })
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })
    return () => observer.disconnect()
  }, [])

  const isMarkdown = isMarkdownFile(tab.name)
  const readonly = !tab.path.match(/\.(md|json|jsonc|yaml|yml|toml|txt|js|ts|jsx|tsx|css|html|sh|env)$/)

  const extensions = useMemo(() => {
    const exts = detectExtensions(tab.name, readonly)
    exts.push(isDark ? oneDark : lightTheme)
    if (isDark) exts.push(darkBlendTheme)

    if (!readonly) {
      exts.push(
        keymap.of([
          {
            key: "Mod-s",
            run: () => {
              if (viewRef.current) {
                const content = viewRef.current.state.doc.toString()
                onChangeRef.current(tab.path, content)
                onSaveRef.current(tab.path, content)
              }
              return true
            },
          },
        ])
      )
    }

    return exts
  }, [tab.name, readonly, isDark])

  useEffect(() => {
    if (!editorRef.current) return

    const cacheKey = (isDark ? "d:" : "l:") + tab.path
    const cached = stateCache.get(cacheKey)
    const state =
      cached && cached.doc.toString() === tab.content
        ? cached
        : EditorState.create({ doc: tab.content, extensions })

    const view = new EditorView({ state, parent: editorRef.current })
    viewRef.current = view

    // File types without a statically-known grammar (js, ts, css, yaml, sh,
    // toml, ...) are resolved by filename and imported on demand.
    let cancelled = false
    if (!detectLanguage(tab.name)) {
      const desc = LanguageDescription.matchFilename(languages, tab.name)
      if (desc) {
        desc
          .load()
          .then((support) => {
            if (!cancelled && viewRef.current === view) {
              view.dispatch({ effects: languageCompartment.reconfigure(support) })
            }
          })
          .catch(() => {})
      }
    }

    return () => {
      cancelled = true
      stateCache.set(cacheKey, view.state)
      view.destroy()
      viewRef.current = null
    }
  }, [tab.path, isDark])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return

    const current = view.state.doc.toString()
    if (current !== tab.content) {
      view.dispatch({
        changes: { from: 0, to: current.length, insert: tab.content },
      })
    }
  }, [tab.content])

  // The CodeMirror keymap is unmounted while previewing; keep Cmd+S working.
  useEffect(() => {
    if (!preview) return
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "s") {
        e.preventDefault()
        onChangeRef.current(tab.path, tab.content)
        onSaveRef.current(tab.path, tab.content)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [preview, tab.path, tab.content])

  return (
    <div className="editor-container h-full flex flex-col relative">
      {isMarkdown && (
        <div className="absolute top-2 right-4 z-10 flex gap-0.5 bg-card/90 border border-border rounded-md p-0.5 shadow-sm">
          <button
            className={`p-1 rounded ${!preview ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"}`}
            title={t("editor.edit")}
            onClick={() => setPreview(false)}
          >
            <Code size={13} />
          </button>
          <button
            className={`p-1 rounded ${preview ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"}`}
            title={t("editor.preview")}
            onClick={() => setPreview(true)}
          >
            <Eye size={13} />
          </button>
        </div>
      )}
      {isMarkdown && preview ? (
        <MarkdownPreview tab={tab} />
      ) : (
        <div className="editor-content flex-1 min-h-0" ref={editorRef} />
      )}
    </div>
  )
}

export function Editor({ tab, onChange, onSave }: EditorProps) {
  if (tab.is_image) return <ImageViewer tab={tab} />
  return <TextViewer tab={tab} onChange={onChange} onSave={onSave} />
}

export default Editor

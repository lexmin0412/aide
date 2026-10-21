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
import { getCachedState, getLatestStateForPath, setCachedState, stateCacheKey } from "@/lib/editorStateCache"
import type { DocReader } from "@/hooks/useTabs"
import type { EditorTab } from "../types"

interface EditorProps {
  tab: EditorTab
  /** Lightweight channel: fired once when the document becomes dirty. */
  onDirtyChange: (path: string, dirty: boolean) => void
  /** Save is content-less on purpose: the hook reads the live document. */
  onSave: (path: string) => void
  /** Registers a live-document reader so the tab cache can be flushed/saved. */
  registerDocSource: (path: string, reader: DocReader | null) => void
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

function MarkdownPreview({ content }: { content: string }) {
  const html = useMemo(() => renderMarkdown(content), [content])
  return (
    <div
      className="h-full overflow-y-auto px-8 py-6 text-sm leading-relaxed markdown-preview"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  )
}

function TextViewer({ tab, onDirtyChange, onSave, registerDocSource }: EditorProps) {
  const editorRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const onDirtyChangeRef = useRef(onDirtyChange)
  const onSaveRef = useRef(onSave)
  const registerDocSourceRef = useRef(registerDocSource)
  // True once we told the tab store this document is dirty; reset on save, tab
  // switch and external reload. Prevents one React update per keystroke.
  const dirtyReportedRef = useRef(false)
  // Set while dispatching a programmatic (external) document replacement so the
  // update listener does not mistake it for a user edit.
  const applyingExternalRef = useRef(false)
  const [previewPath, setPreviewPath] = useState<string | null>(null)
  const [previewContent, setPreviewContent] = useState("")
  const { t } = useTranslation()
  const [isDark, setIsDark] = useState(() => document.documentElement.classList.contains("dark"))

  // Callback refs are synced in an effect (never during render) so the
  // CodeMirror extensions created for a mount never hold stale closures.
  useEffect(() => {
    onDirtyChangeRef.current = onDirtyChange
    onSaveRef.current = onSave
    registerDocSourceRef.current = registerDocSource
  })

  useEffect(() => {
    const observer = new MutationObserver(() => {
      setIsDark(document.documentElement.classList.contains("dark"))
    })
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] })
    return () => observer.disconnect()
  }, [])

  const isMarkdown = isMarkdownFile(tab.name)
  const readonly = !tab.path.match(/\.(md|json|jsonc|yaml|yml|toml|txt|js|ts|jsx|tsx|css|html|sh|env)$/)
  const preview = previewPath === tab.path

  // Create/reuse the view. The parked state for this path is authoritative for
  // an open tab (it may be newer than tab.content), so edits survive switching.
  useEffect(() => {
    if (!editorRef.current) return

    const cacheKey = stateCacheKey(tab.path, isDark, tab.rev)
    const cached = getCachedState(cacheKey)
    let baseDoc = tab.content
    if (!cached && tab.is_dirty) {
      // Theme switch / revision mismatch: fall back to the newest parked doc
      // instead of the lagging tab.content snapshot.
      const parked = getLatestStateForPath(tab.path)
      if (parked) baseDoc = parked.doc.toString()
    }

    const extensions = detectExtensions(tab.name, readonly)
    extensions.push(isDark ? oneDark : lightTheme)
    if (isDark) extensions.push(darkBlendTheme)

    if (!readonly) {
      extensions.push(
        keymap.of([
          {
            key: "Mod-s",
            run: () => {
              // The hook resolves the live document through the registered
              // reader, so no content snapshot is passed here.
              onSaveRef.current(tab.path)
              return true
            },
          },
        ])
      )
    }

    // Dirty channel: cheap, fires at most once per dirty episode.
    extensions.push(
      EditorView.updateListener.of((update) => {
        if (!update.docChanged || applyingExternalRef.current) return
        if (dirtyReportedRef.current) return
        dirtyReportedRef.current = true
        onDirtyChangeRef.current(tab.path, true)
      })
    )

    const state = cached ?? EditorState.create({ doc: baseDoc, extensions })

    const view = new EditorView({ state, parent: editorRef.current })
    viewRef.current = view
    dirtyReportedRef.current = tab.is_dirty
    registerDocSourceRef.current(tab.path, () => viewRef.current?.state.doc.toString())

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
          .catch(() => {
            // Lazy grammar load failed: keep the plain-text editor.
          })
      }
    }

    return () => {
      cancelled = true
      registerDocSourceRef.current(tab.path, null)
      setCachedState(cacheKey, tab.path, view.state)
      view.destroy()
      viewRef.current = null
    }
    // tab.content / tab.is_dirty are read as a mount snapshot only: the effects
    // below handle later content/dirty changes, and re-creating the view on them
    // would drop focus and undo history.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.path, tab.rev, tab.name, isDark, readonly])

  // External content replacement (mtime reload, save completion): mirror it into
  // the mounted view when they diverge.
  useEffect(() => {
    const view = viewRef.current
    if (!view) return

    const current = view.state.doc.toString()
    if (current !== tab.content) {
      applyingExternalRef.current = true
      try {
        view.dispatch({
          changes: { from: 0, to: current.length, insert: tab.content },
        })
      } finally {
        applyingExternalRef.current = false
      }
    }
  }, [tab.content])

  // After a save (or a disk reload) the tab is clean again: allow the next edit
  // to report dirtiness once.
  useEffect(() => {
    if (!tab.is_dirty) dirtyReportedRef.current = false
  }, [tab.is_dirty, tab.path])

  // Keep Cmd+S working while the markdown preview is visible (the CodeMirror
  // keymap only fires when the editor is focused). The view stays mounted, so
  // the hook reads the live document.
  useEffect(() => {
    if (!preview) return
    const onKeyDown = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "s") return
      // The CodeMirror keymap already handles saves while the editor owns focus.
      if (editorRef.current?.contains(e.target as Node)) return
      e.preventDefault()
      onSaveRef.current(tab.path)
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [preview, tab.path])

  // CodeMirror measures poorly inside a display:none container; re-measure when
  // the editor becomes visible again.
  useEffect(() => {
    if (preview) return
    const frame = requestAnimationFrame(() => viewRef.current?.requestMeasure())
    return () => cancelAnimationFrame(frame)
  }, [preview])

  const showPreview = () => {
    const view = viewRef.current
    setPreviewContent(view ? view.state.doc.toString() : tab.content)
    setPreviewPath(tab.path)
  }

  return (
    <div className="editor-container h-full flex flex-col relative">
      {isMarkdown && (
        <div className="absolute top-2 right-4 z-10 flex gap-0.5 bg-card/90 border border-border rounded-md p-0.5 shadow-sm">
          <button
            className={`p-1 rounded ${!preview ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"}`}
            title={t("editor.edit")}
            aria-label={t("editor.edit")}
            aria-pressed={!preview}
            onClick={() => setPreviewPath(null)}
          >
            <Code size={13} />
          </button>
          <button
            className={`p-1 rounded ${preview ? "bg-secondary text-foreground" : "text-muted-foreground hover:text-foreground"}`}
            title={t("editor.preview")}
            aria-label={t("editor.preview")}
            aria-pressed={preview}
            onClick={showPreview}
          >
            <Eye size={13} />
          </button>
        </div>
      )}
      {isMarkdown && preview ? <MarkdownPreview content={previewContent} /> : null}
      <div
        className={`editor-content flex-1 min-h-0 ${isMarkdown && preview ? "hidden" : ""}`}
        ref={editorRef}
      />
    </div>
  )
}

export function Editor({ tab, onDirtyChange, onSave, registerDocSource }: EditorProps) {
  if (tab.is_image) return <ImageViewer tab={tab} />
  return (
    <TextViewer tab={tab} onDirtyChange={onDirtyChange} onSave={onSave} registerDocSource={registerDocSource} />
  )
}

export default Editor

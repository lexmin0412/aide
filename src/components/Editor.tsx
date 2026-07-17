import { useEffect, useRef, useMemo } from "react"
import { EditorView, keymap } from "@codemirror/view"
import { EditorState } from "@codemirror/state"
import { basicSetup } from "codemirror"
import { json, jsonParseLinter } from "@codemirror/lang-json"
import { markdown } from "@codemirror/lang-markdown"
import { oneDark } from "@codemirror/theme-one-dark"
import { linter, lintGutter } from "@codemirror/lint"
import { indentOnInput } from "@codemirror/language"
import type { EditorTab } from "../types"

interface EditorProps {
  tab: EditorTab
  onChange: (path: string, content: string) => void
  onSave: (path: string, content: string) => void
}

function detectLanguage(filename: string) {
  const ext = filename.split(".").pop()?.toLowerCase()
  switch (ext) {
    case "json":
    case "jsonc":
      return json()
    case "md":
    case "markdown":
      return markdown()
    default:
      return []
  }
}

function detectExtensions(filename: string, readonly: boolean) {
  const exts = [
    basicSetup,
    indentOnInput(),
    oneDark,
    EditorView.theme({
      "&": { height: "100%" },
      ".cm-scroller": { overflow: "auto" },
    }),
  ]

  const lang = detectLanguage(filename)
  if (lang) exts.push(lang)

  if (filename.endsWith(".json") || filename.endsWith(".jsonc")) {
    exts.push(lintGutter())
    exts.push(linter(jsonParseLinter()))
  }

  if (readonly) {
    exts.push(EditorView.editable.of(false))
  }

  return exts
}

export function Editor({ tab, onChange, onSave }: EditorProps) {
  const editorRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const onChangeRef = useRef(onChange)
  const onSaveRef = useRef(onSave)
  onChangeRef.current = onChange
  onSaveRef.current = onSave

  const readonly = !tab.path.match(/\.(md|json|jsonc|yaml|yml|toml|txt|js|ts|jsx|tsx|css|html|sh|env)$/)

  const extensions = useMemo(() => {
    const exts = detectExtensions(tab.name, readonly)

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
  }, [tab.name, readonly])

  useEffect(() => {
    if (!editorRef.current) return

    const state = EditorState.create({
      doc: tab.content,
      extensions,
    })

    const view = new EditorView({
      state,
      parent: editorRef.current,
    })

    viewRef.current = view

    return () => {
      view.destroy()
      viewRef.current = null
    }
  }, [tab.path])

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

  return (
    <div className="editor-container h-full flex flex-col">
      <div className="editor-content flex-1 min-h-0" ref={editorRef} />
    </div>
  )
}

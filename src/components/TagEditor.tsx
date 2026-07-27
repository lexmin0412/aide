import { useState, useRef, useEffect, useMemo, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"

interface TagEditorProps {
  path: string
  initialTags: string[]
  allTags: string[]
  onSave: (tags: string[]) => void
  onClose: () => void
}

export function TagEditor({ path, initialTags, allTags, onSave, onClose }: TagEditorProps) {
  const [tags, setTags] = useState<string[]>(initialTags)
  const [input, setInput] = useState("")
  const [saving, setSaving] = useState(false)
  const [focusIndex, setFocusIndex] = useState(0)
  const ref = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)
  const tagsRef = useRef(tags)
  tagsRef.current = tags

  useEffect(() => {
    const handler = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose()
      }
    }
    document.addEventListener("pointerdown", handler, true)
    return () => document.removeEventListener("pointerdown", handler, true)
  }, [onClose])

  const suggestions = useMemo(() => {
    if (!input.trim()) return []
    const q = input.toLowerCase()
    return allTags
      .filter((t) => t.toLowerCase().includes(q) && !tags.includes(t))
      .slice(0, 8)
  }, [input, allTags, tags])

  useEffect(() => {
    setFocusIndex(0)
  }, [suggestions.length])

  const addTag = useCallback((tag?: string) => {
    const t = (tag || inputRef.current?.value || "").trim()
    if (!t) return
    setTags((prev) => (prev.includes(t) ? prev : [...prev, t]))
    setInput("")
    inputRef.current?.focus()
  }, [])

  const removeTag = useCallback((tag: string) => {
    setTags((prev) => prev.filter((t) => t !== tag))
  }, [])

  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLInputElement>) => {
    const currentTags = tagsRef.current
    const val = e.currentTarget.value
    if (suggestions.length > 0 && e.key === "ArrowDown") {
      e.preventDefault()
      setFocusIndex((prev) => Math.min(prev + 1, suggestions.length - 1))
      return
    }
    if (suggestions.length > 0 && e.key === "ArrowUp") {
      e.preventDefault()
      setFocusIndex((prev) => Math.max(prev - 1, 0))
      return
    }
    if ((e.key === "Enter" || e.key === ",") && val.trim()) {
      e.preventDefault()
      if (suggestions.length > 0 && suggestions[focusIndex]) {
        addTag(suggestions[focusIndex])
      } else {
        addTag()
      }
      return
    }
    if (e.key === "Backspace" && !val && currentTags.length > 0) {
      return
    }
  }, [suggestions, focusIndex, addTag])

  const handleSave = useCallback(async () => {
    setSaving(true)
    try {
      await invoke("update_skill_tags", { path, tags })
      onSave(tags)
      onClose()
    } catch (e) {
      console.error("Failed to save tags:", e)
    } finally {
      setSaving(false)
    }
  }, [path, tags, onSave, onClose])

  return (
    <div
      ref={ref}
      className="absolute z-50 top-full right-0 mt-1 w-72 bg-popover border border-border rounded-lg shadow-xl p-3"
      onClick={(e) => e.stopPropagation()}
    >
      <div className="flex flex-wrap gap-1.5 min-h-[28px] mb-2">
        {tags.map((t) => (
          <span
            key={t}
            className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-secondary text-xs text-foreground"
          >
            {t}
            <button
              className="text-muted-foreground hover:text-foreground leading-none text-sm"
              onClick={() => removeTag(t)}
            >
              ×
            </button>
          </span>
        ))}
      </div>
      <div className="relative mb-3">
        <input
          ref={inputRef}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Add tag..."
          className="h-7 w-full min-w-0 rounded-lg border border-input bg-transparent px-2.5 py-1 text-xs transition-colors outline-none file:inline-flex file:h-6 file:border-0 file:bg-transparent file:text-sm file:font-medium file:text-foreground placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
        />
        {suggestions.length > 0 && (
          <div className="absolute left-0 right-0 top-full mt-0.5 bg-popover border border-border rounded-md shadow-lg overflow-hidden z-50">
            {suggestions.map((s, i) => (
              <button
                key={s}
                className={`w-full text-left px-2.5 py-1.5 text-xs transition-colors ${
                  i === focusIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
                }`}
                onClick={() => addTag(s)}
                onMouseEnter={() => setFocusIndex(i)}
              >
                {s}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="flex justify-end gap-1.5">
        <button
          onClick={onClose}
          className="px-3 py-1 text-xs text-muted-foreground hover:text-foreground"
        >
          Cancel
        </button>
        <button
          onClick={handleSave}
          disabled={saving}
          className="px-3 py-1 text-xs bg-foreground text-background rounded-md hover:opacity-90 disabled:opacity-50"
        >
          {saving ? "Saving..." : "Save"}
        </button>
      </div>
    </div>
  )
}

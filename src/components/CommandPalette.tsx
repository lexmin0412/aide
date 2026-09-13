import { useState, useEffect, useMemo, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Search, FileText } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import type { SkillInfo } from "../types"

interface SkillFileMatch {
  skill_name: string
  display_name: string
  file_path: string
  line_number: number
  line_text: string
}

interface CommandPaletteProps {
  open: boolean
  onClose: () => void
  onSelectSkill: (skill: SkillInfo) => void
  onSelectFileMatch: (skill: SkillInfo, filePath: string) => void
}

export function CommandPalette({ open, onClose, onSelectSkill, onSelectFileMatch }: CommandPaletteProps) {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [query, setQuery] = useState("")
  const [focusIndex, setFocusIndex] = useState(0)
  const [fileMatches, setFileMatches] = useState<SkillFileMatch[]>([])
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    setQuery("")
    setFocusIndex(0)
    setFileMatches([])
    invoke<SkillInfo[]>("list_skills").then(setSkills).catch(() => {})
    const timer = setTimeout(() => inputRef.current?.focus(), 0)
    return () => clearTimeout(timer)
  }, [open])

  // Full-text search across skill files, debounced.
  useEffect(() => {
    const q = query.trim()
    if (q.length < 2) {
      setFileMatches([])
      return
    }
    const timer = setTimeout(() => {
      invoke<SkillFileMatch[]>("search_skills", { query: q })
        .then(setFileMatches)
        .catch(() => setFileMatches([]))
    }, 150)
    return () => clearTimeout(timer)
  }, [query])

  const results = useMemo(() => {
    const q = query.trim().toLowerCase()
    const pool = skills
    if (!q) return pool.slice(0, 8)
    return pool
      .filter(
        (s) =>
          s.display_name.toLowerCase().includes(q) ||
          s.name.toLowerCase().includes(q) ||
          s.description?.toLowerCase().includes(q) ||
          s.tags.some((t) => t.toLowerCase().includes(q))
      )
      .slice(0, 8)
  }, [skills, query])

  useEffect(() => {
    setFocusIndex(0)
  }, [query])

  const total = results.length + fileMatches.length

  const chooseIndex = (index: number) => {
    if (index < results.length) {
      const skill = results[index]
      if (skill) onSelectSkill(skill)
    } else {
      const match = fileMatches[index - results.length]
      if (match) {
        const skill = skills.find((s) => s.name === match.skill_name)
        if (skill) onSelectFileMatch(skill, match.file_path)
      }
    }
    onClose()
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault()
      setFocusIndex((prev) => Math.min(prev + 1, total - 1))
    } else if (e.key === "ArrowUp") {
      e.preventDefault()
      setFocusIndex((prev) => Math.max(prev - 1, 0))
    } else if (e.key === "Enter" && total > 0) {
      e.preventDefault()
      chooseIndex(focusIndex)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="sm:max-w-[520px] top-[20%] translate-y-0" showCloseButton={false}>
        <DialogHeader className="sr-only">
          <DialogTitle>Search Skills</DialogTitle>
        </DialogHeader>
        <div className="flex items-center gap-2 border-b border-border pb-2">
          <Search size={14} className="text-muted-foreground shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search skills and their files..."
            className="w-full bg-transparent outline-none text-sm placeholder:text-muted-foreground"
          />
        </div>
        <div className="max-h-[320px] overflow-y-auto -mx-1">
          {total === 0 ? (
            <p className="text-xs text-muted-foreground text-center py-6">
              {query.trim().length >= 2 ? "No matches" : "Type to search skills"}
            </p>
          ) : (
            <>
              {results.length > 0 && (
                <p className="px-3 pt-1 pb-0.5 text-[10px] uppercase tracking-wider text-muted-foreground/70">Skills</p>
              )}
              {results.map((s, i) => (
                <button
                  key={s.path}
                  className={`w-full text-left px-3 py-2 rounded-md transition-colors ${
                    i === focusIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
                  }`}
                  onClick={() => chooseIndex(i)}
                  onMouseEnter={() => setFocusIndex(i)}
                >
                  <div className="text-sm font-medium truncate">{s.display_name}</div>
                  {s.description && (
                    <div className="text-xs text-muted-foreground truncate">{s.description}</div>
                  )}
                </button>
              ))}
              {fileMatches.length > 0 && (
                <p className="px-3 pt-2 pb-0.5 text-[10px] uppercase tracking-wider text-muted-foreground/70">In files</p>
              )}
              {fileMatches.map((m, i) => {
                const index = results.length + i
                const fileName = m.file_path.split("/").slice(-2).join("/")
                return (
                  <button
                    key={m.file_path + m.line_number}
                    className={`w-full text-left px-3 py-2 rounded-md transition-colors ${
                      index === focusIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
                    }`}
                    onClick={() => chooseIndex(index)}
                    onMouseEnter={() => setFocusIndex(index)}
                  >
                    <div className="flex items-center gap-1.5 text-xs font-medium truncate">
                      <FileText size={11} className="shrink-0 text-muted-foreground" />
                      <span className="truncate">{m.display_name}</span>
                      <span className="text-[10px] text-muted-foreground font-mono truncate">{fileName}:{m.line_number}</span>
                    </div>
                    <div className="text-xs text-muted-foreground truncate font-mono">{m.line_text}</div>
                  </button>
                )
              })}
            </>
          )}
        </div>
        <p className="text-[10px] text-muted-foreground text-center">↑↓ navigate · Enter open · Esc close</p>
      </DialogContent>
    </Dialog>
  )
}

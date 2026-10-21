import { useState, useEffect, useMemo, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Search, FileText } from "lucide-react"
import { useTranslation } from "react-i18next"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { normalizeSearchQuery, searchResults, shouldSearch, type SettledSearch } from "@/lib/searchState"
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
  const { t } = useTranslation()
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [query, setQuery] = useState("")
  const [focusIndex, setFocusIndex] = useState(0)
  // File matches are stored together with the query they belong to; the visible
  // list is derived below instead of being reset synchronously in an effect.
  const [settledMatches, setSettledMatches] = useState<SettledSearch<SkillFileMatch>>({
    query: "",
    results: [],
  })
  const inputRef = useRef<HTMLInputElement>(null)
  // Request sequencing: a slow response for an older query must not overwrite
  // the results of a newer one.
  const searchSeqRef = useRef(0)

  // Reset the query when the palette opens (documented "adjust state when a prop
  // changes" pattern instead of a reset effect).
  const [prevOpen, setPrevOpen] = useState(open)
  if (open !== prevOpen) {
    setPrevOpen(open)
    if (open) {
      setQuery("")
      setFocusIndex(0)
    }
  }
  const normalizedQuery = normalizeSearchQuery(query)
  const [prevQuery, setPrevQuery] = useState(normalizedQuery)
  if (normalizedQuery !== prevQuery) {
    setPrevQuery(normalizedQuery)
    setFocusIndex(0)
  }

  useEffect(() => {
    if (!open) return
    invoke<SkillInfo[]>("list_skills")
      .then(setSkills)
      .catch(() => {
        // Keep the last known skill list; the palette stays usable.
      })
    const timer = setTimeout(() => inputRef.current?.focus(), 0)
    return () => clearTimeout(timer)
  }, [open])

  // Full-text search across skill files, debounced. Responses for stale queries
  // are ignored, and the visible list is derived so short queries need no reset.
  useEffect(() => {
    if (!shouldSearch(normalizedQuery)) return
    const seq = ++searchSeqRef.current
    const timer = setTimeout(() => {
      invoke<SkillFileMatch[]>("search_skills", { query: normalizedQuery })
        .then((matches) => {
          if (searchSeqRef.current === seq) {
            setSettledMatches({ query: normalizedQuery, results: matches })
          }
        })
        .catch(() => {
          // Search failures degrade to "no file matches"; skill results remain.
          if (searchSeqRef.current === seq) {
            setSettledMatches({ query: normalizedQuery, results: [] })
          }
        })
    }, 150)
    return () => clearTimeout(timer)
  }, [normalizedQuery])

  const fileMatches = searchResults(query, settledMatches)

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

  const total = results.length + fileMatches.length
  // Clamp instead of resetting in an effect so the highlight never points past
  // the end of the current result list.
  const activeIndex = Math.min(focusIndex, Math.max(0, total - 1))

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
      setFocusIndex(Math.min(activeIndex + 1, total - 1))
    } else if (e.key === "ArrowUp") {
      e.preventDefault()
      setFocusIndex(Math.max(activeIndex - 1, 0))
    } else if (e.key === "Enter" && total > 0) {
      e.preventDefault()
      chooseIndex(activeIndex)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent
        className="sm:max-w-[520px] top-[20%] translate-y-0"
        showCloseButton={false}
        onEscapeKeyDown={(event) => {
          event.preventDefault()
          onClose()
        }}
      >
        <DialogHeader className="sr-only">
          <DialogTitle>{t("palette.title")}</DialogTitle>
        </DialogHeader>
        <div className="flex items-center gap-2 border-b border-border pb-2">
          <Search size={14} className="text-muted-foreground shrink-0" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKeyDown}
            placeholder={t("palette.placeholder")}
            className="w-full bg-transparent outline-none text-sm placeholder:text-muted-foreground"
          />
        </div>
        <div className="max-h-[320px] overflow-y-auto -mx-1">
          {total === 0 ? (
            <p className="text-xs text-muted-foreground text-center py-6">
              {query.trim().length >= 2 ? t("palette.noMatches") : t("palette.typeToSearch")}
            </p>
          ) : (
            <>
              {results.length > 0 && (
                <p className="px-3 pt-1 pb-0.5 text-[10px] uppercase tracking-wider text-muted-foreground/70">{t("palette.skillsGroup")}</p>
              )}
              {results.map((s, i) => (
                <button
                  key={s.path}
                  className={`w-full text-left px-3 py-2 rounded-md transition-colors ${
                    i === activeIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
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
                <p className="px-3 pt-2 pb-0.5 text-[10px] uppercase tracking-wider text-muted-foreground/70">{t("palette.filesGroup")}</p>
              )}
              {fileMatches.map((m, i) => {
                const index = results.length + i
                const fileName = m.file_path.split("/").slice(-2).join("/")
                return (
                  <button
                    key={m.file_path + m.line_number}
                    className={`w-full text-left px-3 py-2 rounded-md transition-colors ${
                      index === activeIndex ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
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
        <p className="text-[10px] text-muted-foreground text-center">{t("palette.hint")}</p>
      </DialogContent>
    </Dialog>
  )
}

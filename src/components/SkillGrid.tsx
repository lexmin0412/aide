import { useState, useEffect, useMemo, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { SkillCard } from "./SkillCard"
import { SyncPanel } from "./SyncPanel"
import { useSkillStore } from "../stores/skillStore"
import type { SkillInfo } from "../types"

interface SkillGridProps {
  onSelectSkill: (skill: SkillInfo) => void
}

export default function SkillGrid({ onSelectSkill }: SkillGridProps) {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [syncOpen, setSyncOpen] = useState(false)
  const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set())
  const { searchQuery, setSearchQuery, scrollPosition, setScrollPosition } = useSkillStore()
  const scrollRef = useRef<HTMLDivElement>(null)
  const restoredRef = useRef(false)

  useEffect(() => {
    invoke<SkillInfo[]>("list_skills")
      .then(setSkills)
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const onScroll = () => setScrollPosition(el.scrollTop)
    el.addEventListener("scroll", onScroll, { passive: true })
    return () => el.removeEventListener("scroll", onScroll)
  }, [setScrollPosition])

  useEffect(() => {
    if (loading || restoredRef.current) return
    restoredRef.current = true
    if (scrollPosition > 0) {
      const el = scrollRef.current
      if (!el) return
      let attempts = 0
      const tryScroll = () => {
        if (el.scrollHeight > el.clientHeight) {
          el.scrollTo(0, scrollPosition)
        } else if (attempts < 15) {
          attempts++
          requestAnimationFrame(tryScroll)
        }
      }
      requestAnimationFrame(tryScroll)
    }
  }, [loading, scrollPosition])

  const allTags = useMemo(() => {
    const tags = new Set<string>()
    for (const s of skills) {
      for (const t of s.tags) tags.add(t)
    }
    return Array.from(tags).sort()
  }, [skills])

  const toggleTag = (tag: string) => {
    setSelectedTags((prev) => {
      const next = new Set(prev)
      if (next.has(tag)) next.delete(tag)
      else next.add(tag)
      return next
    })
  }

  const filtered = useMemo(() => {
    let result = skills
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase()
      result = result.filter(
        (s) => s.display_name.toLowerCase().includes(q) || s.description?.toLowerCase().includes(q)
      )
    }
    if (selectedTags.size > 0) {
      result = result.filter((s) => s.tags.some((t) => selectedTags.has(t)))
    }
    return result
  }, [skills, searchQuery, selectedTags])

  if (loading) {
    return <div className="h-full flex items-center justify-center text-sm text-muted-foreground">Loading...</div>
  }

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-4 px-6 pt-5 pb-4 shrink-0">
        <div className="flex-1 min-w-0">
          <h1 className="text-lg font-semibold tracking-tight">Skills</h1>
          <p className="text-xs text-muted-foreground mt-0.5">{skills.length} installed</p>
        </div>
        <Input
          placeholder="Search skills..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="max-w-[260px] h-8 text-xs"
        />
        <Button variant="outline" size="sm" onClick={() => setSyncOpen(true)}>Sync</Button>
      </div>
      {allTags.length > 0 && (
        <div className="flex items-center gap-1.5 px-6 pb-3 shrink-0 flex-wrap">
          {allTags.map((tag) => {
            const active = selectedTags.has(tag)
            return (
              <button
                key={tag}
                onClick={() => toggleTag(tag)}
                className={`px-2 py-0.5 rounded-full text-[11px] border transition-colors ${
                  active
                    ? "bg-foreground text-background border-foreground"
                    : "bg-transparent text-muted-foreground border-border hover:border-foreground/30 hover:text-foreground"
                }`}
              >
                {tag}
              </button>
            )
          })}
          {selectedTags.size > 0 && (
            <button
              onClick={() => setSelectedTags(new Set())}
              className="px-2 py-0.5 text-[11px] text-muted-foreground hover:text-foreground"
            >
              Clear
            </button>
          )}
        </div>
      )}
      <div ref={scrollRef} className="flex-1 overflow-y-auto px-6 pb-6">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
          {filtered.map((skill) => (
            <SkillCard
              key={skill.name}
              skill={skill}
              allTags={allTags}
              onClick={onSelectSkill}
              onTagsChanged={(skillPath, tags) => {
                setSkills((prev) => prev.map((s) => s.path === skillPath ? { ...s, tags } : s))
              }}
            />
          ))}
        </div>
        {searchQuery && filtered.length === 0 && (
          <div className="text-sm text-muted-foreground text-center mt-12">No skills match "{searchQuery}"</div>
        )}
      </div>
      <SyncPanel open={syncOpen} onClose={() => setSyncOpen(false)} />
    </div>
  )
}

import { useState, useEffect, useMemo } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { SkillCard } from "./SkillCard"
import { SyncPanel } from "./SyncPanel"
import type { SkillInfo } from "../types"

interface SkillGridProps {
  onSelectSkill: (skill: SkillInfo) => void
}

export default function SkillGrid({ onSelectSkill }: SkillGridProps) {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [syncOpen, setSyncOpen] = useState(false)
  const [query, setQuery] = useState("")

  useEffect(() => {
    invoke<SkillInfo[]>("list_skills")
      .then(setSkills)
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  const filtered = useMemo(() => {
    if (!query.trim()) return skills
    const q = query.toLowerCase()
    return skills.filter(
      (s) => s.display_name.toLowerCase().includes(q) || s.description?.toLowerCase().includes(q)
    )
  }, [skills, query])

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
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="max-w-[260px] h-8 text-xs"
        />
        <Button variant="outline" size="sm" onClick={() => setSyncOpen(true)}>Sync</Button>
      </div>
      <div className="flex-1 overflow-y-auto px-6 pb-6">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
          {filtered.map((skill) => (
            <SkillCard key={skill.name} skill={skill} onClick={onSelectSkill} />
          ))}
        </div>
        {query && filtered.length === 0 && (
          <div className="text-sm text-muted-foreground text-center mt-12">No skills match "{query}"</div>
        )}
      </div>
      <SyncPanel open={syncOpen} onClose={() => setSyncOpen(false)} />
    </div>
  )
}

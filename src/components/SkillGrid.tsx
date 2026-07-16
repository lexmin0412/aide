import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { SkillInfo } from "../types"
import { SkillCard } from "./SkillCard"
import { SyncPanel } from "./SyncPanel"

interface SkillGridProps {
  onSelectSkill: (skill: SkillInfo) => void
}

export function SkillGrid({ onSelectSkill }: SkillGridProps) {
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [syncOpen, setSyncOpen] = useState(false)

  useEffect(() => {
    invoke<SkillInfo[]>("list_skills")
      .then(setSkills)
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [])

  if (loading) {
    return <div className="skill-grid-loading">Loading skills...</div>
  }

  return (
    <div className="skill-grid">
      <div className="skill-grid-header">
        <div className="skill-grid-header-left">
          <h1>Skills</h1>
          <span className="skill-grid-count">{skills.length} skills</span>
        </div>
        <button className="sync-open-btn" onClick={() => setSyncOpen(true)}>
          Sync
        </button>
      </div>
      <div className="skill-grid-cards">
        {skills.map((skill) => (
          <SkillCard
            key={skill.name}
            skill={skill}
            onClick={onSelectSkill}
          />
        ))}
      </div>
      <SyncPanel open={syncOpen} onClose={() => setSyncOpen(false)} />
    </div>
  )
}

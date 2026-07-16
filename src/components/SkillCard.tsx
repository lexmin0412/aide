import type { SkillInfo } from "../types"

interface SkillCardProps {
  skill: SkillInfo
  onClick: (skill: SkillInfo) => void
}

export function SkillCard({ skill, onClick }: SkillCardProps) {
  return (
    <div className="skill-card" onClick={() => onClick(skill)}>
      <div className="skill-card-header">
        <span className="skill-card-icon">
          {skill.is_symlink ? "\uD83D\uDD17" : "\uD83E\uDE84"}
        </span>
        <span className="skill-card-name">{skill.display_name}</span>
      </div>
      <div className="skill-card-desc">
        {skill.description || (
          <span className="skill-card-desc-empty">No description</span>
        )}
      </div>
      <div className="skill-card-meta">
        <span>{skill.file_count} files</span>
        {skill.is_symlink && skill.target_path && (
          <span className="skill-card-symlink" title={skill.target_path}>
            symlink
          </span>
        )}
      </div>
    </div>
  )
}

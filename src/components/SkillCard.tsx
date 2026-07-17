import type { SkillInfo } from "../types"

interface SkillCardProps {
  skill: SkillInfo
  onClick: (skill: SkillInfo) => void
}

export function SkillCard({ skill, onClick }: SkillCardProps) {
  return (
    <div
      onClick={() => onClick(skill)}
      className="group bg-card border border-border rounded-xl p-5 cursor-pointer transition-all hover:border-foreground/30 hover:bg-card/80 flex flex-col gap-2.5 min-h-[140px]"
    >
      <div className="flex items-center gap-2">
        <span className="text-base">{skill.is_symlink ? "🔗" : "🪄"}</span>
        <span className="text-sm font-semibold truncate">{skill.display_name}</span>
      </div>
      <div className="text-xs text-muted-foreground line-clamp-3 flex-1 leading-relaxed">
        {skill.description || <span className="italic opacity-60">No description</span>}
      </div>
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground/80">
        <span>{skill.file_count} files</span>
        {skill.is_symlink && (
          <span className="px-1.5 py-0.5 rounded bg-blue-500/15 text-blue-400 text-[10px]">symlink</span>
        )}
      </div>
    </div>
  )
}

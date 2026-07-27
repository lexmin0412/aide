import { useState } from "react"
import type { SkillInfo } from "../types"
import { TagEditor } from "./TagEditor"

interface SkillCardProps {
  skill: SkillInfo
  allTags: string[]
  onClick: (skill: SkillInfo) => void
  onTagsChanged?: (skillPath: string, tags: string[]) => void
}

export function SkillCard({ skill, allTags, onClick, onTagsChanged }: SkillCardProps) {
  const [showTagEditor, setShowTagEditor] = useState(false)

  return (
    <div
      onClick={() => onClick(skill)}
      className="group bg-card border border-border rounded-xl p-5 cursor-pointer transition-all hover:border-foreground/30 hover:bg-card/80 flex flex-col gap-2.5 min-h-[140px] relative"
    >
      <div className="flex items-center gap-2">
        <span className="text-base">{skill.is_symlink ? "↗" : "◆"}</span>
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
        <span
          className="ml-auto flex gap-1 relative"
          onClick={(e) => {
            e.stopPropagation()
            setShowTagEditor(true)
          }}
        >
          {skill.tags.length > 0 ? (
            <>
              {skill.tags.slice(0, 3).map((t) => (
                <span key={t} className="px-1.5 py-0.5 rounded bg-secondary text-[10px] text-muted-foreground truncate max-w-[80px]">
                  {t}
                </span>
              ))}
              {skill.tags.length > 3 && (
                <span className="text-[10px] text-muted-foreground">+{skill.tags.length - 3}</span>
              )}
            </>
          ) : (
            <span className="text-[10px] text-muted-foreground/50 group-hover:text-muted-foreground transition-colors">
              + tags
            </span>
          )}
          {showTagEditor && (
            <TagEditor
              path={skill.path}
              initialTags={skill.tags}
              allTags={allTags}
              onSave={(tags) => onTagsChanged?.(skill.path, tags)}
              onClose={() => setShowTagEditor(false)}
            />
          )}
        </span>
      </div>
    </div>
  )
}

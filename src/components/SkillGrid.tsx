import { useState, useEffect, useMemo, useRef } from "react"
import { useTranslation } from "react-i18next"
import { invoke } from "@tauri-apps/api/core"
import { open } from "@tauri-apps/plugin-dialog"
import { Plus, FolderDown, FolderInput, Store, GitBranch, RefreshCw } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { SkillCard } from "./SkillCard"
import { SyncPanel } from "./SyncPanel"
import { NewSkillDialog } from "./NewSkillDialog"
import { MarketDialog } from "./MarketDialog"
import { GitSyncDialog } from "./GitSyncDialog"
import { CardGridSkeleton } from "./Skeleton"
import { toast } from "@/lib/toast"
import { useSkillStore } from "../stores/skillStore"
import type { SkillInfo } from "../types"

interface SkillGridProps {
  onSelectSkill: (skill: SkillInfo) => void
}

export default function SkillGrid({ onSelectSkill }: SkillGridProps) {
  const { t } = useTranslation()
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [syncOpen, setSyncOpen] = useState(false)
  const [newOpen, setNewOpen] = useState(false)
  const [marketOpen, setMarketOpen] = useState(false)
  const [gitOpen, setGitOpen] = useState(false)
  const [updateCount, setUpdateCount] = useState(0)
  const [refreshing, setRefreshing] = useState(false)
  const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set())
  // Subscribe narrowly: the grid must not re-render on every scroll frame.
  const searchQuery = useSkillStore((s) => s.searchQuery)
  const setSearchQuery = useSkillStore((s) => s.setSearchQuery)
  const scrollRef = useRef<HTMLDivElement>(null)
  const restoredRef = useRef(false)

  const refresh = () => {
    return invoke<SkillInfo[]>("list_skills")
      .then(setSkills)
      .catch((e) => console.error("Failed to list skills:", e))
  }

  const refreshUpdates = () => {
    invoke<number[]>("check_skill_updates")
      .then((u) => setUpdateCount(u.length))
      .catch(() => {})
  }

  // Manual refresh: picks up skills added outside the app (e.g. directly in
  // ~/.agents/skills) without restarting.
  const handleRefresh = () => {
    setRefreshing(true)
    Promise.all([
      invoke<SkillInfo[]>("list_skills").then((list) => {
        setSkills(list)
        toast(t("skills.refreshed", { count: list.length }), "success")
      }),
      invoke<number[]>("check_skill_updates").then((u) => setUpdateCount(u.length)).catch(() => {}),
    ])
      .catch((e) => toast(t("skills.refreshFailed", { error: String(e) }), "error"))
      .finally(() => setRefreshing(false))
  }

  const importSkill = async () => {
    const selected = await open({ directory: true, multiple: false, title: t("skills.selectFolder") })
    if (typeof selected !== "string") return
    try {
      const name = await invoke<string>("import_skill", { source: selected })
      toast(t("skills.imported", { name }), "success")
      void refresh()
    } catch (e) {
      toast(t("skills.importFailed", { error: String(e) }), "error")
    }
  }

  useEffect(() => {
    refresh().finally(() => setLoading(false))
    // Silent check on mount: a dot on the Browse button signals stale skills.
    invoke<number[]>("check_skill_updates")
      .then((u) => setUpdateCount(u.length))
      .catch(() => {})
  }, [])

  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const onScroll = () => useSkillStore.getState().setScrollPosition(el.scrollTop)
    el.addEventListener("scroll", onScroll, { passive: true })
    return () => el.removeEventListener("scroll", onScroll)
  }, [])

  useEffect(() => {
    if (loading || restoredRef.current) return
    restoredRef.current = true
    const scrollPosition = useSkillStore.getState().scrollPosition
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
  }, [loading])

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
        (s) =>
          s.display_name.toLowerCase().includes(q) ||
          s.name.toLowerCase().includes(q) ||
          s.description?.toLowerCase().includes(q) ||
          s.tags.some((t) => t.toLowerCase().includes(q))
      )
    }
    if (selectedTags.size > 0) {
      result = result.filter((s) => s.tags.some((t) => selectedTags.has(t)))
    }
    return result
  }, [skills, searchQuery, selectedTags])

  if (loading) {
    return (
      <div className="h-full flex flex-col">
        <div className="flex items-center gap-4 px-6 pt-5 pb-4 shrink-0">
          <div className="flex-1">
            <h1 className="text-lg font-semibold tracking-tight">{t("skills.title")}</h1>
          </div>
        </div>
        <div className="flex-1 overflow-y-auto px-6 pb-6">
          <CardGridSkeleton />
        </div>
      </div>
    )
  }

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-4 px-6 pt-5 pb-4 shrink-0">
        <div className="flex-1 min-w-0">
          <h1 className="text-lg font-semibold tracking-tight">{t("skills.title")}</h1>
          <p className="text-xs text-muted-foreground mt-0.5">{t("skills.installed", { count: skills.length })}</p>
        </div>
        <Input
          placeholder={t("skills.searchPlaceholder")}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="max-w-[260px] h-8 text-xs"
        />
        <Button variant="outline" size="sm" onClick={handleRefresh} disabled={refreshing} title={t("skills.refresh")}>
          <RefreshCw size={13} className={refreshing ? "animate-spin" : ""} />
        </Button>
        <Button variant="outline" size="sm" onClick={() => setGitOpen(true)}>
          <GitBranch size={13} /> {t("skills.git")}
        </Button>
        <Button variant="outline" size="sm" onClick={() => setSyncOpen(true)}>{t("skills.sync")}</Button>
        <Button variant="outline" size="sm" className="relative" onClick={() => setMarketOpen(true)}>
          <Store size={13} /> {t("skills.browse")}
          {updateCount > 0 && (
            <span
              className="absolute -top-1 -right-1 min-w-[16px] h-4 px-1 rounded-full bg-primary text-primary-foreground text-[9px] font-medium flex items-center justify-center"
              title={t("skills.updatesBadge", { count: updateCount })}
            >
              {updateCount}
            </span>
          )}
        </Button>
        <Button variant="outline" size="sm" onClick={() => void importSkill()}>
          <FolderInput size={13} /> {t("skills.import")}
        </Button>
        <Button size="sm" onClick={() => setNewOpen(true)}>
          <Plus size={13} /> {t("skills.new")}
        </Button>
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
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-transparent text-muted-foreground border-border hover:border-primary/40 hover:text-foreground"
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
        {skills.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center gap-3 text-center">
            <FolderDown size={36} className="text-primary/40" />
            <div>
              <p className="text-sm font-medium">{t("skills.empty.title")}</p>
              <p className="text-xs text-muted-foreground mt-1 max-w-[360px]">
                Skills live in <span className="font-mono">~/.agents/skills</span>, one folder per skill with a
                SKILL.md file. Create your first skill, or use Sync to link existing tool directories.
              </p>
            </div>
            <div className="flex gap-2">
              <Button size="sm" onClick={() => setNewOpen(true)}>
                <Plus size={13} /> {t("skills.empty.create")}
              </Button>
              <Button variant="outline" size="sm" onClick={() => void importSkill()}>
                <FolderInput size={13} /> {t("skills.empty.import")}
              </Button>
            </div>
          </div>
        ) : (
          <>
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
              <div className="text-sm text-muted-foreground text-center mt-12">{t("skills.noMatch", { query: searchQuery })}</div>
            )}
          </>
        )}
      </div>
      <SyncPanel open={syncOpen} onClose={() => setSyncOpen(false)} />
      <GitSyncDialog
        open={gitOpen}
        skills={skills}
        onClose={() => setGitOpen(false)}
        onChanged={() => void refresh()}
      />
      <MarketDialog
        open={marketOpen}
        skills={skills}
        onClose={() => setMarketOpen(false)}
        onInstalled={() => { void refresh(); refreshUpdates() }}
      />
      <NewSkillDialog
        open={newOpen}
        onClose={() => setNewOpen(false)}
        onCreated={(dirName) => {
          void invoke<SkillInfo[]>("list_skills").then((list) => {
            setSkills(list)
            // Jump straight into the freshly created skill.
            const created = list.find((s) => s.name === dirName)
            if (created) onSelectSkill(created)
          })
        }}
      />
    </div>
  )
}

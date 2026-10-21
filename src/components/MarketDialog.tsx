import { useState, useEffect, useRef } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Search, Download, Check, Store, ExternalLink } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toast } from "@/lib/toast"
import { useTranslation } from "react-i18next"
import {
  isSearchPending,
  normalizeSearchQuery,
  searchResults,
  shouldSearch,
  type SettledSearch,
} from "@/lib/searchState"
import { shortCommit, summarizeUpdatePreview } from "@/lib/updatePreview"
import type { SkillInfo, SkillUpdatePreview } from "../types"

interface RegistrySkill {
  id: string
  name: string
  installs: number
  source: string
}

interface MarketDialogProps {
  open: boolean
  skills: SkillInfo[]
  onClose: () => void
  onInstalled: () => void
}

interface SkillUpdateInfo {
  skill: string
  id: string
  installed_commit: string
  latest_commit: string
}

function formatInstalls(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`
  return String(n)
}

/** One grouped file list inside the update-review dialog. */
function PreviewGroup({ title, files, tone }: { title: string; files: string[]; tone: string }) {
  if (files.length === 0) return null
  return (
    <div>
      <p className={`text-xs font-medium ${tone}`}>{title}</p>
      <ul className="mt-1 space-y-0.5">
        {files.slice(0, 20).map((file) => (
          <li key={file} className="font-mono text-[10px] text-muted-foreground truncate" title={file}>
            {file}
          </li>
        ))}
      </ul>
    </div>
  )
}

export function MarketDialog({ open, skills, onClose, onInstalled }: MarketDialogProps) {
  const { t } = useTranslation()
  const [query, setQuery] = useState("")
  // Registry results are stored with the query they belong to; the visible list
  // and the "searching" indicator are derived below instead of being reset
  // synchronously inside an effect.
  const [settled, setSettled] = useState<SettledSearch<RegistrySkill>>({ query: "", results: [] })
  const [installing, setInstalling] = useState<string | null>(null)
  const [updates, setUpdates] = useState<SkillUpdateInfo[]>([])
  const [updating, setUpdating] = useState<string | null>(null)
  // Review-before-update: preview the file diff, then apply update_skill.
  const [previewing, setPreviewing] = useState<string | null>(null)
  const [preview, setPreview] = useState<SkillUpdatePreview | null>(null)
  // Ignore responses for stale (debounced) queries.
  const searchSeqRef = useRef(0)
  const searchInputRef = useRef<HTMLInputElement>(null)

  // Reset the query when the dialog opens (adjust-state-during-render pattern).
  const [prevOpen, setPrevOpen] = useState(open)
  if (open !== prevOpen) {
    setPrevOpen(open)
    if (open) setQuery("")
    else {
      setPreview(null)
      setPreviewing(null)
    }
  }
  const normalizedQuery = normalizeSearchQuery(query)

  useEffect(() => {
    if (!open) return
    invoke<SkillUpdateInfo[]>("check_skill_updates")
      .then(setUpdates)
      .catch(() => {
        // Update badges are optional; a failed check leaves them empty.
      })
    const frame = requestAnimationFrame(() => searchInputRef.current?.focus())
    return () => cancelAnimationFrame(frame)
  }, [open])

  useEffect(() => {
    if (!shouldSearch(normalizedQuery)) return
    const seq = ++searchSeqRef.current
    const timer = setTimeout(() => {
      invoke<RegistrySkill[]>("search_registry", { query: normalizedQuery })
        .then((list) => {
          if (searchSeqRef.current === seq) {
            setSettled({ query: normalizedQuery, results: list })
          }
        })
        .catch((e) => {
          if (searchSeqRef.current !== seq) return
          toast(t("market.registrySearchFailed", { error: String(e) }), "error")
          setSettled({ query: normalizedQuery, results: [] })
        })
    }, 300)
    return () => clearTimeout(timer)
  }, [normalizedQuery, t])

  const results = searchResults(query, settled)
  const searching = isSearchPending(query, settled)

  const isInstalled = (id: string) => {
    const skillName = id.split("/").pop() || ""
    return skills.some((s) => s.name === skillName)
  }

  const updateSkill = async (skill: string) => {
    setUpdating(skill)
    try {
      const result = await invoke<{ installed: string[] }>("update_skill", { skill })
      toast(t("market.updatedToast", { names: result.installed.join(", ") }), "success")
      setUpdates((prev) => prev.filter((u) => u.skill !== skill))
      onInstalled()
    } catch (e) {
      toast(t("market.updateFailed", { error: String(e) }), "error")
    } finally {
      setUpdating(null)
    }
  }

  const openUpdatePreview = async (skill: string) => {
    setPreviewing(skill)
    try {
      setPreview(await invoke<SkillUpdatePreview>("preview_skill_update", { skill }))
    } catch (e) {
      toast(t("market.preview.previewFailed", { error: String(e) }), "error")
    } finally {
      setPreviewing(null)
    }
  }

  const applyPreviewedUpdate = async () => {
    const target = preview
    if (!target) return
    setPreview(null)
    await updateSkill(target.skill)
  }

  const install = async (id: string) => {
    setInstalling(id)
    try {
      const result = await invoke<{ installed: string[]; skipped: string[] }>(
        "install_skill_from_registry",
        { id }
      )
      if (result.installed.length > 0) {
        toast(t("market.installedToast", { names: result.installed.join(", ") }), "success")
        onInstalled()
      }
      if (result.skipped.length > 0) {
        toast(t("market.skippedToast", { names: result.skipped.join(", ") }), "info")
      }
      if (result.installed.length === 0 && result.skipped.length === 0) {
        toast(t("market.nothingToInstall"), "info")
      }
    } catch (e) {
      toast(t("market.installFailed", { error: String(e) }), "error")
    } finally {
      setInstalling(null)
    }
  }

  return (
    <>
      <Dialog open={open} onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="sm:max-w-[560px] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Store size={14} /> {t("market.title")}
          </DialogTitle>
          <DialogDescription>
            {t("market.desc")}{" "}
            <span className="font-mono text-[10px] bg-muted px-1 rounded">~/.agents/skills</span>
          </DialogDescription>
        </DialogHeader>
        {updates.length > 0 && (
          <div className="border border-border rounded-lg p-2.5 space-y-1.5 bg-primary/[0.04]">
            <p className="text-[11px] font-medium text-foreground">
              {t("market.updatesTitle", { count: updates.length })}
            </p>
            {updates.map((u) => (
              <div key={u.skill} className="flex items-center gap-2">
                <span className="text-xs truncate flex-1 min-w-0">{u.skill}</span>
                <span className="text-[10px] text-muted-foreground font-mono shrink-0">
                  {u.installed_commit.slice(0, 7)} → {u.latest_commit.slice(0, 7)}
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  className="h-6 text-xs px-2 shrink-0"
                  disabled={updating === u.skill || previewing === u.skill}
                  onClick={() => void openUpdatePreview(u.skill)}
                >
                  {previewing === u.skill
                    ? t("market.preview.checking")
                    : updating === u.skill
                      ? t("market.updating")
                      : t("market.update")}
                </Button>
              </div>
            ))}
          </div>
        )}
        <div className="flex items-center gap-2 border border-input rounded-lg px-2.5 h-8">
          <Search size={13} className="text-muted-foreground shrink-0" />
          <input
            ref={searchInputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("market.searchPlaceholder")}
            className="w-full bg-transparent outline-none text-xs placeholder:text-muted-foreground"
          />
        </div>
        <div className="max-h-[360px] overflow-y-auto -mx-1 px-1 space-y-1">
          {query.trim().length < 2 ? (
            <p className="text-xs text-muted-foreground text-center py-8">{t("market.typeToSearch")}</p>
          ) : searching && results.length === 0 ? (
            <p className="text-xs text-muted-foreground text-center py-8">{t("market.searching")}</p>
          ) : results.length === 0 ? (
            <p className="text-xs text-muted-foreground text-center py-8">{t("market.noMatches")}</p>
          ) : (
            results.map((r) => {
              const installed = isInstalled(r.id)
              return (
                <div
                  key={r.id}
                  className="flex items-center gap-3 px-3 py-2 rounded-md border border-border/60 hover:bg-accent/40"
                >
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium truncate">{r.name}</div>
                    <div className="text-[10px] text-muted-foreground font-mono truncate">{r.source}</div>
                  </div>
                  <span className="text-[10px] text-muted-foreground shrink-0" title={t("market.installs")}>
                    ↓ {formatInstalls(r.installs)}
                  </span>
                  <Button
                    size="sm"
                    variant={installed ? "ghost" : "outline"}
                    className="h-6 text-xs px-2 shrink-0"
                    disabled={installed || installing === r.id}
                    onClick={() => void install(r.id)}
                    title={installed ? "Already installed" : undefined}
                  >
                    {installed ? (
                      <><Check size={11} /> {t("market.installed")}</>
                    ) : installing === r.id ? (
                      t("market.installing")
                    ) : (
                      <><Download size={11} /> {t("market.install")}</>
                    )}
                  </Button>
                </div>
              )
            })
          )}
        </div>
        <p className="text-[10px] text-muted-foreground text-center">
          {t("market.sourceLine")}{" "}
          <a
            href="https://skills.sh"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-0.5 hover:text-foreground"
          >
            {t("market.openSite")} <ExternalLink size={9} />
          </a>
        </p>
      </DialogContent>
      </Dialog>

      <Dialog open={preview !== null} onOpenChange={(o) => { if (!o) setPreview(null) }}>
        <DialogContent className="sm:max-w-[520px]">
          {preview && (() => {
            const summary = summarizeUpdatePreview(preview)
            const from = shortCommit(preview.installed_commit) ?? t("market.preview.unknownCommit")
            const to = shortCommit(preview.latest_commit) ?? t("market.preview.unknownCommit")
            return (
              <>
                <DialogHeader>
                  <DialogTitle>{t("market.preview.title")}</DialogTitle>
                  <DialogDescription>
                    {t("market.preview.desc", { skill: preview.skill, from, to })}
                  </DialogDescription>
                </DialogHeader>
                <div className="max-h-[320px] space-y-3 overflow-y-auto">
                  <div className="flex flex-wrap gap-1.5">
                    <Badge variant="secondary">{t("market.preview.addedTitle", { count: summary.added })}</Badge>
                    <Badge variant="secondary">{t("market.preview.changedTitle", { count: summary.changed })}</Badge>
                    <Badge variant="secondary">{t("market.preview.removedTitle", { count: summary.removed })}</Badge>
                  </div>
                  {preview.local_modified && (
                    <p className="text-[11px] text-amber-500 break-words">{t("market.preview.localModified")}</p>
                  )}
                  {!summary.hasChanges && (
                    <p className="text-[11px] text-muted-foreground">{t("market.preview.noChanges")}</p>
                  )}
                  <PreviewGroup
                    title={t("market.preview.addedTitle", { count: summary.added })}
                    files={preview.added}
                    tone="text-emerald-500"
                  />
                  <PreviewGroup
                    title={t("market.preview.changedTitle", { count: summary.changed })}
                    files={preview.changed}
                    tone="text-amber-500"
                  />
                  <PreviewGroup
                    title={t("market.preview.removedTitle", { count: summary.removed })}
                    files={preview.removed}
                    tone="text-destructive"
                  />
                </div>
                <DialogFooter>
                  <Button variant="ghost" size="sm" onClick={() => setPreview(null)}>
                    {t("common.cancel")}
                  </Button>
                  <Button size="sm" disabled={updating !== null} onClick={() => void applyPreviewedUpdate()}>
                    {updating !== null ? t("market.updating") : t("market.preview.updateNow")}
                  </Button>
                </DialogFooter>
              </>
            )
          })()}
        </DialogContent>
      </Dialog>
    </>
  )
}

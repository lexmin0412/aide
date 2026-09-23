import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Search, Download, Check, Store, ExternalLink } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { toast } from "@/lib/toast"
import { useTranslation } from "react-i18next"
import type { SkillInfo } from "../types"

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

export function MarketDialog({ open, skills, onClose, onInstalled }: MarketDialogProps) {
  const { t } = useTranslation()
  const [query, setQuery] = useState("")
  const [results, setResults] = useState<RegistrySkill[]>([])
  const [searching, setSearching] = useState(false)
  const [installing, setInstalling] = useState<string | null>(null)
  const [updates, setUpdates] = useState<SkillUpdateInfo[]>([])
  const [updating, setUpdating] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    setQuery("")
    setResults([])
    invoke<SkillUpdateInfo[]>("check_skill_updates")
      .then(setUpdates)
      .catch(() => {})
  }, [open])

  useEffect(() => {
    const q = query.trim()
    if (q.length < 2) {
      setResults([])
      setSearching(false)
      return
    }
    setSearching(true)
    const timer = setTimeout(() => {
      invoke<RegistrySkill[]>("search_registry", { query: q })
        .then(setResults)
        .catch((e) => {
          toast(t("market.registrySearchFailed", { error: String(e) }), "error")
          setResults([])
        })
        .finally(() => setSearching(false))
    }, 300)
    return () => clearTimeout(timer)
  }, [query])

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
                  disabled={updating === u.skill}
                  onClick={() => void updateSkill(u.skill)}
                >
                  {updating === u.skill ? t("market.updating") : t("market.update")}
                </Button>
              </div>
            ))}
          </div>
        )}
        <div className="flex items-center gap-2 border border-input rounded-lg px-2.5 h-8">
          <Search size={13} className="text-muted-foreground shrink-0" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("market.searchPlaceholder")}
            autoFocus
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
            open site <ExternalLink size={9} />
          </a>
        </p>
      </DialogContent>
    </Dialog>
  )
}

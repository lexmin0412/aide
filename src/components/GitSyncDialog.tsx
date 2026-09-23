import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { GitBranch, CloudUpload, CloudDownload, Plus, Trash2, Star } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { toast } from "@/lib/toast"
import { useTranslation } from "react-i18next"
import type { GitRemotesConfig, SkillInfo } from "../types"

interface GitSyncDialogProps {
  open: boolean
  skills: SkillInfo[]
  onClose: () => void
  /** Called when a pull changed the local skills store. */
  onChanged: () => void
}

interface ScopeResult {
  kind: "publish" | "pull"
  ok: boolean
  text: string
}

interface NewRemoteForm {
  name: string
  url: string
  branch: string
}

const EMPTY_FORM: NewRemoteForm = { name: "", url: "", branch: "main" }

export function GitSyncDialog({ open, skills, onClose, onChanged }: GitSyncDialogProps) {
  const { t } = useTranslation()
  const [config, setConfig] = useState<GitRemotesConfig>({ remotes: [], skill_scopes: {} })
  const [form, setForm] = useState<NewRemoteForm>(EMPTY_FORM)
  const [adding, setAdding] = useState(false)
  const [busyScope, setBusyScope] = useState<string | null>(null)
  const [results, setResults] = useState<Record<string, ScopeResult>>({})

  useEffect(() => {
    if (!open) return
    invoke<GitRemotesConfig>("get_git_config")
      .then(setConfig)
      .catch((e) => toast(t("git.loadFailed", { error: String(e) }), "error"))
  }, [open])

  const defaultScope = config.remotes.find((r) => r.is_default)?.name ?? config.remotes[0]?.name ?? null

  const effectiveScope = (skill: SkillInfo): string | null => {
    if (skill.scope && config.remotes.some((r) => r.name === skill.scope)) return skill.scope
    return defaultScope
  }

  const addRemote = async () => {
    if (adding) return
    const next: GitRemotesConfig = {
      ...config,
      remotes: [
        ...config.remotes,
        {
          name: form.name.trim(),
          url: form.url.trim(),
          branch: form.branch.trim() || "main",
          is_default: config.remotes.length === 0,
        },
      ],
    }
    setAdding(true)
    try {
      await invoke("save_git_config", { config: next })
      setConfig(next)
      setForm(EMPTY_FORM)
      toast(t("git.enabledToast"), "success")
    } catch (e) {
      toast(t("git.saveFailed", { error: String(e) }), "error")
    } finally {
      setAdding(false)
    }
  }

  const removeRemote = async (name: string) => {
    const next: GitRemotesConfig = {
      ...config,
      remotes: config.remotes.filter((r) => r.name !== name),
    }
    try {
      await invoke("save_git_config", { config: next })
      setConfig(next)
      toast(t("git.removedToast"), "success")
    } catch (e) {
      toast(t("git.removeFailed", { error: String(e) }), "error")
    }
  }

  const setSkillScope = async (skill: string, scope: string | null) => {
    try {
      await invoke("set_skill_scope", { skill, scope })
      const nextScopes = { ...config.skill_scopes }
      if (scope) nextScopes[skill] = scope
      else delete nextScopes[skill]
      setConfig({ ...config, skill_scopes: nextScopes })
    } catch (e) {
      toast(t("git.scopeFailed", { error: String(e) }), "error")
    }
  }

  const publish = async (scope: string) => {
    setBusyScope(scope)
    try {
      const r = await invoke<{ committed: boolean; pushed: boolean; skills: string[] }>(
        "publish_scope",
        { scope }
      )
      const text = r.committed
        ? t("git.committed", { count: r.skills.length })
        : t("git.noChanges", { count: r.skills.length })
      setResults((prev) => ({ ...prev, [scope]: { kind: "publish", ok: true, text } }))
    } catch (e) {
      setResults((prev) => ({ ...prev, [scope]: { kind: "publish", ok: false, text: String(e) } }))
    } finally {
      setBusyScope(null)
    }
  }

  const pull = async (scope: string) => {
    setBusyScope(scope)
    try {
      const r = await invoke<{ updated: string[] }>("pull_scope", { scope })
      const text =
        r.updated.length > 0
          ? t("git.pulled", { count: r.updated.length })
          : t("git.pullEmpty")
      setResults((prev) => ({ ...prev, [scope]: { kind: "pull", ok: true, text } }))
      onChanged()
    } catch (e) {
      setResults((prev) => ({ ...prev, [scope]: { kind: "pull", ok: false, text: String(e) } }))
    } finally {
      setBusyScope(null)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="sm:max-w-[600px] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <GitBranch size={14} /> {t("git.title")}
          </DialogTitle>
          <DialogDescription>
            {t("git.desc")}
          </DialogDescription>
        </DialogHeader>

        <div className="max-h-[440px] overflow-y-auto -mx-1 px-1 space-y-3">
          {config.remotes.length === 0 && (
            <p className="text-xs text-muted-foreground bg-muted/40 border border-border rounded-md p-3">
              {t("git.noScopes")}
            </p>
          )}

          {config.remotes.map((r) => {
            const inScope = skills.filter((s) => effectiveScope(s) === r.name)
            const result = results[r.name]
            const busy = busyScope === r.name
            return (
              <div key={r.name} className="border border-border rounded-lg p-3 space-y-2">
                <div className="flex items-center gap-2 min-w-0">
                  <span className="text-sm font-medium">{r.name}</span>
                  {r.is_default && (
                    <span className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded bg-secondary text-[10px] text-muted-foreground">
                      <Star size={9} /> default
                    </span>
                  )}
                  <span className="text-[10px] text-muted-foreground font-mono truncate flex-1 min-w-0" title={r.url}>
                    {r.url}
                  </span>
                  <span className="text-[10px] text-muted-foreground shrink-0">
                    {t("git.skills", { count: inScope.length })}
                  </span>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 shrink-0 text-muted-foreground hover:text-destructive"
                    title={t("git.remove")}
                    onClick={() => void removeRemote(r.name)}
                  >
                    <Trash2 size={12} />
                  </Button>
                </div>
                <div className="flex items-center gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    className="h-6 text-xs px-2"
                    disabled={busy}
                    onClick={() => void publish(r.name)}
                  >
                    <CloudUpload size={12} /> {busy ? "..." : t("git.publish")}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-6 text-xs px-2"
                    disabled={busy}
                    onClick={() => void pull(r.name)}
                  >
                    <CloudDownload size={12} /> {t("git.pull")}
                  </Button>
                  {result && (
                    <span
                      className={`text-[10px] truncate flex-1 min-w-0 ${result.ok ? "text-emerald-400" : "text-destructive"}`}
                      title={result.text}
                    >
                      {result.text}
                    </span>
                  )}
                </div>
              </div>
            )
          })}

          {config.remotes.length < 8 && (
            <div className="border border-dashed border-border rounded-lg p-3 space-y-2">
              <Label className="text-[11px] text-muted-foreground">{t("git.addScope")}</Label>
              <div className="flex gap-2">
                <Input
                  autoFocus
                  value={form.name}
                  onChange={(e) => setForm({ ...form, name: e.target.value })}
                  placeholder={t("git.scopePlaceholder")}
                  className="h-7 text-xs flex-1"
                />
                <Input
                  value={form.url}
                  onChange={(e) => setForm({ ...form, url: e.target.value })}
                  placeholder={t("git.urlPlaceholder")}
                  className="h-7 text-xs flex-[2] font-mono"
                />
                <Input
                  value={form.branch}
                  onChange={(e) => setForm({ ...form, branch: e.target.value })}
                  placeholder={t("git.branchPlaceholder")}
                  className="h-7 text-xs w-20"
                />
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 text-xs px-2 shrink-0"
                  disabled={!form.name.trim() || !form.url.trim() || adding}
                  onClick={() => void addRemote()}
                >
                  <Plus size={12} /> {t("git.add")}
                </Button>
              </div>
            </div>
          )}

          {config.remotes.length > 0 && skills.length > 0 && (
            <div className="border border-border rounded-lg p-3 space-y-1.5">
              <p className="text-[11px] text-muted-foreground">
                {t("git.skillScopes")}
                {defaultScope && <span className="font-mono"> ({defaultScope})</span>}
              </p>
              <div className="max-h-[180px] overflow-y-auto space-y-0.5">
                {skills.map((s) => (
                  <div key={s.path} className="flex items-center gap-2">
                    <span className="text-xs truncate flex-1 min-w-0" title={s.name}>
                      {s.display_name}
                    </span>
                    <select
                      value={s.scope ?? ""}
                      onChange={(e) => void setSkillScope(s.name, e.target.value || null)}
                      className="h-6 text-[11px] rounded-md border border-input bg-transparent px-1.5 outline-none"
                    >
                      <option value="">{t("git.defaultOption")}</option>
                      {config.remotes.map((r) => (
                        <option key={r.name} value={r.name}>
                          {r.name}
                        </option>
                      ))}
                    </select>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

import { useState, useEffect, useMemo } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Plus, RefreshCw, Pencil, Trash2, KeyRound, Search } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { toast } from "@/lib/toast"
import { useTranslation } from "react-i18next"
import { CardGridSkeleton } from "./Skeleton"
import { SearchSelect } from "./SearchSelect"
import type { ModelProfile, ModelProfilesConfig, ProviderSummary } from "../types"

const TARGETS: { key: string; name: string; hint: string }[] = [
  { key: "claude_code", name: "Claude Code", hint: "settings.json env" },
  { key: "opencode", name: "OpenCode", hint: "provider config" },
  { key: "codex", name: "Codex", hint: "config.toml + env_key" },
]

interface ModelSyncResult {
  tool: string
  ok: boolean
  message: string
}

interface ProfileSyncResult {
  profile_id: string
  profile_name: string
  results: ModelSyncResult[]
}

const EMPTY_FORM: ModelProfile = {
  id: "",
  name: "",
  provider: "",
  base_url: "",
  api_key: "",
  model: "",
  models: [],
  targets: ["claude_code"],
}

function maskKey(key: string): string {
  if (key.length <= 8) return "••••••••"
  return `${key.slice(0, 3)}••••${key.slice(-4)}`
}

export default function ModelPage() {
  const { t } = useTranslation()
  const [profiles, setProfiles] = useState<ModelProfile[]>([])
  const [providers, setProviders] = useState<ProviderSummary[]>([])
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState<ModelProfile | null>(null)
  const [syncing, setSyncing] = useState<string | null>(null)
  const [results, setResults] = useState<Record<string, ModelSyncResult[]>>({})

  const load = () => {
    return Promise.all([
      invoke<ModelProfilesConfig>("list_model_profiles").then((p) => setProfiles(p.profiles ?? [])),
      invoke<ProviderSummary[]>("list_model_providers", { force: false }).then(setProviders),
    ])
  }

  useEffect(() => {
    load()
      .catch((e) => {
        console.error("Failed to load models:", e)
        toast(t("models.loadFailed", { error: String(e) }), "error")
      })
      .finally(() => setLoading(false))
  }, [])

  const saveProfiles = async (next: ModelProfile[]) => {
    await invoke("save_model_profiles", { config: { profiles: next } })
    setProfiles(next)
  }

  const removeProfile = async (id: string) => {
    try {
      await saveProfiles(profiles.filter((p) => p.id !== id))
      toast(t("models.deletedToast"), "success")
    } catch (e) {
      toast(t("models.deleteFailed", { error: String(e) }), "error")
    }
  }

  const syncProfile = async (profile: ModelProfile) => {
    setSyncing(profile.id)
    try {
      const r = await invoke<ModelSyncResult[]>("sync_model_profile", { profileId: profile.id })
      setResults((prev) => ({ ...prev, [profile.id]: r }))
      const ok = r.filter((x) => x.ok).length
      toast(t("models.syncToast", { name: profile.name, ok, total: r.length }), ok > 0 ? "success" : "info")
    } catch (e) {
      toast(t("models.syncFailed", { error: String(e) }), "error")
    } finally {
      setSyncing(null)
    }
  }

  const syncAll = async () => {
    setSyncing("__all__")
    try {
      const r = await invoke<ProfileSyncResult[]>("sync_all_model_profiles")
      const byProfile: Record<string, ModelSyncResult[]> = {}
      for (const p of r) byProfile[p.profile_id] = p.results
      setResults((prev) => ({ ...prev, ...byProfile }))
      const targets = r.reduce((n, p) => n + p.results.filter((x) => x.ok).length, 0)
      toast(t("models.syncAllToast", { count: targets, profiles: r.length }), "success")
    } catch (e) {
      toast(t("models.syncFailed", { error: String(e) }), "error")
    } finally {
      setSyncing(null)
    }
  }

  const providerName = (id: string) => providers.find((p) => p.id === id)?.name ?? id

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-4 px-6 pt-5 pb-4 border-b border-border shrink-0">
        <div className="flex-1 min-w-0">
          <h1 className="text-lg font-semibold tracking-tight">Models</h1>
          <p className="text-xs text-muted-foreground mt-0.5">
            {t("models.count", { count: profiles.length })}
          </p>
        </div>
        <Button variant="outline" size="sm" disabled={syncing !== null || profiles.length === 0} onClick={() => void syncAll()}>
          <RefreshCw size={13} className={syncing === "__all__" ? "animate-spin" : ""} /> {t("models.syncAll")}
        </Button>
        <Button
          size="sm"
          onClick={() => {
            setEditing({ ...EMPTY_FORM, id: crypto.randomUUID() })
          }}
        >
          <Plus size={13} /> {t("models.add")}
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {loading ? (
          <CardGridSkeleton count={3} />
        ) : profiles.length === 0 ? (
          <div className="h-full flex flex-col items-center justify-center gap-3 text-center">
            <KeyRound size={36} className="text-primary/40" />
            <div>
              <p className="text-sm font-medium">{t("models.empty.title")}</p>
              <p className="text-xs text-muted-foreground mt-1 max-w-[400px]">
                {t("models.empty.desc")}
              </p>
            </div>
            <Button
              size="sm"
              onClick={() => setEditing({ ...EMPTY_FORM, id: crypto.randomUUID() })}
            >
              <Plus size={13} /> {t("models.empty.cta")}
            </Button>
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-3">
            {profiles.map((p) => {
              const r = results[p.id]
              return (
                <Card key={p.id} className="p-4 flex flex-col gap-2.5 min-h-[150px]">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0 flex-1">
                      <div className="text-sm font-semibold truncate">{p.name}</div>
                      <div className="text-[11px] text-muted-foreground truncate">
                        {providerName(p.provider)}
                        {p.base_url && <span className="font-mono"> · {p.base_url}</span>}
                      </div>
                    </div>
                    <div className="flex gap-1 shrink-0">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t("models.edit")}
                        onClick={() => setEditing({ ...p })}
                      >
                        <Pencil className="size-3" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-destructive hover:text-destructive"
                        title={t("common.delete")}
                        onClick={() => void removeProfile(p.id)}
                      >
                        <Trash2 className="size-3" />
                      </Button>
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-1.5 text-[11px] text-muted-foreground">
                    <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-muted font-mono">
                      <KeyRound size={9} /> {maskKey(p.api_key)}
                    </span>
                    {p.model && <Badge variant="secondary" className="text-[10px]">{p.model}</Badge>}
                    {p.models.length > 0 && (
                      <span>{p.models.length} model{p.models.length > 1 ? "s" : ""}</span>
                    )}
                  </div>
                  <div className="flex flex-wrap gap-1">
                    {p.targets.map((t) => (
                      <Badge key={t} variant="outline" className="text-[9px] px-1 py-0">
                        {TARGETS.find((x) => x.key === t)?.name ?? t}
                      </Badge>
                    ))}
                  </div>
                  {r && (
                    <div className="space-y-0.5 text-[10px]">
                      {r.map((x) => (
                        <div
                          key={x.tool}
                          className={x.ok ? "text-emerald-400" : "text-amber-400"}
                          title={x.message}
                        >
                          {TARGETS.find((t) => t.key === x.tool)?.name ?? x.tool}: {x.ok ? "synced" : x.message}
                        </div>
                      ))}
                    </div>
                  )}
                  <div className="mt-auto flex justify-end">
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-6 text-xs px-2"
                      disabled={syncing !== null || p.targets.length === 0}
                      title={p.targets.length === 0 ? t("models.noTargets") : undefined}
                      onClick={() => void syncProfile(p)}
                    >
                      {syncing === p.id ? "Syncing..." : "Sync"}
                    </Button>
                  </div>
                </Card>
              )
            })}
          </div>
        )}
      </div>

      {editing && (
        <ProfileEditor
          profile={editing}
          providers={providers}
          providersReady={!loading}
          onClose={() => setEditing(null)}
          onSave={async (p) => {
            const next = profiles.some((x) => x.id === p.id)
              ? profiles.map((x) => (x.id === p.id ? p : x))
              : [...profiles, p]
            try {
              await saveProfiles(next)
              toast(t("models.savedToast", { name: p.name }), "success")
              setEditing(null)
            } catch (e) {
              toast(t("models.saveFailed", { e: String(e) }), "error")
            }
          }}
        />
      )}
    </div>
  )
}

function ProfileEditor({
  profile,
  providers,
  providersReady,
  onClose,
  onSave,
}: {
  profile: ModelProfile
  providers: ProviderSummary[]
  providersReady: boolean
  onClose: () => void
  onSave: (p: ModelProfile) => Promise<void>
}) {
  const { t } = useTranslation()
  const [form, setForm] = useState<ModelProfile>(profile)
  const [saving, setSaving] = useState(false)
  const [modelFilter, setModelFilter] = useState("")

  const selectedProvider = providers.find((p) => p.id === form.provider)
  const providerModels = useMemo(() => {
    const list = selectedProvider?.models ?? []
    const q = modelFilter.trim().toLowerCase()
    return q ? list.filter((m) => m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q)) : list
  }, [selectedProvider, modelFilter])

  const set = (patch: Partial<ModelProfile>) => setForm((prev) => ({ ...prev, ...patch }))

  const selectAllVisible = () => {
    set({
      models: Array.from(new Set([...form.models, ...providerModels.map((m) => m.id)])),
    })
  }

  const invertVisible = () => {
    const selected = new Set(form.models)
    for (const id of providerModels.map((m) => m.id)) {
      if (selected.has(id)) selected.delete(id)
      else selected.add(id)
    }
    set({ models: Array.from(selected) })
  }

  const toggle = (list: string[], value: string): string[] =>
    list.includes(value) ? list.filter((x) => x !== value) : [...list, value]

  const submit = async () => {
    if (!form.name.trim() || !form.provider.trim() || !form.api_key.trim()) return
    setSaving(true)
    await onSave({ ...form, name: form.name.trim(), provider: form.provider.trim() })
    setSaving(false)
  }

  return (
    <Dialog open onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="sm:max-w-[560px] flex flex-col">
        <DialogHeader>
          <DialogTitle>{profile.name ? t("models.editor.editTitle") : t("models.editor.addTitle")}</DialogTitle>
          <DialogDescription>
            {t("models.editor.desc")}
          </DialogDescription>
        </DialogHeader>
        <div className="overflow-y-auto max-h-[420px] space-y-3 pr-1">
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <Label>{t("models.editor.name")}</Label>
              <Input
                value={form.name}
                onChange={(e) => set({ name: e.target.value })}
                placeholder={t("models.editor.nameExample")}
              />
            </div>
            <div className="space-y-1">
              <Label>{t("models.editor.provider")}</Label>
              {providers.length > 0 ? (
                <SearchSelect
                  value={form.provider}
                  placeholder={t("models.editor.providerPlaceholder")}
                  emptyText={t("models.editor.noModelsMatch")}
                  options={providers.map((p) => ({
                    value: p.id,
                    label: p.name,
                    hint: `${p.models.length}`,
                  }))}
                  onChange={(v) =>
                    set({
                      provider: v,
                      models: [],
                      model: "",
                      base_url:
                        providers.find((p) => p.id === v)?.base_url ?? "",
                    })
                  }
                />
              ) : providersReady ? (
                <Input
                  value={form.provider}
                  onChange={(e) => set({ provider: e.target.value })}
                  placeholder={t("models.editor.providerOffline")}
                />
              ) : (
                <div className="h-8 w-full rounded-lg border border-input bg-transparent px-2 text-xs text-muted-foreground flex items-center">
                  {t("models.editor.loadingProviders")}
                </div>
              )}
            </div>
          </div>
          <div className="space-y-1">
            <Label>{t("models.editor.baseUrl")}</Label>
            <Input
              value={form.base_url}
              onChange={(e) => set({ base_url: e.target.value })}
              placeholder="https://api.openai.com/v1"
              className="font-mono text-xs"
            />
          </div>
          <div className="space-y-1">
            <Label>{t("models.editor.apiKey")}</Label>
            <Input
              type="password"
              value={form.api_key}
              onChange={(e) => set({ api_key: e.target.value })}
              placeholder="sk-..."
              className="font-mono text-xs"
            />
          </div>

          {selectedProvider && selectedProvider.models.length > 0 && (
            <>
              <div className="space-y-1">
                <div className="flex items-center justify-between">
                  <Label>{t("models.editor.modelsExposed", { count: form.models.length })}</Label>
                  <div className="flex gap-1">
                    <button
                      className="px-1.5 h-5 rounded text-[10px] text-muted-foreground hover:text-foreground hover:bg-muted"
                      onClick={selectAllVisible}
                    >
                      {t("models.editor.all")}
                    </button>
                    <button
                      className="px-1.5 h-5 rounded text-[10px] text-muted-foreground hover:text-foreground hover:bg-muted"
                      onClick={invertVisible}
                    >
                      {t("models.editor.invert")}
                    </button>
                  </div>
                </div>
                <div className="relative">
                  <Search size={12} className="absolute left-2 top-2 text-muted-foreground" />
                  <Input
                    value={modelFilter}
                    onChange={(e) => setModelFilter(e.target.value)}
                    placeholder={t("models.editor.modelSearch")}
                    className="h-7 text-xs pl-7"
                  />
                </div>
                <div className="max-h-[140px] overflow-y-auto rounded-lg border border-border divide-y divide-border/60">
                  {providerModels.length === 0 ? (
                    <p className="text-xs text-muted-foreground text-center py-4">{t("models.editor.noModelsMatch")}</p>
                  ) : (
                    providerModels.map((m) => (
                      <label
                        key={m.id}
                        className="flex items-center gap-2 px-2.5 py-1.5 text-xs cursor-pointer hover:bg-accent/50"
                      >
                        <input
                          type="checkbox"
                          className="accent-[var(--primary)]"
                          checked={form.models.includes(m.id)}
                          onChange={() => set({ models: toggle(form.models, m.id) })}
                        />
                        <span className="truncate flex-1">{m.name}</span>
                        {m.context && (
                          <span className="text-[10px] text-muted-foreground font-mono shrink-0">
                            {t("models.editor.ctx", { count: (m.context / 1000).toFixed(0) })}
                          </span>
                        )}
                      </label>
                    ))
                  )}
                </div>
              </div>
              <div className="space-y-1">
                <Label>{t("models.editor.defaultModel")}</Label>
                <SearchSelect
                  value={form.model}
                  placeholder={t("models.editor.none")}
                  emptyText={t("models.editor.selectModelsFirst")}
                  options={form.models.map((m) => ({ value: m, label: m }))}
                  onChange={(v) => set({ model: v })}
                />
              </div>
            </>
          )}

          <div className="space-y-1">
            <Label>{t("models.editor.syncTargets")}</Label>
            <div className="flex flex-wrap gap-2">
              {TARGETS.map((tg) => (
                <label
                  key={tg.key}
                  className={`flex items-center gap-1.5 text-xs px-2.5 py-1.5 rounded-lg border cursor-pointer transition-colors ${
                    form.targets.includes(tg.key)
                      ? "border-primary/50 bg-primary/[0.07]"
                      : "border-border hover:border-primary/30"
                  }`}
                >
                  <input
                    type="checkbox"
                    className="accent-[var(--primary)]"
                    checked={form.targets.includes(tg.key)}
                    onChange={() => set({ targets: toggle(form.targets, tg.key) })}
                  />
                  <span className="font-medium">{tg.name}</span>
                  <span className="text-[10px] text-muted-foreground">{t(`models.editor.targetHint.${tg.key}`)}</span>
                </label>
              ))}
            </div>
            <p className="text-[10px] text-muted-foreground">
              {t("models.editor.otherTools")}
            </p>
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            size="sm"
            disabled={!form.name.trim() || !form.provider.trim() || !form.api_key.trim() || saving}
            onClick={() => void submit()}
          >
            {saving ? t("common.saving") : t("common.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

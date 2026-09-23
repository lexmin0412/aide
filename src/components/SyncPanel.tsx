import { useState, useEffect } from "react"
import { useTranslation } from "react-i18next"
import { invoke } from "@tauri-apps/api/core"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"

interface ToolInfo {
  key: string; name: string; global_skills: string; status: string
}

interface SyncResult {
  key: string; name: string; success: boolean
  merged: string[]; backed_up: string[]; conflicts: string[]; error: string | null
}

interface SyncPanelProps {
  open: boolean
  onClose: () => void
}

const STATUS_KEYS: Record<string, string> = {
  not_installed: "sync.status.notInstalled",
  compatible: "sync.status.native",
  synced: "sync.status.linked",
  has_content: "sync.status.pendingMerge",
  ready: "sync.status.ready",
}

const STATUS_CLASS: Record<string, string> = {
  not_installed: "text-muted-foreground",
  compatible: "text-emerald-400",
  synced: "text-emerald-400",
  has_content: "text-amber-400",
  ready: "text-primary",
}

export function SyncPanel({ open, onClose }: SyncPanelProps) {
  const { t } = useTranslation()
  const [tools, setTools] = useState<ToolInfo[]>([])
  const [syncing, setSyncing] = useState<Set<string>>(new Set())
  const [results, setResults] = useState<Record<string, SyncResult>>({})

  const refresh = async () => {
    const list = await invoke<ToolInfo[]>("check_sync_statuses")
    setTools(list)
  }
  useEffect(() => { if (open) refresh() }, [open])

  const syncOne = async (key: string) => {
    setSyncing((prev) => new Set(prev).add(key))
    try {
      const result = await invoke<SyncResult>("sync_tool", { toolKey: key })
      setResults((prev) => ({ ...prev, [key]: result }))
    } catch (e) {
      setResults((prev) => ({
        ...prev,
        [key]: { key, name: key, success: false, merged: [], backed_up: [], conflicts: [], error: String(e) },
      }))
    } finally {
      setSyncing((prev) => { const next = new Set(prev); next.delete(key); return next })
      refresh()
    }
  }
  const syncAll = async () => {
    const toSync = tools.filter((t) => t.status === "has_content" || t.status === "ready")
    for (const t of toSync) await syncOne(t.key)
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle>{t("sync.title")}</DialogTitle>
          <DialogDescription>{t("sync.desc")}</DialogDescription>
        </DialogHeader>
        <div className="flex gap-2">
          <Button size="sm" onClick={syncAll}>{t("sync.syncAll")}</Button>
          <Button size="sm" variant="ghost" onClick={refresh}>{t("sync.refresh")}</Button>
        </div>
        <div className="max-h-[420px] overflow-y-auto -mx-1 px-1 space-y-2">
          {tools.map((tool) => {
            const st = {
              label: t(STATUS_KEYS[tool.status] ?? "sync.status.ready"),
              className: STATUS_CLASS[tool.status] ?? "",
            }
            const result = results[tool.key]
            const isSyncing = syncing.has(tool.key)
            const canSync = tool.status === "has_content" || tool.status === "ready"
            return (
              <div key={tool.key} className={`px-3 py-2.5 rounded-md border border-border ${isSyncing ? "opacity-50" : ""}`}>
                <div className="flex items-center justify-between gap-3">
                  <div className="flex items-center gap-2 min-w-0 flex-1">
                    <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${
                      tool.status === "synced" || tool.status === "compatible" ? "bg-emerald-400" :
                      tool.status === "has_content" ? "bg-amber-400" :
                      tool.status === "ready" ? "bg-primary" : "bg-muted-foreground/50"
                    }`} />
                    <div className="min-w-0">
                      <div className="text-sm font-medium">{tool.name}</div>
                      <div className="text-[10px] text-muted-foreground font-mono truncate">{tool.global_skills}</div>
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <span className={`text-[11px] ${st.className}`}>{st.label}</span>
                    {canSync && !result?.success && (
                      <Button size="sm" variant="outline" className="h-6 text-xs px-2" disabled={isSyncing} onClick={() => syncOne(tool.key)}>
                        {isSyncing ? "..." : "Sync"}
                      </Button>
                    )}
                  </div>
                </div>
                {result && (
                  <div className="mt-1.5 ml-3.5 text-[11px] space-y-0.5">
                    {result.merged.length > 0 && (
                      <div className="text-emerald-400">
                        {t("sync.merged", { count: result.merged.length })}
                      </div>
                    )}
                    {!result.merged.length && !result.backed_up.length && !result.conflicts.length && !result.error && (
                      <div className="text-emerald-400">{t("sync.linked")}</div>
                    )}
                    {result.backed_up.length > 0 && (
                      <div className="text-amber-400">
                        {t("sync.backedUp", { count: result.backed_up.length, tool: tool.key })}
                      </div>
                    )}
                    {result.conflicts.length > 0 && (
                      <div className="text-amber-400">{t("sync.conflicts", { count: result.conflicts.length })}</div>
                    )}
                    {result.error && <div className="text-red-400">{result.error}</div>}
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </DialogContent>
    </Dialog>
  )
}

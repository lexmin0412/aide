import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { Button } from "@/components/ui/button"

interface ToolInfo {
  key: string; name: string; global_skills: string; status: string
}

interface SyncResult {
  key: string; name: string; success: boolean
  merged: string[]; conflicts: string[]; error: string | null
}

interface SyncPanelProps {
  open: boolean
  onClose: () => void
}

const STATUS_LABELS: Record<string, { label: string; className: string }> = {
  not_installed: { label: "Not Detected", className: "text-muted-foreground" },
  compatible: { label: "Native", className: "text-emerald-400" },
  synced: { label: "Linked", className: "text-emerald-400" },
  has_content: { label: "Pending Merge", className: "text-amber-400" },
  ready: { label: "Ready", className: "text-blue-400" },
}

export function SyncPanel({ open, onClose }: SyncPanelProps) {
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
        [key]: { key, name: key, success: false, merged: [], conflicts: [], error: String(e) },
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

  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm" onClick={onClose}>
      <div className="w-[460px] max-h-[70vh] bg-card border border-border rounded-xl shadow-2xl flex flex-col" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-between px-5 py-4 border-b border-border">
          <div>
            <h2 className="text-sm font-semibold">Sync Skills to Tools</h2>
            <p className="text-[11px] text-muted-foreground mt-0.5">Link <code className="font-mono text-[10px] bg-muted px-1 rounded">~/.agents/skills</code> to each tool</p>
          </div>
          <button className="text-muted-foreground hover:text-foreground" onClick={onClose}>✕</button>
        </div>
        <div className="px-5 py-3 overflow-y-auto flex-1">
          <div className="flex gap-2 mb-3">
            <Button size="sm" onClick={syncAll}>Sync All</Button>
            <Button size="sm" variant="ghost" onClick={refresh}>Refresh</Button>
          </div>
          <div className="space-y-2">
            {tools.map((tool) => {
              const st = STATUS_LABELS[tool.status] || STATUS_LABELS.ready
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
                        tool.status === "ready" ? "bg-blue-400" : "bg-zinc-500"
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
                      {result.success && !result.error && result.merged.length > 0 && (
                        <span className="text-emerald-400">Merged {result.merged.length} skill{result.merged.length > 1 ? "s" : ""}</span>
                      )}
                      {result.success && !result.error && result.merged.length === 0 && (
                        <span className="text-emerald-400">Linked</span>
                      )}
                      {result.conflicts.length > 0 && (
                        <span className="text-amber-400">{result.conflicts.length} conflict{result.conflicts.length > 1 ? "s" : ""}</span>
                      )}
                      {result.error && <span className="text-red-400">{result.error}</span>}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        </div>
      </div>
    </div>
  )
}

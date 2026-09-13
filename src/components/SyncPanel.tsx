import { useState, useEffect } from "react"
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

const STATUS_LABELS: Record<string, { label: string; className: string }> = {
  not_installed: { label: "Not Detected", className: "text-muted-foreground" },
  compatible: { label: "Native", className: "text-emerald-400" },
  synced: { label: "Linked", className: "text-emerald-400" },
  has_content: { label: "Pending Merge", className: "text-amber-400" },
  ready: { label: "Ready", className: "text-primary" },
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
          <DialogTitle>Sync Skills to Tools</DialogTitle>
          <DialogDescription>
            Link <code className="font-mono text-[10px] bg-muted px-1 rounded">~/.agents/skills</code> to each
            tool. Conflicting tool-side copies are backed up to{" "}
            <code className="font-mono text-[10px] bg-muted px-1 rounded">~/.aide/sync-backup</code>.
          </DialogDescription>
        </DialogHeader>
        <div className="flex gap-2">
          <Button size="sm" onClick={syncAll}>Sync All</Button>
          <Button size="sm" variant="ghost" onClick={refresh}>Refresh</Button>
        </div>
        <div className="max-h-[420px] overflow-y-auto -mx-1 px-1 space-y-2">
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
                        Merged {result.merged.length} skill{result.merged.length > 1 ? "s" : ""} into ~/.agents/skills
                      </div>
                    )}
                    {!result.merged.length && !result.backed_up.length && !result.conflicts.length && !result.error && (
                      <div className="text-emerald-400">Linked</div>
                    )}
                    {result.backed_up.length > 0 && (
                      <div className="text-amber-400">
                        {result.backed_up.length} conflicting skill{result.backed_up.length > 1 ? "s" : ""} backed up to ~/.aide/sync-backup/{tool.key}
                      </div>
                    )}
                    {result.conflicts.length > 0 && (
                      <div className="text-amber-400">{result.conflicts.length} conflict{result.conflicts.length > 1 ? "s" : ""}</div>
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

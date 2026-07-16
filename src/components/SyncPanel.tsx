import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"

interface ToolInfo {
  key: string
  name: string
  global_skills: string
  status: string
}

interface SyncResult {
  key: string
  name: string
  success: boolean
  merged: string[]
  conflicts: string[]
  error: string | null
}

interface SyncPanelProps {
  open: boolean
  onClose: () => void
}

function StatusDot({ status }: { status: string }) {
  const colors: Record<string, string> = {
    compatible: "#5cb87a",
    synced: "#5cb87a",
    has_content: "#d4a157",
    ready: "#5b9bd5",
    not_installed: "#666",
  }
  return (
    <span
      className="status-dot"
      style={{ background: colors[status] || "#666" }}
    />
  )
}

function StatusText({ status }: { status: string }) {
  const labels: Record<string, string> = {
    compatible: "Native support",
    synced: "Linked",
    has_content: "Pending merge",
    ready: "Ready",
    not_installed: "Not detected",
  }
  return <span className="status-text">{labels[status] || status}</span>
}

export function SyncPanel({ open, onClose }: SyncPanelProps) {
  const [tools, setTools] = useState<ToolInfo[]>([])
  const [syncing, setSyncing] = useState<Set<string>>(new Set())
  const [results, setResults] = useState<Record<string, SyncResult>>({})
  const [visible, setVisible] = useState(false)

  const refresh = async () => {
    const list = await invoke<ToolInfo[]>("check_sync_statuses")
    setTools(list)
  }

  useEffect(() => {
    if (open) {
      setVisible(true)
      refresh()
    } else {
      const t = setTimeout(() => setVisible(false), 200)
      return () => clearTimeout(t)
    }
  }, [open])

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
      setSyncing((prev) => {
        const next = new Set(prev)
        next.delete(key)
        return next
      })
      refresh()
    }
  }

  const syncAll = async () => {
    const toSync = tools.filter((t) => t.status === "has_content" || t.status === "ready")
    for (const t of toSync) {
      await syncOne(t.key)
    }
  }

  if (!visible) return null

  const needsSync = tools.filter((t) => t.status === "has_content" || t.status === "ready")

  return (
    <div className={`sync-overlay ${open ? "entering" : "exiting"}`} onClick={onClose}>
      <div className="sync-sheet" onClick={(e) => e.stopPropagation()}>
        <div className="sync-sheet-top">
          <div className="sync-sheet-header">
            <div>
              <h2 className="sync-sheet-title">Sync</h2>
              <p className="sync-sheet-subtitle">
                Link skills from <code>~/.agents/skills</code> to each tool
              </p>
            </div>
            <button className="sync-sheet-close" onClick={onClose}>
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none">
                <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              </svg>
            </button>
          </div>

          {needsSync.length > 0 && (
            <button className="sync-all-btn" onClick={syncAll}>
              Sync all ({needsSync.length})
            </button>
          )}
        </div>

        <div className="sync-sheet-list">
          {tools.map((tool) => {
            const result = results[tool.key]
            const isSyncing = syncing.has(tool.key)
            const canSync = tool.status === "has_content" || tool.status === "ready"
            const hasResult = result && (result.success || result.error)

            return (
              <div key={tool.key} className={`sync-row ${isSyncing ? "syncing" : ""}`}>
                <div className="sync-row-main">
                  <div className="sync-row-info">
                    <StatusDot status={tool.status} />
                    <div>
                      <div className="sync-row-name">{tool.name}</div>
                      <div className="sync-row-path">{tool.global_skills}</div>
                    </div>
                  </div>
                  <div className="sync-row-right">
                    <StatusText status={tool.status} />
                    {canSync && !hasResult && (
                      <button
                        className="sync-row-btn"
                        disabled={isSyncing}
                        onClick={() => syncOne(tool.key)}
                      >
                        {isSyncing ? "Syncing" : "Link"}
                      </button>
                    )}
                  </div>
                </div>

                {hasResult && (
                  <div className="sync-row-detail">
                    {result!.success && !result!.error && (
                      <span className="detail-ok">
                        {result!.merged.length > 0
                          ? `Merged ${result!.merged.length} skill${result!.merged.length > 1 ? "s" : ""}`
                          : "Linked"}
                      </span>
                    )}
                    {result!.conflicts.length > 0 && (
                      <span className="detail-warn">
                        {result!.conflicts.length} conflict{result!.conflicts.length > 1 ? "s" : ""}
                      </span>
                    )}
                    {result!.error && <span className="detail-err">{result!.error}</span>}
                  </div>
                )}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

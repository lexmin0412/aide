import { useCallback, useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open, save } from "@tauri-apps/plugin-dialog"
import {
  Bot,
  CheckCircle2,
  Cpu,
  FileText,
  History,
  Package,
  RefreshCw,
  ScrollText,
  Server,
  ShieldCheck,
  Wrench,
} from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { CardGridSkeleton } from "./Skeleton"
import { PageError } from "./PageError"
import { toast } from "@/lib/toast"
import {
  countReportIssues,
  countToolStatuses,
  driftToolsNeedingCleanup,
  formatBytes,
  isReportHealthy,
  normalizeTimestamp,
  toolsNeedingAttention,
} from "@/lib/health"
import {
  DEFAULT_SNAPSHOT_LABEL,
  formatSnapshotTime,
  latestSnapshots,
  summarizeRestoreResult,
} from "@/lib/snapshots"
import {
  describeConflicts,
  formatBundleBytes,
  summarizeBundleImport,
  summarizeBundlePreview,
} from "@/lib/bundles"
import type {
  BundleExportResult,
  BundleImportMode,
  BundleImportResult,
  BundlePreview,
  DoctorReport,
  McpCleanupResult,
  ModelProfileSyncResult,
  RestoreResult,
  SkillUpdateInfo,
  SnapshotMeta,
} from "../types"

type RepairAction = "tools" | "updates" | "mcp" | "models" | "refresh"

// Tool statuses reported by `check_sync_statuses` reuse the SyncPanel labels.
const TOOL_STATUS_KEYS: Record<string, string> = {
  not_installed: "sync.status.notInstalled",
  compatible: "sync.status.native",
  synced: "sync.status.linked",
  has_content: "sync.status.pendingMerge",
  ready: "sync.status.ready",
}

function Section({ title, icon, children }: { title: string; icon: React.ReactNode; children: React.ReactNode }) {
  return (
    <section className="rounded-xl border border-border bg-card/40 p-4" aria-label={title}>
      <div className="flex items-center gap-2 mb-2">
        <span className="text-muted-foreground">{icon}</span>
        <h2 className="text-sm font-medium">{title}</h2>
      </div>
      {children}
    </section>
  )
}

function Stat({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-xl border border-border bg-card/40 p-3">
      <div className="text-lg font-semibold tracking-tight">{value}</div>
      <div className="text-[11px] text-muted-foreground truncate">{label}</div>
    </div>
  )
}

export default function HealthPage({ active = true }: { active?: boolean } = {}) {
  const { t } = useTranslation()
  const [report, setReport] = useState<DoctorReport | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<RepairAction | null>(null)
  // Snapshots section state.
  const [snapshots, setSnapshots] = useState<SnapshotMeta[] | null>(null)
  const [snapshotsError, setSnapshotsError] = useState<string | null>(null)
  const [snapshotsReloadKey, setSnapshotsReloadKey] = useState(0)
  const [snapshotBusy, setSnapshotBusy] = useState<"create" | "restore" | "delete" | null>(null)
  const [createOpen, setCreateOpen] = useState(false)
  const [snapshotLabel, setSnapshotLabel] = useState("")
  const [confirmRestore, setConfirmRestore] = useState<SnapshotMeta | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<SnapshotMeta | null>(null)
  // Environment bundle state: export options, import preview and mode.
  const [includeBundleSecrets, setIncludeBundleSecrets] = useState(false)
  const [bundleBusy, setBundleBusy] = useState<"export" | "preview" | "import" | null>(null)
  const [bundlePreview, setBundlePreview] = useState<BundlePreview | null>(null)
  const [bundlePath, setBundlePath] = useState<string | null>(null)
  const [importMode, setImportMode] = useState<BundleImportMode>("merge")

  const refreshReport = useCallback(async (): Promise<DoctorReport | null> => {
    try {
      const next = await invoke<DoctorReport>("get_doctor_report")
      setReport(next)
      setError(null)
      return next
    } catch (e) {
      setError(String(e))
      return null
    }
  }, [])

  // Reload whenever the page becomes visible again: the report is cheap to
  // build and other pages may have changed the underlying state in between.
  useEffect(() => {
    if (!active) return
    let cancelled = false
    const init = async () => {
      try {
        const next = await invoke<DoctorReport>("get_doctor_report")
        if (cancelled) return
        setReport(next)
        setError(null)
      } catch (e) {
        if (!cancelled) setError(String(e))
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    void init()
    return () => {
      cancelled = true
    }
  }, [active])

  const retryLoad = useCallback(() => {
    setLoading(true)
    setError(null)
    void invoke<DoctorReport>("get_doctor_report")
      .then((next) => setReport(next))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false))
  }, [])

  // Load the snapshot list whenever the page becomes visible or an action asks
  // for a refresh.
  useEffect(() => {
    if (!active) return
    let cancelled = false
    const loadSnapshots = async () => {
      try {
        const list = await invoke<SnapshotMeta[]>("list_snapshots")
        if (cancelled) return
        setSnapshots(list)
        setSnapshotsError(null)
      } catch (e) {
        if (!cancelled) setSnapshotsError(String(e))
      }
    }
    void loadSnapshots()
    return () => {
      cancelled = true
    }
  }, [active, snapshotsReloadKey])

  // Periodic freshness: refresh the doctor report every 60s while the page is
  // visible. Skipped while a dialog is open or a repair is running (the page
  // must not shift under the user) and while the window is hidden.
  useEffect(() => {
    if (!active) return
    const timer = window.setInterval(() => {
      if (document.visibilityState === "hidden") return
      if (busy !== null) return
      if (document.querySelector('[data-slot="dialog-content"]')) return
      void refreshReport()
    }, 60_000)
    return () => window.clearInterval(timer)
  }, [active, busy, refreshReport])

  // Runs one repair command, toasts the outcome and always reloads the report so
  // the page reflects the post-repair state.
  const runRepair = useCallback(
    async (action: RepairAction, repair: () => Promise<string>) => {
      setBusy(action)
      try {
        toast(await repair(), "success")
      } catch (e) {
        toast(t("health.actions.failed", { error: String(e) }), "error")
      } finally {
        await refreshReport()
        setBusy(null)
      }
    },
    [refreshReport, t]
  )

  const handleRefresh = () => {
    setBusy("refresh")
    void invoke<DoctorReport>("get_doctor_report")
      .then((next) => {
        setReport(next)
        setError(null)
      })
      .catch((e) => toast(t("health.actions.failed", { error: String(e) }), "error"))
      .finally(() => setBusy(null))
  }

  const handleSyncTools = () => {
    void runRepair("tools", async () => {
      const results = await invoke<{ success: boolean }[]>("sync_all_tools")
      const ok = results.filter((result) => result.success).length
      return t("health.actions.toolsDone", { ok, total: results.length })
    })
  }

  const handleCheckUpdates = () => {
    void runRepair("updates", async () => {
      const updates = await invoke<SkillUpdateInfo[]>("check_skill_updates")
      return t("health.actions.updatesChecked", { count: updates.length })
    })
  }

  const handleCleanupDrift = () => {
    void runRepair("mcp", async () => {
      const results = await invoke<McpCleanupResult[]>("cleanup_mcp_drift")
      const updated = results.filter((result) => !result.skipped).length
      return t("health.actions.driftCleaned", { updated, skipped: results.length - updated })
    })
  }

  const handleSyncModels = () => {
    void runRepair("models", async () => {
      const results = await invoke<ModelProfileSyncResult[]>("sync_all_model_profiles")
      const count = results.reduce((sum, item) => sum + item.results.length, 0)
      return t("health.actions.modelsSynced", { count })
    })
  }

  const refreshSnapshots = () => setSnapshotsReloadKey((key) => key + 1)

  const createSnapshot = async () => {
    setSnapshotBusy("create")
    try {
      const label = snapshotLabel.trim() || DEFAULT_SNAPSHOT_LABEL
      const created = await invoke<SnapshotMeta>("create_snapshot", { label })
      toast(t("snapshots.createdToast", { label: created.label }), "success")
      setCreateOpen(false)
      setSnapshotLabel("")
      refreshSnapshots()
    } catch (e) {
      toast(t("snapshots.actionFailed", { error: String(e) }), "error")
    } finally {
      setSnapshotBusy(null)
    }
  }

  const restoreSnapshot = async (snapshot: SnapshotMeta) => {
    setConfirmRestore(null)
    setSnapshotBusy("restore")
    try {
      const result = await invoke<RestoreResult>("restore_snapshot", { id: snapshot.id })
      const summary = summarizeRestoreResult(result)
      toast(
        t("snapshots.restoredToast", { files: summary.restoredFiles, drift: summary.skillDrift }),
        summary.hasDrift ? "info" : "success"
      )
      refreshSnapshots()
      void refreshReport()
    } catch (e) {
      toast(t("snapshots.actionFailed", { error: String(e) }), "error")
    } finally {
      setSnapshotBusy(null)
    }
  }

  const deleteSnapshot = async (snapshot: SnapshotMeta) => {
    setConfirmDelete(null)
    setSnapshotBusy("delete")
    try {
      await invoke("delete_snapshot", { id: snapshot.id })
      toast(t("snapshots.deletedToast"), "success")
      refreshSnapshots()
    } catch (e) {
      toast(t("snapshots.actionFailed", { error: String(e) }), "error")
    } finally {
      setSnapshotBusy(null)
    }
  }

  const exportBundle = async () => {
    setBundleBusy("export")
    try {
      const destPath = await save({
        title: t("bundle.exportTitle"),
        defaultPath: "aide-environment.aidebundle",
        filters: [{ name: t("bundle.fileFilter"), extensions: ["aidebundle"] }],
      })
      if (typeof destPath !== "string") return
      const result = await invoke<BundleExportResult>("export_bundle", {
        destPath,
        includeSecrets: includeBundleSecrets,
      })
      toast(
        t("bundle.exportedToast", {
          skills: result.skills,
          mcp: result.mcp_servers,
          profiles: result.model_profiles,
          size: formatBundleBytes(result.bytes),
          path: result.path,
          redacted: result.redacted_secrets.length,
        }),
        "success"
      )
    } catch (e) {
      toast(t("bundle.actionFailed", { error: String(e) }), "error")
    } finally {
      setBundleBusy(null)
    }
  }

  const chooseBundleForImport = async () => {
    setBundleBusy("preview")
    try {
      const selected = await open({
        title: t("bundle.importTitle"),
        multiple: false,
        filters: [{ name: t("bundle.fileFilter"), extensions: ["aidebundle"] }],
      })
      if (typeof selected !== "string") return
      const preview = await invoke<BundlePreview>("preview_bundle", { path: selected })
      setBundlePath(selected)
      setImportMode("merge")
      setBundlePreview(preview)
    } catch (e) {
      toast(t("bundle.previewFailed", { error: String(e) }), "error")
    } finally {
      setBundleBusy(null)
    }
  }

  const closeBundlePreview = () => {
    setBundlePreview(null)
    setBundlePath(null)
  }

  const confirmBundleImport = async () => {
    if (!bundlePath) return
    setBundleBusy("import")
    try {
      const result = await invoke<BundleImportResult>("import_bundle", {
        path: bundlePath,
        mode: importMode,
      })
      const summary = summarizeBundleImport(result)
      toast(
        t("bundle.importedToast", {
          added: summary.added,
          replaced: summary.replaced,
          safety: summary.safetySnapshot,
        }),
        summary.warningCount > 0 ? "info" : "success"
      )
      closeBundlePreview()
      void refreshReport()
      refreshSnapshots()
    } catch (e) {
      toast(t("bundle.actionFailed", { error: String(e) }), "error")
    } finally {
      setBundleBusy(null)
    }
  }

  const title = (
    <div className="flex-1 min-w-0">
      <h1 className="text-lg font-semibold tracking-tight">{t("health.title")}</h1>
      <p className="text-xs text-muted-foreground mt-0.5">
        {loading
          ? t("health.checking")
          : report
            ? t("health.summary", {
                count: countReportIssues(report),
                time: normalizeTimestamp(report.generated_at)
                  ? new Date(normalizeTimestamp(report.generated_at)).toLocaleString()
                  : t("health.unknownTime"),
              })
            : t("health.checking")}
      </p>
    </div>
  )

  const header = (
    <div className="flex items-center gap-4 px-6 pt-5 pb-4 shrink-0">
      {title}
      <Button
        variant="outline"
        size="sm"
        onClick={handleRefresh}
        disabled={busy !== null}
        title={t("health.refresh")}
      >
        <RefreshCw size={13} className={busy === "refresh" ? "animate-spin" : ""} /> {t("health.refresh")}
      </Button>
    </div>
  )

  if (loading) {
    return (
      <div className="h-full flex flex-col">
        {header}
        <div className="flex-1 overflow-y-auto px-6 pb-6">
          <CardGridSkeleton />
        </div>
      </div>
    )
  }

  if (error && !report) {
    return (
      <div className="h-full flex flex-col">
        {header}
        <PageError message={t("health.actions.loadFailed", { error })} onRetry={retryLoad} />
      </div>
    )
  }

  if (!report) return null

  const issues = countReportIssues(report)
  const healthy = isReportHealthy(report)
  const attentionTools = toolsNeedingAttention(report.tools)
  const statusCounts = countToolStatuses(report.tools)
  const driftTools = driftToolsNeedingCleanup(report.mcp.drift)
  const skills = report.skills
  const mcp = report.mcp
  const models = report.models
  const agent = report.agent
  const audit = report.audit
  const busyLabel = busy === null ? null : t("health.actions.running")

  return (
    <div className="h-full flex flex-col">
      {header}
      <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-3">
        {healthy ? (
          <>
            <div className="rounded-xl border border-emerald-500/30 bg-emerald-500/5 p-5 flex items-start gap-3">
              <CheckCircle2 size={18} className="text-emerald-500 shrink-0 mt-0.5" />
              <div>
                <p className="text-sm font-medium">{t("health.allGoodTitle")}</p>
                <p className="text-xs text-muted-foreground mt-1">{t("health.allGoodDesc")}</p>
              </div>
            </div>
            <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
              <Stat label={t("health.sections.skills")} value={skills.total} />
              <Stat label={t("health.sections.tools")} value={report.tools.length} />
              <Stat label={t("health.sections.mcp")} value={mcp.servers} />
              <Stat label={t("health.sections.models")} value={models.profiles} />
            </div>
          </>
        ) : (
          <>
            <Section title={t("health.sections.skills")} icon={<Wrench size={14} />}>
              <p className="text-xs text-muted-foreground">
                {t("health.skills.summary", { total: skills.total, linked: skills.linked })}
              </p>
              {skills.broken.length > 0 && (
                <ul className="mt-2 space-y-1">
                  {skills.broken.map((name) => (
                    <li key={name} className="text-xs text-destructive break-words">
                      {t("health.skills.broken", { name })}
                    </li>
                  ))}
                </ul>
              )}
              {skills.updates_available > 0 && (
                <p className="mt-2 text-xs text-amber-500">
                  {t("health.skills.updates", { count: skills.updates_available })}
                </p>
              )}
              {skills.update_error && (
                <p className="mt-2 text-xs text-destructive break-words">
                  {t("health.skills.updateError", { error: skills.update_error })}
                </p>
              )}
              <div className="flex flex-wrap gap-2 mt-3">
                <Button size="sm" variant="outline" disabled={busy !== null} onClick={handleSyncTools}>
                  {busy === "tools" ? busyLabel : t("health.skills.sync")}
                </Button>
                <Button size="sm" variant="outline" disabled={busy !== null} onClick={handleCheckUpdates}>
                  {busy === "updates" ? busyLabel : t("health.skills.checkUpdates")}
                </Button>
              </div>
            </Section>

            {attentionTools.length > 0 && (
              <Section title={t("health.sections.tools")} icon={<Cpu size={14} />}>
                <div className="flex flex-wrap gap-1.5 mb-2">
                  {Object.entries(statusCounts).map(([status, count]) => (
                    <span
                      key={status}
                      className="px-1.5 py-0.5 rounded bg-secondary text-[10px] text-muted-foreground"
                    >
                      {t(TOOL_STATUS_KEYS[status] ?? status)}: {count}
                    </span>
                  ))}
                </div>
                <ul className="space-y-1">
                  {attentionTools.map((tool) => (
                    <li key={tool.key} className="text-xs">
                      <span className="font-medium">{tool.name}</span>
                      <span className="text-muted-foreground">
                        {" "}
                        — {t(TOOL_STATUS_KEYS[tool.status] ?? tool.status)} · {tool.global_skills}
                      </span>
                    </li>
                  ))}
                </ul>
              </Section>
            )}

            {(driftTools.length > 0 || mcp.errors.length > 0) && (
              <Section title={t("health.sections.mcp")} icon={<Server size={14} />}>
                <p className="text-xs text-muted-foreground">
                  {t("health.mcp.summary", { servers: mcp.servers, disabled: mcp.disabled })}
                </p>
                {driftTools.length > 0 && (
                  <>
                    <p className="mt-2 text-xs text-amber-500">
                      {t("health.mcp.drift", { count: driftTools.length })}
                    </p>
                    <ul className="mt-1 space-y-1">
                      {driftTools.map((tool) => (
                        <li key={tool.tool_key} className="text-xs break-words">
                          <span className="font-medium">{tool.tool_name}</span>
                          {tool.missing.length > 0 && (
                            <span className="text-muted-foreground">
                              {" "}
                              · {t("health.mcp.missing", { names: tool.missing.join(", ") })}
                            </span>
                          )}
                          {tool.leftover.length > 0 && (
                            <span className="text-muted-foreground">
                              {" "}
                              · {t("health.mcp.leftover", { names: tool.leftover.join(", ") })}
                            </span>
                          )}
                        </li>
                      ))}
                    </ul>
                  </>
                )}
                {mcp.errors.length > 0 && (
                  <ul className="mt-2 space-y-1">
                    {mcp.errors.map((message, index) => (
                      <li key={`${index}-${message}`} className="text-xs text-destructive break-words">
                        {message}
                      </li>
                    ))}
                  </ul>
                )}
                <div className="flex flex-wrap gap-2 mt-3">
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={busy !== null || driftTools.length === 0}
                    onClick={handleCleanupDrift}
                  >
                    {busy === "mcp" ? busyLabel : t("health.mcp.cleanup")}
                  </Button>
                </div>
              </Section>
            )}

            {(models.missing_targets.length > 0 || models.errors.length > 0) && (
              <Section title={t("health.sections.models")} icon={<ShieldCheck size={14} />}>
                <p className="text-xs text-muted-foreground">
                  {t("health.models.summary", { count: models.profiles })}
                </p>
                {models.missing_targets.length > 0 && (
                  <p className="mt-2 text-xs text-amber-500 break-words">
                    {t("health.models.missingTargets", {
                      count: models.missing_targets.length,
                      names: models.missing_targets.join(", "),
                    })}
                  </p>
                )}
                {models.errors.length > 0 && (
                  <ul className="mt-2 space-y-1">
                    {models.errors.map((message, index) => (
                      <li key={`${index}-${message}`} className="text-xs text-destructive break-words">
                        {message}
                      </li>
                    ))}
                  </ul>
                )}
                <div className="flex flex-wrap gap-2 mt-3">
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={busy !== null || models.profiles === 0}
                    onClick={handleSyncModels}
                  >
                    {busy === "models" ? busyLabel : t("health.models.sync")}
                  </Button>
                </div>
              </Section>
            )}

            {report.configs.length > 0 && (
              <Section title={t("health.sections.configs")} icon={<FileText size={14} />}>
                <p className="text-xs text-muted-foreground">
                  {t("health.configs.errors", { count: report.configs.length })}
                </p>
                <ul className="mt-2 space-y-1">
                  {report.configs.map((entry, index) => (
                    <li key={`${index}-${entry.path}`} className="text-xs break-words">
                      <span className="font-mono">{entry.path}</span>
                      <span className="text-destructive"> — {entry.error}</span>
                    </li>
                  ))}
                </ul>
              </Section>
            )}
          </>
        )}

        <Section title={t("snapshots.title")} icon={<History size={14} />}>
          <div className="flex items-start justify-between gap-2 mb-2">
            <p className="text-[11px] text-muted-foreground flex-1 min-w-0">{t("snapshots.desc")}</p>
            <Button
              size="sm"
              variant="outline"
              className="shrink-0"
              disabled={snapshotBusy !== null}
              onClick={() => setCreateOpen(true)}
            >
              {t("snapshots.create")}
            </Button>
          </div>
          {snapshotsError ? (
            <div className="flex items-center gap-2">
              <p className="text-xs text-destructive flex-1 break-words">
                {t("snapshots.loadFailed", { error: snapshotsError })}
              </p>
              <Button size="sm" variant="ghost" onClick={refreshSnapshots}>
                {t("common.retry")}
              </Button>
            </div>
          ) : snapshots === null ? (
            <p className="text-xs text-muted-foreground">{t("snapshots.loading")}</p>
          ) : snapshots.length === 0 ? (
            <p className="text-xs text-muted-foreground">{t("snapshots.empty")}</p>
          ) : (
            <ul className="space-y-1.5">
              {latestSnapshots(snapshots, 5).map((snapshot) => (
                <li
                  key={snapshot.id}
                  className="flex flex-wrap items-center gap-2 rounded-md border border-border px-2.5 py-1.5"
                >
                  <div className="flex-1 min-w-0">
                    <p className="text-xs font-medium truncate">{snapshot.label}</p>
                    <p className="text-[10px] text-muted-foreground truncate">
                      {formatSnapshotTime(snapshot.created_at)}
                      {" · "}
                      {t("snapshots.meta", {
                        skills: snapshot.skill_count,
                        files: snapshot.files.length,
                        size: formatBytes(snapshot.size_bytes),
                      })}
                    </p>
                  </div>
                  <div className="flex gap-1 shrink-0">
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-6 text-xs px-2"
                      disabled={snapshotBusy !== null}
                      onClick={() => setConfirmRestore(snapshot)}
                    >
                      {snapshotBusy === "restore" ? t("snapshots.restoring") : t("snapshots.restore")}
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      className="h-6 text-xs px-2 text-destructive"
                      disabled={snapshotBusy !== null}
                      onClick={() => setConfirmDelete(snapshot)}
                    >
                      {t("snapshots.delete")}
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          )}
          {snapshots && snapshots.length > 5 && (
            <p className="text-[10px] text-muted-foreground/70 mt-2">
              {t("snapshots.hidden", { count: snapshots.length - 5 })}
            </p>
          )}
        </Section>

        <Section title={t("bundle.title")} icon={<Package size={14} />}>
          <p className="text-[11px] text-muted-foreground mb-2">{t("bundle.desc")}</p>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="outline" disabled={bundleBusy !== null} onClick={() => void exportBundle()}>
              {bundleBusy === "export" ? t("bundle.exporting") : t("bundle.export")}
            </Button>
            <Button size="sm" variant="outline" disabled={bundleBusy !== null} onClick={() => void chooseBundleForImport()}>
              {bundleBusy === "preview" ? t("bundle.previewing") : t("bundle.import")}
            </Button>
          </div>
          <div className="flex items-start gap-2 mt-3">
            <Checkbox
              id="bundle-include-non-secret"
              checked={includeBundleSecrets}
              onCheckedChange={(checked) => setIncludeBundleSecrets(checked === true)}
            />
            <div className="min-w-0">
              <label htmlFor="bundle-include-non-secret" className="text-xs cursor-pointer">
                {t("bundle.includeNonSecret")}
              </label>
              <p className="text-[10px] text-muted-foreground mt-0.5">{t("bundle.secretsNote")}</p>
            </div>
          </div>
        </Section>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <Section title={t("health.sections.agent")} icon={<Bot size={14} />}>
            <p className="text-xs text-muted-foreground">
              {agent.enabled
                ? t("health.agent.enabled")
                : agent.binary_available
                  ? t("health.agent.disabled")
                  : t("health.agent.noBinary")}
            </p>
          </Section>
          <Section title={t("health.sections.audit")} icon={<ScrollText size={14} />}>
            <p className="text-xs text-muted-foreground">
              {t("health.audit.size", { size: formatBytes(audit.size_bytes) })}
            </p>
            <p className="mt-1 text-[10px] text-muted-foreground/70 font-mono truncate" title={audit.path}>
              {audit.path}
            </p>
          </Section>
        </div>

        {issues > 0 && (
          <p className="text-[10px] text-muted-foreground text-center">
            {t("health.issues", { count: issues })}
          </p>
        )}
      </div>

      <Dialog open={createOpen} onOpenChange={(next) => { if (!next) setCreateOpen(false) }}>
        <DialogContent className="sm:max-w-[400px]">
          <DialogHeader>
            <DialogTitle>{t("snapshots.createTitle")}</DialogTitle>
            <DialogDescription>{t("snapshots.createDesc")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-1">
            <Label htmlFor="snapshot-label">{t("snapshots.label")}</Label>
            <Input
              id="snapshot-label"
              value={snapshotLabel}
              onChange={(event) => setSnapshotLabel(event.target.value)}
              placeholder={t("snapshots.labelPlaceholder")}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault()
                  void createSnapshot()
                }
              }}
            />
          </div>
          <DialogFooter>
            <Button variant="ghost" size="sm" onClick={() => setCreateOpen(false)}>
              {t("common.cancel")}
            </Button>
            <Button size="sm" disabled={snapshotBusy !== null} onClick={() => void createSnapshot()}>
              {snapshotBusy === "create" ? t("snapshots.creating") : t("snapshots.createAction")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmRestore !== null} onOpenChange={(next) => { if (!next) setConfirmRestore(null) }}>
        <DialogContent className="sm:max-w-[400px]">
          <DialogHeader>
            <DialogTitle>{t("snapshots.restoreTitle")}</DialogTitle>
            <DialogDescription>
              {confirmRestore ? t("snapshots.restoreConfirm", { label: confirmRestore.label }) : null}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" size="sm" onClick={() => setConfirmRestore(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              size="sm"
              disabled={snapshotBusy !== null}
              onClick={() => { if (confirmRestore) void restoreSnapshot(confirmRestore) }}
            >
              {snapshotBusy === "restore" ? t("snapshots.restoring") : t("snapshots.restore")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmDelete !== null} onOpenChange={(next) => { if (!next) setConfirmDelete(null) }}>
        <DialogContent className="sm:max-w-[400px]">
          <DialogHeader>
            <DialogTitle>{t("snapshots.deleteTitle")}</DialogTitle>
            <DialogDescription>
              {confirmDelete ? t("snapshots.deleteConfirm", { label: confirmDelete.label }) : null}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" size="sm" onClick={() => setConfirmDelete(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="destructive"
              size="sm"
              disabled={snapshotBusy !== null}
              onClick={() => { if (confirmDelete) void deleteSnapshot(confirmDelete) }}
            >
              {t("snapshots.delete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={bundlePreview !== null} onOpenChange={(next) => { if (!next) closeBundlePreview() }}>
        <DialogContent className="sm:max-w-[560px] flex flex-col max-h-[85vh]">
          {bundlePreview &&
            (() => {
              const summary = summarizeBundlePreview(bundlePreview)
              const conflictRows = describeConflicts(bundlePreview)
              return (
                <>
                  <DialogHeader>
                    <DialogTitle>{t("bundle.previewTitle")}</DialogTitle>
                    <DialogDescription>
                      {t("bundle.previewMeta", {
                        version: bundlePreview.format_version,
                        time: formatSnapshotTime(bundlePreview.created_at),
                        aide: bundlePreview.aide_version,
                      })}
                    </DialogDescription>
                  </DialogHeader>
                  <div className="flex-1 min-h-0 space-y-3 overflow-y-auto pr-1">
                    {bundlePath && (
                      <p className="text-[10px] font-mono text-muted-foreground truncate" title={bundlePath}>
                        {t("bundle.file", { path: bundlePath })}
                      </p>
                    )}
                    <div className="grid grid-cols-3 gap-2">
                      {conflictRows.map((row) => (
                        <div key={row.category} className="rounded-lg border border-border p-2">
                          <p className="text-[11px] font-medium">{t(`bundle.categories.${row.category}`)}</p>
                          <p className="text-[10px] text-muted-foreground">
                            {t("bundle.newCount", { count: row.newCount })}
                          </p>
                          <p
                            className={`text-[10px] ${
                              row.conflictingCount > 0 ? "text-amber-500" : "text-muted-foreground"
                            }`}
                          >
                            {t("bundle.conflictCount", { count: row.conflictingCount })}
                          </p>
                        </div>
                      ))}
                    </div>
                    {bundlePreview.includes_secrets && (
                      <p className="text-[11px] text-muted-foreground">{t("bundle.includesSecretsNote")}</p>
                    )}
                    {summary.isEmpty && (
                      <p className="text-[11px] text-muted-foreground">{t("bundle.emptyPreview")}</p>
                    )}
                    {bundlePreview.redacted_secrets.length > 0 && (
                      <div>
                        <p className="text-xs font-medium">
                          {t("bundle.redactedTitle", { count: bundlePreview.redacted_secrets.length })}
                        </p>
                        <ul className="mt-1 space-y-0.5">
                          {bundlePreview.redacted_secrets.map((secretPath, index) => (
                            <li
                              key={`${index}-${secretPath}`}
                              className="font-mono text-[10px] text-muted-foreground truncate"
                              title={secretPath}
                            >
                              {secretPath}
                            </li>
                          ))}
                        </ul>
                      </div>
                    )}
                    {bundlePreview.warnings.length > 0 && (
                      <div>
                        <p className="text-xs font-medium text-amber-500">{t("bundle.warningsTitle")}</p>
                        <ul className="mt-1 space-y-0.5">
                          {bundlePreview.warnings.map((warning, index) => (
                            <li key={`${index}-${warning}`} className="text-[11px] text-amber-500 break-words">
                              {warning}
                            </li>
                          ))}
                        </ul>
                      </div>
                    )}
                    <fieldset>
                      <legend className="text-xs font-medium mb-1">{t("bundle.modeTitle")}</legend>
                      <div className="space-y-1.5">
                        <label className="flex items-start gap-2 text-xs cursor-pointer">
                          <input
                            type="radio"
                            name="bundle-import-mode"
                            value="merge"
                            checked={importMode === "merge"}
                            onChange={() => setImportMode("merge")}
                            className="mt-0.5 accent-primary"
                          />
                          <span>
                            <span className="font-medium">{t("bundle.modeMerge")}</span>
                            {" · "}
                            <span className="text-muted-foreground">{t("bundle.modeMergeDesc")}</span>
                          </span>
                        </label>
                        <label className="flex items-start gap-2 text-xs cursor-pointer">
                          <input
                            type="radio"
                            name="bundle-import-mode"
                            value="replace"
                            checked={importMode === "replace"}
                            onChange={() => setImportMode("replace")}
                            className="mt-0.5 accent-primary"
                          />
                          <span>
                            <span className="font-medium">{t("bundle.modeReplace")}</span>
                            {" · "}
                            <span className="text-muted-foreground">{t("bundle.modeReplaceDesc")}</span>
                          </span>
                        </label>
                        {importMode === "replace" && (
                          <p className="text-[11px] text-destructive">{t("bundle.replaceWarning")}</p>
                        )}
                      </div>
                    </fieldset>
                  </div>
                  <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={closeBundlePreview}>
                      {t("common.cancel")}
                    </Button>
                    <Button size="sm" disabled={bundleBusy !== null} onClick={() => void confirmBundleImport()}>
                      {bundleBusy === "import" ? t("bundle.importing") : t("bundle.import")}
                    </Button>
                  </DialogFooter>
                </>
              )
            })()}
        </DialogContent>
      </Dialog>
    </div>
  )
}

import { useCallback, useEffect, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import {
  Bot,
  CheckCircle2,
  Cpu,
  FileText,
  RefreshCw,
  ScrollText,
  Server,
  ShieldCheck,
  Wrench,
} from "lucide-react"
import { useTranslation } from "react-i18next"
import { Button } from "@/components/ui/button"
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
import type { DoctorReport, McpCleanupResult, ModelProfileSyncResult, SkillUpdateInfo } from "../types"

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
    </div>
  )
}

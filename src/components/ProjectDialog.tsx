import { useCallback, useEffect, useMemo, useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog"
import { FolderGit2, FolderOpen } from "lucide-react"
import { useTranslation } from "react-i18next"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { CardGridSkeleton } from "./Skeleton"
import { PageError } from "./PageError"
import { toast } from "@/lib/toast"
import {
  buildProjectConfig,
  defaultSelectedTools,
  filterProjectSkills,
  projectCapableTools,
  sortProjectTools,
  summarizeSyncResult,
  toggleSelection,
} from "@/lib/projectProfile"
import type { ProjectConfig, ProjectStatus, ProjectSyncResult, SkillInfo } from "../types"

interface ProjectDialogProps {
  open: boolean
  onClose: () => void
}

/** Minimal shape of a central MCP server used by the picker. */
interface ProjectMcpServer {
  name: string
  disabled?: boolean | null
}

const LAST_PROJECT_PATH_KEY = "aide.projectPath"

function readStoredProjectPath(): string | null {
  try {
    return localStorage.getItem(LAST_PROJECT_PATH_KEY)
  } catch {
    // localStorage can be unavailable (private mode); the picker still works.
    return null
  }
}

function storeProjectPath(path: string) {
  try {
    localStorage.setItem(LAST_PROJECT_PATH_KEY, path)
  } catch {
    // Persisting the last path is best-effort.
  }
}

function Section({ title, badge, children }: { title: string; badge?: string; children: React.ReactNode }) {
  return (
    <section className="rounded-lg border border-border bg-card/40 p-3" aria-label={title}>
      <div className="flex items-center gap-2 mb-2">
        <h3 className="text-xs font-medium uppercase tracking-wider text-muted-foreground">{title}</h3>
        {badge && <Badge variant="secondary" className="text-[10px]">{badge}</Badge>}
      </div>
      {children}
    </section>
  )
}

export function ProjectDialog({ open, onClose }: ProjectDialogProps) {
  const { t } = useTranslation()
  const [projectPath, setProjectPath] = useState<string | null>(() => readStoredProjectPath())
  const [status, setStatus] = useState<ProjectStatus | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [skills, setSkills] = useState<SkillInfo[]>([])
  const [servers, setServers] = useState<ProjectMcpServer[]>([])
  const [selectedSkills, setSelectedSkills] = useState<string[]>([])
  const [selectedMcp, setSelectedMcp] = useState<string[]>([])
  const [selectedTools, setSelectedTools] = useState<string[]>([])
  const [skillQuery, setSkillQuery] = useState("")
  const [busy, setBusy] = useState<"save" | "clear" | null>(null)
  const [reloadKey, setReloadKey] = useState(0)
  const [syncResult, setSyncResult] = useState<ProjectSyncResult | null>(null)
  const [confirmClear, setConfirmClear] = useState(false)

  const applyStatus = useCallback((next: ProjectStatus) => {
    setStatus(next)
    setSelectedSkills(next.config.skills)
    setSelectedMcp(next.config.mcp)
    setSelectedTools(next.has_config ? next.config.tools : defaultSelectedTools(next.tools))
    setLoadError(null)
  }, [])

  // Load the project status together with the skill/MCP catalogs whenever the
  // dialog opens, the folder changes or the user retries after a failure.
  useEffect(() => {
    if (!open || !projectPath) return
    let cancelled = false
    const init = async () => {
      try {
        const [skillList, serverList, projectStatus] = await Promise.all([
          invoke<SkillInfo[]>("list_skills"),
          invoke<ProjectMcpServer[]>("list_mcp_servers"),
          invoke<ProjectStatus>("get_project_status", { projectPath }),
        ])
        if (cancelled) return
        setSkills(skillList)
        setServers(serverList)
        setSkillQuery("")
        setSyncResult(null)
        applyStatus(projectStatus)
      } catch (e) {
        if (!cancelled) setLoadError(String(e))
      }
    }
    void init()
    return () => {
      cancelled = true
    }
  }, [open, projectPath, reloadKey, applyStatus])

  const pickFolder = async () => {
    const selected = await openFolderDialog({
      directory: true,
      multiple: false,
      title: t("project.pickFolderTitle"),
    })
    if (typeof selected !== "string") return
    storeProjectPath(selected)
    setStatus(null)
    setLoadError(null)
    setSyncResult(null)
    setConfirmClear(false)
    setProjectPath(selected)
  }

  const retryLoad = () => {
    setLoadError(null)
    setStatus(null)
    setReloadKey((key) => key + 1)
  }

  // Save the given config, run the sync, refresh the status and surface the
  // sync outcome without discarding it (applyStatus runs first).
  const persistProfile = async (config: ProjectConfig, toastKey: "saved" | "cleared") => {
    if (!projectPath) return
    setBusy(toastKey === "saved" ? "save" : "clear")
    try {
      await invoke("save_project_config", { projectPath, config })
      const result = await invoke<ProjectSyncResult>("sync_project", { projectPath })
      const refreshed = await invoke<ProjectStatus>("get_project_status", { projectPath })
      applyStatus(refreshed)
      setSyncResult(result)
      const summary = summarizeSyncResult(result)
      toast(
        t(`project.toast.${toastKey}`, {
          linked: summary.linked,
          removed: summary.removed,
          tools: summary.toolsWritten,
        }),
        summary.hasIssues ? "info" : "success"
      )
    } catch (e) {
      toast(t("project.toast.failed", { error: String(e) }), "error")
    } finally {
      setBusy(null)
    }
  }

  const handleSave = () => {
    void persistProfile(buildProjectConfig(selectedSkills, selectedMcp, selectedTools), "saved")
  }

  const handleClear = () => {
    setConfirmClear(false)
    void persistProfile(buildProjectConfig([], [], []), "cleared")
  }

  const filteredSkills = useMemo(() => filterProjectSkills(skills, skillQuery), [skills, skillQuery])
  const capableTools = useMemo(() => sortProjectTools(projectCapableTools(status?.tools ?? [])), [status])
  const installedSkillNames = useMemo(() => new Set(skills.map((skill) => skill.name)), [skills])
  const missingSkills = useMemo(
    () => (status?.missing_skills ?? []).filter((name) => !installedSkillNames.has(name)),
    [status, installedSkillNames]
  )
  const installedMcpNames = useMemo(() => new Set(servers.map((server) => server.name)), [servers])
  const missingMcp = useMemo(
    () => (status?.missing_mcp ?? []).filter((name) => !installedMcpNames.has(name)),
    [status, installedMcpNames]
  )

  const showReady = Boolean(projectPath) && !loadError && Boolean(status)
  const busyLabel = busy === "save" ? t("project.actions.saving") : busy === "clear" ? t("project.actions.clearing") : null

  // The folder header stays available while loading or after a load failure so
  // the user can always switch to another project instead of being stuck.
  const pathHeader = projectPath ? (
    <div className="flex flex-wrap items-center gap-2 border-b border-border pb-3 shrink-0">
      <FolderGit2 size={14} className="shrink-0 text-muted-foreground" />
      <span className="font-mono text-[11px] truncate flex-1 min-w-0" title={status?.project_root ?? projectPath}>
        {status?.project_root ?? projectPath}
      </span>
      <Button variant="ghost" size="sm" disabled={busy !== null} onClick={() => void pickFolder()}>
        {t("project.changeFolder")}
      </Button>
    </div>
  ) : null

  const statusLine = status ? (
    <p className="text-[11px] text-muted-foreground shrink-0">
      {status.has_config ? t("project.configured") : t("project.notConfigured")}
      {" · "}
      {t("project.status.linked", { count: status.linked_skills })}
    </p>
  ) : null

  return (
    <Dialog open={open} onOpenChange={(next) => { if (!next) onClose() }}>
      <DialogContent className="sm:max-w-[640px] flex flex-col max-h-[85vh]">
        <DialogHeader>
          <DialogTitle>{t("project.title")}</DialogTitle>
          <DialogDescription>{t("project.desc")}</DialogDescription>
        </DialogHeader>

        {!projectPath ? (
          <div className="flex flex-col items-center justify-center gap-3 py-12 text-center">
            <FolderOpen size={32} className="text-primary/40" />
            <div>
              <p className="text-sm font-medium">{t("project.emptyTitle")}</p>
              <p className="text-xs text-muted-foreground mt-1 max-w-[360px]">{t("project.emptyDesc")}</p>
            </div>
            <Button size="sm" onClick={() => void pickFolder()}>
              {t("project.openFolder")}
            </Button>
          </div>
        ) : (
          <>
            {pathHeader}
            {statusLine}
            {loadError ? (
              <div className="min-h-[240px]">
                <PageError message={t("project.loadFailed", { error: loadError })} onRetry={retryLoad} />
              </div>
            ) : !status ? (
              <div className="py-2"><CardGridSkeleton count={2} /></div>
            ) : (
              <div className="flex-1 min-h-0 overflow-y-auto space-y-3 pr-1">
              <Section title={t("project.skills.title")} badge={t("project.selected", { count: selectedSkills.length })}>
                <Input
                  value={skillQuery}
                  onChange={(event) => setSkillQuery(event.target.value)}
                  placeholder={t("project.skills.search")}
                  aria-label={t("project.skills.search")}
                  className="h-7 text-xs mb-2"
                />
                <div className="max-h-[180px] overflow-y-auto space-y-1 pr-1">
                  {filteredSkills.map((skill) => (
                    <div key={skill.name} className="flex items-center gap-2 px-1.5 py-1 rounded hover:bg-secondary/50">
                      <Checkbox
                        id={`project-skill-${skill.name}`}
                        checked={selectedSkills.includes(skill.name)}
                        onCheckedChange={() => setSelectedSkills((prev) => toggleSelection(prev, skill.name))}
                      />
                      <label htmlFor={`project-skill-${skill.name}`} className="text-xs truncate flex-1 cursor-pointer">
                        {skill.display_name}
                      </label>
                      <span className="font-mono text-[10px] text-muted-foreground/60 truncate max-w-[140px]">
                        {skill.name}
                      </span>
                    </div>
                  ))}
                  {missingSkills.map((name) => (
                    <div key={`missing-skill-${name}`} className="flex items-center gap-2 px-1.5 py-1">
                      <Checkbox
                        id={`project-missing-skill-${name}`}
                        checked={selectedSkills.includes(name)}
                        onCheckedChange={() => setSelectedSkills((prev) => toggleSelection(prev, name))}
                      />
                      <label htmlFor={`project-missing-skill-${name}`} className="text-xs truncate flex-1 cursor-pointer">
                        {name}
                      </label>
                      <Badge variant="outline" className="text-[10px] text-destructive">{t("project.missing")}</Badge>
                    </div>
                  ))}
                  {filteredSkills.length === 0 && missingSkills.length === 0 && (
                    <p className="text-xs text-muted-foreground px-1.5 py-2">
                      {skills.length === 0 ? t("project.skills.empty") : t("project.skills.noMatch")}
                    </p>
                  )}
                </div>
              </Section>

              <Section title={t("project.mcp.title")} badge={t("project.selected", { count: selectedMcp.length })}>
                <div className="max-h-[160px] overflow-y-auto space-y-1 pr-1">
                  {servers.map((server) => (
                    <div key={server.name} className="flex items-center gap-2 px-1.5 py-1 rounded hover:bg-secondary/50">
                      <Checkbox
                        id={`project-mcp-${server.name}`}
                        checked={selectedMcp.includes(server.name)}
                        onCheckedChange={() => setSelectedMcp((prev) => toggleSelection(prev, server.name))}
                      />
                      <label htmlFor={`project-mcp-${server.name}`} className="text-xs truncate flex-1 cursor-pointer">
                        {server.name}
                      </label>
                      {server.disabled && (
                        <Badge variant="outline" className="text-[10px]">{t("mcp.disabled")}</Badge>
                      )}
                    </div>
                  ))}
                  {missingMcp.map((name) => (
                    <div key={`missing-mcp-${name}`} className="flex items-center gap-2 px-1.5 py-1">
                      <Checkbox
                        id={`project-missing-mcp-${name}`}
                        checked={selectedMcp.includes(name)}
                        onCheckedChange={() => setSelectedMcp((prev) => toggleSelection(prev, name))}
                      />
                      <label htmlFor={`project-missing-mcp-${name}`} className="text-xs truncate flex-1 cursor-pointer">
                        {name}
                      </label>
                      <Badge variant="outline" className="text-[10px] text-destructive">{t("project.missing")}</Badge>
                    </div>
                  ))}
                  {servers.length === 0 && missingMcp.length === 0 && (
                    <p className="text-xs text-muted-foreground px-1.5 py-2">{t("project.mcp.empty")}</p>
                  )}
                </div>
              </Section>

              <Section title={t("project.tools.title")} badge={t("project.selected", { count: selectedTools.length })}>
                <div className="max-h-[160px] overflow-y-auto space-y-1 pr-1">
                  {capableTools.map((tool) => (
                    <div key={tool.key} className="flex items-center gap-2 px-1.5 py-1 rounded hover:bg-secondary/50">
                      <Checkbox
                        id={`project-tool-${tool.key}`}
                        checked={selectedTools.includes(tool.key)}
                        onCheckedChange={() => setSelectedTools((prev) => toggleSelection(prev, tool.key))}
                      />
                      <label htmlFor={`project-tool-${tool.key}`} className="text-xs flex-1 truncate cursor-pointer">
                        {tool.name}
                      </label>
                      {tool.detected_globally && (
                        <Badge variant="secondary" className="text-[10px]">{t("project.tools.detected")}</Badge>
                      )}
                      <span
                        className="font-mono text-[10px] text-muted-foreground/60 truncate max-w-[140px]"
                        title={tool.project_skills}
                      >
                        {tool.project_skills}
                      </span>
                    </div>
                  ))}
                  {capableTools.length === 0 && (
                    <p className="text-xs text-muted-foreground px-1.5 py-2">{t("project.tools.empty")}</p>
                  )}
                </div>
                <p className="text-[10px] text-muted-foreground/70 mt-2 px-1.5">{t("project.tools.alwaysManaged")}</p>
              </Section>

              {status.warnings.length > 0 && (
                <section className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3" aria-label={t("project.warningsTitle")}>
                  <h3 className="text-xs font-medium text-amber-500 mb-1">{t("project.warningsTitle")}</h3>
                  <ul className="space-y-0.5">
                    {status.warnings.map((warning, index) => (
                      <li key={`${index}-${warning}`} className="text-[11px] text-amber-500 break-words">
                        {warning}
                      </li>
                    ))}
                  </ul>
                </section>
              )}

              {syncResult && (
                <section className="rounded-lg border border-border bg-card/40 p-3" aria-label={t("project.sync.title")}>
                  <h3 className="text-xs font-medium text-muted-foreground mb-1">{t("project.sync.title")}</h3>
                  <p className="text-[11px] text-muted-foreground">
                    {t("project.sync.summary", {
                      linked: syncResult.skills.linked,
                      removed: syncResult.skills.removed,
                      tools: syncResult.mcp.tools_written,
                    })}
                  </p>
                  {syncResult.warnings.map((warning, index) => (
                    <p key={`sync-warning-${index}`} className="text-[11px] text-amber-500 break-words">{warning}</p>
                  ))}
                  {syncResult.errors.map((message, index) => (
                    <p key={`sync-error-${index}`} className="text-[11px] text-destructive break-words">{message}</p>
                  ))}
                </section>
              )}
              </div>
            )}
          </>
        )}

        <DialogFooter>
          {!showReady ? (
            <Button variant="ghost" size="sm" onClick={onClose}>
              {t("common.cancel")}
            </Button>
          ) : confirmClear ? (
            <div className="flex w-full flex-col gap-2">
              <p className="text-xs text-destructive">{t("project.actions.clearConfirm")}</p>
              <div className="flex justify-end gap-2">
                <Button variant="ghost" size="sm" onClick={() => setConfirmClear(false)}>
                  {t("common.cancel")}
                </Button>
                <Button variant="destructive" size="sm" onClick={handleClear}>
                  {t("project.actions.confirmClear")}
                </Button>
              </div>
            </div>
          ) : (
            <>
              <Button variant="ghost" size="sm" disabled={busy !== null} onClick={onClose}>
                {t("common.cancel")}
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="text-destructive"
                disabled={busy !== null || !showReady || !status?.has_config}
                onClick={() => setConfirmClear(true)}
              >
                {busy === "clear" ? busyLabel : t("project.actions.clear")}
              </Button>
              <Button size="sm" disabled={busy !== null || !showReady} onClick={handleSave}>
                {busy === "save" ? busyLabel : t("project.actions.save")}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export default ProjectDialog

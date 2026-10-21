import { useState, useCallback, useEffect, useRef, lazy, Suspense } from "react"
import { invoke } from "@tauri-apps/api/core"
import { RefreshCw, Trash2, FilePlus, FolderPlus } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { FileTree } from "./FileTree"
import type { FileTreeHandle } from "./FileTree"
const Editor = lazy(() => import("./Editor"))
import { TabBar } from "./TabBar"
import { DirtyCloseDialog } from "./DirtyCloseDialog"
import { UnsavedChangesDialog } from "./UnsavedChangesDialog"
import { useTabs } from "@/hooks/useTabs"
import { useSidebarWidth } from "@/hooks/useSidebarWidth"
import { toast } from "@/lib/toast"
import { useTranslation } from "react-i18next"
import { getCurrentWindow } from "@tauri-apps/api/window"
import type { SkillInfo } from "../types"

interface SkillDetailProps {
  skill: SkillInfo
  initialFile?: string | null
  onBack: () => void
  onDelete: (path: string) => Promise<void>
  /** True while the skills page is visible; only the active pane owns Cmd+W. */
  active?: boolean
  /** App-level registry used to gate navigation away from this pane. */
  registerLeaveGuard?: (guard: ((proceed: () => void) => void) | null) => void
}

export default function SkillDetail({ skill, initialFile, onBack, onDelete, active = true, registerLeaveGuard }: SkillDetailProps) {
  const { t } = useTranslation()
  const [showDeleteDialog, setShowDeleteDialog] = useState(false)
  const rootPath = skill.is_symlink && skill.target_path ? skill.target_path : skill.path
  const treeRef = useRef<FileTreeHandle>(null)
  const [sidebarWidth, sidebarDivider] = useSidebarWidth("skill-detail")

  const {
    tabs, activeTabPath, activeTab,
    openFile, closeTab, switchTab,
    handleDirtyChange, refreshAllTabs, handleSave, handleFileDeleted, handleFileRenamed,
    registerDocSource,
    dirtyPendingTab, resolvePendingClose, cancelPendingClose,
    pendingLeaveTabs, requestLeave, resolveLeave, cancelLeave,
  } = useTabs({ active })

  // Latest openFile without making the effect depend on the changing tab state.
  const openFileRef = useRef(openFile)
  useEffect(() => {
    openFileRef.current = openFile
  }, [openFile])

  useEffect(() => {
    const skillMd = rootPath + "/SKILL.md"
    const target = initialFile || skillMd
    invoke<boolean>("file_exists", { path: target })
      .then((exists) => {
        // Fall back to SKILL.md when the requested file is gone.
        void openFileRef.current(exists ? target : skillMd)
      })
      .catch(() => {
        // Probe failed (file vanished): leave the tab strip untouched.
      })
    // Open the requested file when a skill is opened or the palette asks for
    // another file within the same skill.
  }, [rootPath, initialFile])

  // Reflect the open skill in the window title.
  useEffect(() => {
    const win = getCurrentWindow()
    void win.setTitle(`${skill.display_name} — aide`).catch(() => {})
    return () => {
      void win.setTitle("aide").catch(() => {})
    }
  }, [skill.display_name])

  // App-level navigation (top nav, palette) is gated through this guard so all
  // leave paths share one confirmation dialog.
  useEffect(() => {
    registerLeaveGuard?.((proceed) => requestLeave(proceed))
    return () => registerLeaveGuard?.(null)
  }, [registerLeaveGuard, requestLeave])

  const handleBack = useCallback(() => requestLeave(onBack), [requestLeave, onBack])

  const handleRefresh = useCallback(async () => {
    await treeRef.current?.refresh()
    await refreshAllTabs()
  }, [refreshAllTabs])

  const handleDelete = useCallback(async () => {
    await onDelete(skill.path)
    toast(t("skillDetail.deleted", { name: skill.display_name }), "success")
  }, [onDelete, skill.path, skill.display_name, t])

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-3 px-5 py-2.5 border-b border-border bg-card/40">
        <Button variant="ghost" size="sm" onClick={handleBack}>{t("skillDetail.back")}</Button>
        <div className="flex flex-col min-w-0 flex-1">
          <div className="flex items-center gap-2 min-w-0">
            <span className="text-sm font-medium truncate">
              {skill.is_symlink && "↗ "}{skill.display_name}
            </span>
            {skill.source && (
              <span
                className="px-1.5 py-0.5 rounded bg-secondary text-[10px] text-muted-foreground font-mono shrink-0 max-w-[160px] truncate"
                title={t("skillDetail.source")}
              >
                {skill.source}
              </span>
            )}
            {skill.scope && (
              <span
                className="px-1.5 py-0.5 rounded bg-violet-500/15 text-violet-400 text-[10px] shrink-0 max-w-[100px] truncate"
                title={t("skillCard.scope", { scope: skill.scope })}
              >
                {skill.scope}
              </span>
            )}
            {skill.tags.length > 0 && (
              <div className="flex gap-1 shrink-0">
                {skill.tags.slice(0, 4).map((t) => (
                  <span key={t} className="px-1.5 py-0.5 rounded-full bg-secondary text-[10px] text-muted-foreground max-w-[90px] truncate">
                    {t}
                  </span>
                ))}
                {skill.tags.length > 4 && (
                  <span className="text-[10px] text-muted-foreground self-center">+{skill.tags.length - 4}</span>
                )}
              </div>
            )}
          </div>
          <div className="text-[10px] text-muted-foreground font-mono truncate">
            {skill.is_symlink && skill.target_path ? (
              <>{skill.target_path} → {skill.path}</>
            ) : (
              skill.path
            )}
          </div>
        </div>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground hover:text-destructive" onClick={() => setShowDeleteDialog(true)}>
          <Trash2 size={14} />
        </Button>
      </div>
      <div className="flex-1 flex overflow-hidden">
        {sidebarDivider}
        <div className="bg-card/40 border-r border-border overflow-y-auto shrink-0 flex flex-col" style={{ width: sidebarWidth }}>
          <div className="flex items-center gap-0.5 px-3 py-2 text-[10px] text-muted-foreground font-mono border-b border-border">
            <span className="truncate flex-1">{rootPath.split("/").pop()}</span>
            <div className="flex items-center gap-0.5 shrink-0">
              <button
                className="p-0.5 text-muted-foreground hover:text-foreground rounded hover:bg-card/60"
                onClick={() => treeRef.current?.newFile()}
                title={t("fileTree.newFile")}
              >
                <FilePlus size={12} />
              </button>
              <button
                className="p-0.5 text-muted-foreground hover:text-foreground rounded hover:bg-card/60"
                onClick={() => treeRef.current?.newFolder()}
                title={t("fileTree.newFolder")}
              >
                <FolderPlus size={12} />
              </button>
              <button
                className="p-0.5 text-muted-foreground hover:text-foreground rounded hover:bg-card/60"
                onClick={handleRefresh}
                title={t("common.refresh")}
              >
                <RefreshCw size={12} />
              </button>
            </div>
          </div>
          <FileTree key={rootPath} ref={treeRef} rootPath={rootPath} onSelectFile={openFile} selectedPath={activeTabPath} onFileDeleted={handleFileDeleted} onFileRenamed={handleFileRenamed} />
        </div>
        <div className="flex-1 flex flex-col overflow-hidden">
          <TabBar tabs={tabs} activeTabPath={activeTabPath} onSelect={switchTab} onClose={closeTab} />
          <div className="flex-1 overflow-hidden">
            {activeTab ? (
              <Suspense fallback={null}>
                <Editor
                  tab={activeTab}
                  onDirtyChange={handleDirtyChange}
                  onSave={handleSave}
                  registerDocSource={registerDocSource}
                  onOpenFile={openFile}
                />
              </Suspense>
            ) : (
              <div className="h-full flex flex-col items-center justify-center gap-1 text-sm text-muted-foreground">
                <span>{t("skillDetail.selectFile")}</span>
                <span className="text-[10px] text-muted-foreground/60 font-mono">{t("skillDetail.navigateHint")}</span>
              </div>
            )}
          </div>
        </div>
      </div>

      {dirtyPendingTab && (
        <DirtyCloseDialog
          tab={dirtyPendingTab}
          onSaveClose={() => void resolvePendingClose(true)}
          onDiscard={() => void resolvePendingClose(false)}
          onCancel={cancelPendingClose}
        />
      )}

      {pendingLeaveTabs && (
        <UnsavedChangesDialog
          tabs={pendingLeaveTabs}
          onSave={() => void resolveLeave(true)}
          onDiscard={() => void resolveLeave(false)}
          onCancel={cancelLeave}
        />
      )}

      <Dialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("skillDetail.deleteTitle")}</DialogTitle>
            <DialogDescription>
              {t("skillDetail.deleteDesc", { name: skill.display_name })}
              {skill.is_symlink ? t("skillDetail.deleteDescSymlink") : t("skillDetail.deleteDescDir")}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" size="sm" onClick={() => setShowDeleteDialog(false)}>{t("common.cancel")}</Button>
            <Button variant="destructive" size="sm" onClick={() => { setShowDeleteDialog(false); void handleDelete() }}>{t("skillDetail.moveToTrash")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

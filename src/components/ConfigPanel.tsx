import { useState, useCallback, useEffect, useMemo, useRef, lazy, Suspense } from "react"
import { invoke } from "@tauri-apps/api/core"
import { RefreshCw, FilePlus, FolderPlus, Search } from "lucide-react"
import { Button } from "@/components/ui/button"
import { FileTree } from "./FileTree"
import type { FileTreeHandle } from "./FileTree"
const Editor = lazy(() => import("./Editor"))
import { TabBar } from "./TabBar"
import { DirtyCloseDialog } from "./DirtyCloseDialog"
import { useTabs } from "@/hooks/useTabs"
import { useSidebarWidth } from "@/hooks/useSidebarWidth"
import { CardGridSkeleton } from "./Skeleton"
import { useTranslation } from "react-i18next"
import type { ToolInfo } from "../types"

export function ConfigPanel() {
  const { t } = useTranslation()
  const [tools, setTools] = useState<ToolInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [activeTool, setActiveTool] = useState<string | null>(null)
  const [toolFilter, setToolFilter] = useState("")
  const [homeDir, setHomeDir] = useState("")
  const treeRef = useRef<FileTreeHandle>(null)
  const [sidebarWidth, sidebarDivider] = useSidebarWidth("config-panel")

  const {
    tabs, activeTabPath, activeTab,
    openFile, closeTab, switchTab,
    handleEditorChange, refreshAllTabs, handleSave, handleFileDeleted, handleFileRenamed, clearTabs,
    dirtyPendingTab, resolvePendingClose, cancelPendingClose,
  } = useTabs()

  useEffect(() => {
    Promise.all([
      invoke<ToolInfo[]>("list_tools"),
      invoke<string>("get_home_dir"),
    ])
      .then(([list, home]) => {
        setTools(list)
        setHomeDir(home)
        if (list.length > 0) setActiveTool(list[0].key)
      })
      .catch((e) => console.error("Failed to load tools:", e))
      .finally(() => setLoading(false))
  }, [])

  // Keep the active tool visible even when the filter would hide it.
  const visibleTools = useMemo(() => {
    const q = toolFilter.trim().toLowerCase()
    if (!q) return tools
    return tools.filter((t) => t.name.toLowerCase().includes(q) || t.key === activeTool)
  }, [tools, toolFilter, activeTool])
  const activeToolInfo = tools.find((t) => t.key === activeTool)
  const rootPath = activeToolInfo && homeDir ? homeDir + "/" + activeToolInfo.detect_dir : ""

  const handleRefresh = useCallback(async () => {
    await treeRef.current?.refresh()
    await refreshAllTabs()
  }, [refreshAllTabs])

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-2 px-4 py-2 border-b border-border bg-card/30 shrink-0 overflow-x-auto">
        <span className="text-[10px] text-muted-foreground uppercase tracking-wider shrink-0">{t("configs.tools")}</span>
        {tools.length > 10 && (
          <div className="relative shrink-0">
            <Search size={11} className="absolute left-1.5 top-1/2 -translate-y-1/2 text-muted-foreground" />
            <input
              value={toolFilter}
              onChange={(e) => setToolFilter(e.target.value)}
              placeholder={t("configs.toolsPlaceholder", { count: tools.length })}
              className="h-6 w-32 rounded-md border border-input bg-transparent pl-6 pr-1.5 text-xs outline-none placeholder:text-muted-foreground"
            />
          </div>
        )}
        {visibleTools.map((t) => (
          <Button
            key={t.key}
            variant={activeTool === t.key ? "secondary" : "ghost"}
            size="sm"
            className="h-6 text-xs shrink-0"
            onClick={() => { setActiveTool(t.key); clearTabs() }}
          >
            {t.name}
          </Button>
        ))}
      </div>
      <div className="flex-1 flex overflow-hidden">
        {loading ? (
          <div className="flex-1 p-6"><CardGridSkeleton count={3} /></div>
        ) : activeToolInfo && rootPath ? (
          <>
            {sidebarDivider}
            <div className="bg-card/40 border-r border-border overflow-y-auto shrink-0 flex flex-col" style={{ width: sidebarWidth }}>
              <div className="flex items-center gap-0.5 px-3 py-2 text-[10px] text-muted-foreground font-mono border-b border-border">
                <span className="truncate flex-1">{activeToolInfo.detect_dir}</span>
                <div className="flex items-center gap-0.5 shrink-0">
                  <button
                    className="p-0.5 text-muted-foreground hover:text-foreground rounded hover:bg-card/60"
                    onClick={() => treeRef.current?.newFile()}
                    title="New File"
                  >
                    <FilePlus size={12} />
                  </button>
                  <button
                    className="p-0.5 text-muted-foreground hover:text-foreground rounded hover:bg-card/60"
                    onClick={() => treeRef.current?.newFolder()}
                    title="New Folder"
                  >
                    <FolderPlus size={12} />
                  </button>
                  <button
                    className="p-0.5 text-muted-foreground hover:text-foreground rounded hover:bg-card/60"
                    onClick={handleRefresh}
                    title="Refresh"
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
                    <Editor tab={activeTab} onChange={handleEditorChange} onSave={handleSave} />
                  </Suspense>
                ) : (
                  <div className="h-full flex flex-col items-center justify-center gap-1 text-sm text-muted-foreground">
                    <span>{t("configs.selectFile")}</span>
                    <span className="text-[10px] text-muted-foreground/60 font-mono">{t("configs.navigateHint")}</span>
                  </div>
                )}
              </div>
            </div>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">{t("configs.noTools")}</div>
        )}
      </div>
      {dirtyPendingTab && (
        <DirtyCloseDialog
          tab={dirtyPendingTab}
          onSaveClose={() => void resolvePendingClose(true)}
          onDiscard={() => void resolvePendingClose(false)}
          onCancel={cancelPendingClose}
        />
      )}
    </div>
  )
}

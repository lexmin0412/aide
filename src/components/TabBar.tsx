import { memo } from "react"
import { isActivationKey } from "@/lib/a11y"
import { X } from "lucide-react"
import { useTranslation } from "react-i18next"
import type { EditorTab } from "../types"

interface TabBarProps {
  tabs: EditorTab[]
  activeTabPath: string | null
  onSelect: (path: string) => void
  onClose: (path: string, e?: React.MouseEvent) => void
}

export const TabBar = memo(function TabBar({ tabs, activeTabPath, onSelect, onClose }: TabBarProps) {
  const { t } = useTranslation()
  if (tabs.length === 0) return null
  return (
    <div
      role="tablist"
      aria-label={t("common.tabs")}
      className="flex bg-card/30 border-b border-border overflow-x-auto h-9 shrink-0"
    >
      {tabs.map((tab) => (
        <div
          key={tab.path}
          role="tab"
          aria-selected={activeTabPath === tab.path}
          tabIndex={activeTabPath === tab.path ? 0 : -1}
          className={`group flex items-center gap-1.5 pl-3 pr-1.5 h-full text-xs cursor-pointer border-r border-border whitespace-nowrap focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-ring ${
            activeTabPath === tab.path
              ? "bg-background border-b-2 border-b-primary text-foreground"
              : "bg-card/50 hover:bg-card text-muted-foreground hover:text-foreground"
          }`}
          onClick={() => onSelect(tab.path)}
          onKeyDown={(e) => {
            // Keys from the close button must not also select the tab.
            if (e.target !== e.currentTarget) return
            if (isActivationKey(e.key)) {
              e.preventDefault()
              onSelect(tab.path)
            }
          }}
        >
          <span className="max-w-[150px] truncate">{tab.name}</span>
          {tab.is_dirty && <span className="w-1.5 h-1.5 rounded-full bg-primary shrink-0" />}
          <button
            className="p-0.5 rounded text-muted-foreground/60 hover:text-foreground hover:bg-muted shrink-0"
            title={t("common.closeTab")}
            aria-label={t("common.closeTab")}
            onClick={(e) => onClose(tab.path, e)}
          >
            <X size={12} />
          </button>
        </div>
      ))}
    </div>
  )
})

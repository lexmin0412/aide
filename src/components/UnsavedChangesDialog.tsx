import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { useTranslation } from "react-i18next"
import type { EditorTab } from "../types"

interface UnsavedChangesDialogProps {
  /** Dirty tabs that would lose their edits if the user leaves now. */
  tabs: EditorTab[]
  onSave: () => void
  onDiscard: () => void
  onCancel: () => void
}

/**
 * Single generic leave guard dialog: used for back navigation, page navigation,
 * palette skill switching and ConfigPanel tool switching. Shows the number and
 * names of the dirty tabs so the user knows exactly what is at stake.
 */
export function UnsavedChangesDialog({ tabs, onSave, onDiscard, onCancel }: UnsavedChangesDialogProps) {
  const { t } = useTranslation()
  if (tabs.length === 0) return null
  const names = tabs.map((tab) => tab.name).join(", ")

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onCancel() }}>
      <DialogContent className="sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle>{t("dirtyTab.leaveTitle")}</DialogTitle>
          <DialogDescription className="break-words">
            {t("dirtyTab.leaveDesc", { count: tabs.length, names })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onCancel}>{t("common.cancel")}</Button>
          <Button variant="outline" size="sm" onClick={onDiscard}>{t("dirtyTab.discard")}</Button>
          <Button size="sm" onClick={onSave}>
            {tabs.length > 1 ? t("dirtyTab.saveAll") : t("dirtyTab.saveClose")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

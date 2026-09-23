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

interface DirtyCloseDialogProps {
  tab: EditorTab
  onSaveClose: () => void
  onDiscard: () => void
  onCancel: () => void
}

/** Shown when closing a tab that has unsaved edits. */
export function DirtyCloseDialog({ tab, onSaveClose, onDiscard, onCancel }: DirtyCloseDialogProps) {
  const { t } = useTranslation()
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onCancel() }}>
      <DialogContent className="sm:max-w-[400px]">
        <DialogHeader>
          <DialogTitle>{t("dirtyTab.title")}</DialogTitle>
          <DialogDescription>
            {t("dirtyTab.desc", { name: tab.name })}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onCancel}>{t("common.cancel")}</Button>
          <Button variant="outline" size="sm" onClick={onDiscard}>{t("dirtyTab.discard")}</Button>
          <Button size="sm" onClick={onSaveClose}>{t("dirtyTab.saveClose")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import type { EditorTab } from "../types"

interface DirtyCloseDialogProps {
  tab: EditorTab
  onSaveClose: () => void
  onDiscard: () => void
  onCancel: () => void
}

/** Shown when closing a tab that has unsaved edits. */
export function DirtyCloseDialog({ tab, onSaveClose, onDiscard, onCancel }: DirtyCloseDialogProps) {
  return (
    <Dialog open onOpenChange={(open) => { if (!open) onCancel() }}>
      <DialogContent className="sm:max-w-[400px]">
        <DialogHeader>
          <DialogTitle>Unsaved Changes</DialogTitle>
          <DialogDescription>
            <span className="font-mono text-foreground">{tab.name}</span> has unsaved changes. Save before closing?
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="ghost" size="sm" onClick={onCancel}>Cancel</Button>
          <Button variant="outline" size="sm" onClick={onDiscard}>Discard</Button>
          <Button size="sm" onClick={onSaveClose}>Save &amp; Close</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

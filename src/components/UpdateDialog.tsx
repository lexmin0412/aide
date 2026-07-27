import { useState, useEffect, useRef } from "react"
import { check, Update } from "@tauri-apps/plugin-updater"
import { relaunch } from "@tauri-apps/plugin-process"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"

interface UpdateState {
  version?: string
  body?: string
  downloading: boolean
  done: boolean
  error?: string
  checking: boolean
}

export function UpdateDialog({ onClose }: { onClose: () => void }) {
  const [upd, setUpd] = useState<UpdateState>({ checking: true, downloading: false, done: false })
  const updateRef = useRef<Update | null>(null)

  useEffect(() => {
    let cancelled = false
    const timer = setTimeout(() => {
      if (!cancelled) {
        setUpd({ checking: false, downloading: false, done: false, error: "Request timed out" })
      }
    }, 10000)

    check({ timeout: 8000 })
      .then((u) => {
        if (cancelled) return
        clearTimeout(timer)
        if (u) {
          updateRef.current = u
          setUpd({ version: u.version, body: u.body, downloading: false, done: false, checking: false })
        } else {
          setUpd({ checking: false, downloading: false, done: false })
        }
      })
      .catch((e) => {
        if (cancelled) return
        clearTimeout(timer)
        setUpd({ checking: false, downloading: false, done: false, error: String(e) })
      })

    return () => { cancelled = true }
  }, [])

  const handleInstall = async () => {
    const u = updateRef.current
    if (!u) return
    setUpd((prev) => ({ ...prev, downloading: true, error: undefined }))
    try {
      await u.downloadAndInstall()
      setUpd((prev) => ({ ...prev, downloading: false, done: true }))
    } catch (e) {
      setUpd((prev) => ({ ...prev, downloading: false, error: String(e) }))
    }
  }

  const handleRelaunch = async () => {
    try {
      await relaunch()
    } catch (e) {
      setUpd((prev) => ({ ...prev, error: String(e) }))
    }
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open) onClose() }}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Update</DialogTitle>
        </DialogHeader>
        <div className="text-sm space-y-3 py-2">
          {upd.checking ? (
            <p className="text-muted-foreground">Checking for updates...</p>
          ) : upd.error && !upd.version ? (
            <>
              <p className="text-destructive">Failed to check for updates.</p>
              <p className="text-xs text-muted-foreground">{upd.error}</p>
            </>
          ) : !upd.version ? (
            <p className="text-muted-foreground">aide is up to date.</p>
          ) : upd.done ? (
            <>
              <p>Update installed. Restart to apply.</p>
              {upd.error && <p className="text-destructive text-xs">{upd.error}</p>}
              <div className="flex justify-end gap-2 pt-2">
                <Button variant="outline" size="sm" onClick={onClose}>Later</Button>
                <Button size="sm" onClick={handleRelaunch}>Restart</Button>
              </div>
            </>
          ) : (
            <>
              <p>
                A new version is available:{" "}
                <span className="font-semibold">{upd.version}</span>
              </p>
              {upd.body && (
                <div className="bg-muted rounded-md p-3 max-h-[200px] overflow-y-auto text-xs whitespace-pre-wrap font-mono">
                  {upd.body}
                </div>
              )}
              {upd.error && (
                <p className="text-destructive text-xs">{upd.error}</p>
              )}
              {upd.downloading ? (
                <p className="text-muted-foreground">Downloading and installing...</p>
              ) : (
                <div className="flex justify-end gap-2 pt-2">
                  <Button variant="outline" size="sm" onClick={onClose}>Later</Button>
                  <Button size="sm" onClick={handleInstall}>Download & Install</Button>
                </div>
              )}
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

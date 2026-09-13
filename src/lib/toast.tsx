import { useEffect, useState } from "react"

export type ToastKind = "success" | "error" | "info"

interface ToastItem {
  id: number
  message: string
  kind: ToastKind
}

type Listener = (toast: ToastItem) => void

let nextId = 0
const listeners = new Set<Listener>()

export function toast(message: string, kind: ToastKind = "info") {
  const item: ToastItem = { id: ++nextId, message, kind }
  listeners.forEach((fn) => fn(item))
}

const KIND_STYLES: Record<ToastKind, string> = {
  success: "border-emerald-500/40",
  error: "border-destructive/60",
  info: "border-border",
}

const KIND_DOT: Record<ToastKind, string> = {
  success: "bg-emerald-400",
  error: "bg-destructive",
  info: "bg-blue-400",
}

export function ToastHost() {
  const [items, setItems] = useState<ToastItem[]>([])

  useEffect(() => {
    const onToast: Listener = (t) => {
      setItems((prev) => [...prev.slice(-4), t])
      window.setTimeout(() => {
        setItems((prev) => prev.filter((i) => i.id !== t.id))
      }, 4000)
    }
    listeners.add(onToast)
    return () => {
      listeners.delete(onToast)
    }
  }, [])

  if (items.length === 0) return null

  return (
    <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2 pointer-events-none">
      {items.map((t) => (
        <div
          key={t.id}
          className={`pointer-events-auto flex items-center gap-2 min-w-[200px] max-w-[360px] px-3 py-2 rounded-lg bg-card border shadow-lg text-xs animate-in fade-in slide-in-from-bottom-2 ${KIND_STYLES[t.kind]}`}
        >
          <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${KIND_DOT[t.kind]}`} />
          <span className="whitespace-pre-wrap break-words">{t.message}</span>
        </div>
      ))}
    </div>
  )
}

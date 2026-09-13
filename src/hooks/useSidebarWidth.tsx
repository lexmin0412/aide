import { useState } from "react"

const DEFAULT_WIDTH = 256
const MIN_WIDTH = 180
const MAX_WIDTH = 480

/** Sidebar width persisted per usage key, resizable by dragging the divider. */
export function useSidebarWidth(key: string): [number, React.ReactNode] {
  const storageKey = `sidebar-width-${key}`
  const [width, setWidth] = useState(() => {
    const stored = Number(localStorage.getItem(storageKey))
    return stored >= MIN_WIDTH && stored <= MAX_WIDTH ? stored : DEFAULT_WIDTH
  })

  const startDrag = (e: React.MouseEvent) => {
    e.preventDefault()
    const startX = e.clientX
    const startWidth = width
    let latest = startWidth
    const onMove = (ev: MouseEvent) => {
      latest = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, startWidth + ev.clientX - startX))
      setWidth(latest)
    }
    const onUp = () => {
      localStorage.setItem(storageKey, String(latest))
      window.removeEventListener("mousemove", onMove)
      window.removeEventListener("mouseup", onUp)
      document.body.style.cursor = ""
      document.body.style.userSelect = ""
    }
    document.body.style.cursor = "col-resize"
    document.body.style.userSelect = "none"
    window.addEventListener("mousemove", onMove)
    window.addEventListener("mouseup", onUp)
  }

  const divider = (
    <div
      className="w-1 shrink-0 cursor-col-resize hover:bg-ring/40 transition-colors"
      onMouseDown={startDrag}
    />
  )

  return [width, divider]
}

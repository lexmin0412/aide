import { useState } from "react"
import { useTranslation } from "react-i18next"

const DEFAULT_WIDTH = 256
const MIN_WIDTH = 180
const MAX_WIDTH = 480
const KEYBOARD_STEP = 16

/** Sidebar width persisted per usage key, resizable by dragging the divider
 * (or with the arrow keys when the divider has focus). */
export function useSidebarWidth(key: string): [number, React.ReactNode] {
  const { t } = useTranslation()
  const storageKey = `sidebar-width-${key}`
  const [width, setWidth] = useState(() => {
    const stored = Number(localStorage.getItem(storageKey))
    return stored >= MIN_WIDTH && stored <= MAX_WIDTH ? stored : DEFAULT_WIDTH
  })

  const persist = (next: number) => {
    const clamped = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, next))
    setWidth(clamped)
    localStorage.setItem(storageKey, String(clamped))
  }

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

  const onKeyDown = (e: React.KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === "ArrowLeft") {
      e.preventDefault()
      persist(width - KEYBOARD_STEP)
    } else if (e.key === "ArrowRight") {
      e.preventDefault()
      persist(width + KEYBOARD_STEP)
    } else if (e.key === "Home") {
      e.preventDefault()
      persist(MIN_WIDTH)
    } else if (e.key === "End") {
      e.preventDefault()
      persist(MAX_WIDTH)
    }
  }

  // Rendered as a focusable button so the splitter is operable from the keyboard
  // (drag with the mouse, arrows/Home/End with the keyboard).
  const divider = (
    <button
      type="button"
      aria-label={t("common.resizeSidebar")}
      className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-ring/40 focus-visible:bg-ring/60 focus-visible:outline-none transition-colors"
      onMouseDown={startDrag}
      onKeyDown={onKeyDown}
    />
  )

  return [width, divider]
}

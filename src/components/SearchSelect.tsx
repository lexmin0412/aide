import { useState, useEffect, useRef, useMemo } from "react"
import { ChevronDown, Search } from "lucide-react"

export interface SearchSelectOption {
  value: string
  label: string
  hint?: string
}

interface SearchSelectProps {
  value: string
  options: SearchSelectOption[]
  placeholder: string
  emptyText?: string
  onChange: (value: string) => void
}

/** Filterable single-select dropdown for long option lists (e.g. 200+
 * providers). Keyboard: arrows navigate, Enter selects, Esc closes. */
export function SearchSelect({
  value,
  options,
  placeholder,
  emptyText = "No matches",
  onChange,
}: SearchSelectProps) {
  const [open, setOpen] = useState(false)
  const [filter, setFilter] = useState("")
  const [index, setIndex] = useState(0)
  const ref = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    const onPointerDown = (e: PointerEvent) => {
      if (e.button !== 0) return
      if (ref.current?.contains(e.target as Node)) return
      setOpen(false)
    }
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false)
    }
    document.addEventListener("pointerdown", onPointerDown, true)
    document.addEventListener("keydown", onKey)
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true)
      document.removeEventListener("keydown", onKey)
    }
  }, [open])

  useEffect(() => {
    if (open) {
      setFilter("")
      setIndex(0)
      requestAnimationFrame(() => inputRef.current?.focus())
    }
  }, [open])

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase()
    if (!q) return options
    return options.filter(
      (o) =>
        o.label.toLowerCase().includes(q) ||
        o.value.toLowerCase().includes(q) ||
        (o.hint ?? "").toLowerCase().includes(q)
    )
  }, [options, filter])

  useEffect(() => {
    setIndex((prev) => Math.min(prev, Math.max(0, filtered.length - 1)))
  }, [filtered.length])

  const selected = options.find((o) => o.value === value)

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        className="h-8 w-full rounded-lg border border-input bg-transparent px-2 text-xs text-left flex items-center justify-between gap-1 hover:border-primary/40 transition-colors"
        onClick={() => setOpen((o) => !o)}
      >
        <span className={`truncate ${selected ? "" : "text-muted-foreground"}`}>
          {selected?.label ?? placeholder}
        </span>
        <ChevronDown size={12} className="text-muted-foreground shrink-0" />
      </button>
      {open && (
        <div className="absolute z-50 left-0 right-0 top-full mt-1 rounded-lg border border-border bg-popover shadow-lg p-1">
          <div className="relative mb-1">
            <Search size={12} className="absolute left-2 top-1.5 text-muted-foreground" />
            <input
              ref={inputRef}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault()
                  setIndex((i) => Math.min(i + 1, filtered.length - 1))
                } else if (e.key === "ArrowUp") {
                  e.preventDefault()
                  setIndex((i) => Math.max(i - 1, 0))
                } else if (e.key === "Enter") {
                  e.preventDefault()
                  const opt = filtered[index]
                  if (opt) {
                    onChange(opt.value)
                    setOpen(false)
                  }
                }
              }}
              placeholder="Search..."
              className="h-7 w-full rounded-md border border-input bg-transparent pl-7 pr-2 text-xs outline-none placeholder:text-muted-foreground"
            />
          </div>
          <div className="max-h-[240px] overflow-y-auto">
            {filtered.length === 0 ? (
              <p className="text-xs text-muted-foreground text-center py-4">{emptyText}</p>
            ) : (
              filtered.map((o, i) => (
                <button
                  type="button"
                  key={o.value}
                  className={`w-full text-left px-2 py-1.5 rounded-md text-xs flex items-center justify-between gap-2 ${
                    i === index ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
                  }`}
                  onClick={() => {
                    onChange(o.value)
                    setOpen(false)
                  }}
                  onMouseEnter={() => setIndex(i)}
                >
                  <span className="truncate">{o.label}</span>
                  {o.hint && (
                    <span className="text-[10px] text-muted-foreground font-mono shrink-0">{o.hint}</span>
                  )}
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  )
}

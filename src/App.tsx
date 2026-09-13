import { useState, useCallback, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"
import { check, Update } from "@tauri-apps/plugin-updater"
import SkillGrid from "./components/SkillGrid"
import SkillDetail from "./components/SkillDetail"
import { ConfigPanel } from "./components/ConfigPanel"
import MCPPage from "./components/MCPPage"
import { UpdateDialog } from "./components/UpdateDialog"
import { CommandPalette } from "./components/CommandPalette"
import { Logo } from "./components/Logo"
import { ToastHost } from "@/lib/toast"
import { applyTheme, getThemePref } from "@/lib/theme"
import type { ThemePref } from "@/lib/theme"
import type { SkillInfo } from "./types"
import "./App.css"

type Page = "skills" | "mcp" | "configs"

const THEME_ICONS: Record<ThemePref, React.ReactNode> = {
  system: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2" y="3" width="20" height="14" rx="2" />
      <line x1="8" y1="21" x2="16" y2="21" />
      <line x1="12" y1="17" x2="12" y2="21" />
    </svg>
  ),
  light: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41" />
    </svg>
  ),
  dark: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
    </svg>
  ),
}

const THEME_ORDER: ThemePref[] = ["system", "light", "dark"]

function ThemeToggle() {
  const [pref, setPref] = useState<ThemePref>(() => getThemePref())

  const cycle = () => {
    const next = THEME_ORDER[(THEME_ORDER.indexOf(pref) + 1) % THEME_ORDER.length]
    setPref(next)
    applyTheme(next)
  }

  return (
    <button
      onClick={cycle}
      className="text-muted-foreground hover:text-foreground transition-colors p-0.5"
      title={`Theme: ${pref}`}
    >
      {THEME_ICONS[pref]}
    </button>
  )
}

export default function App() {
  const [page, setPage] = useState<Page>("skills")
  const [selectedSkill, setSelectedSkill] = useState<SkillInfo | null>(null)
  const [initialFile, setInitialFile] = useState<string | null>(null)
  const [gridKey, setGridKey] = useState(0)
  const [showUpdate, setShowUpdate] = useState(false)
  const [paletteOpen, setPaletteOpen] = useState(false)

  const tabs: { key: Page; label: string }[] = [
    { key: "skills", label: "Skills" },
    { key: "mcp", label: "MCP" },
    { key: "configs", label: "Configs" },
  ]

  const handleSkillDeleted = useCallback(async (path: string) => {
    await invoke("delete_entry", { path })
    setSelectedSkill(null)
    setGridKey((k) => k + 1)
  }, [])

  // Global shortcuts: Cmd+K opens the skill search palette.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault()
        setPaletteOpen((prev) => !prev)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  // Silent update check shortly after launch; opens the dialog only when an
  // update exists. Failures are ignored (manual check stays available).
  useEffect(() => {
    let cancelled = false
    const timer = setTimeout(() => {
      check({ timeout: 8000 })
        .then((u: Update | null) => {
          if (!cancelled && u) setShowUpdate(true)
        })
        .catch(() => {})
    }, 3000)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [])

  const openSkillFromPalette = useCallback((skill: SkillInfo) => {
    setPage("skills")
    setInitialFile(null)
    setSelectedSkill(skill)
  }, [])

  const openFileFromPalette = useCallback((skill: SkillInfo, filePath: string) => {
    setPage("skills")
    setInitialFile(filePath)
    setSelectedSkill(skill)
  }, [])

  return (
    <div className="h-full flex flex-col text-foreground">
      <div className="flex items-center h-10 px-4 border-b border-border bg-card/80 shrink-0 gap-6">
        <div className="flex items-center gap-1.5">
          <Logo />
          <span className="font-mono text-[11px] font-medium tracking-[0.22em] uppercase text-muted-foreground">aide</span>
        </div>
        <div className="flex gap-1">
          {tabs.map((t) => (
            <button
              key={t.key}
              onClick={() => { setPage(t.key); if (t.key === "skills") setSelectedSkill(null) }}
              className={`px-3 h-7 text-xs rounded-md transition-colors ${
                page === t.key
                  ? "bg-primary/15 text-primary font-medium"
                  : "text-muted-foreground hover:text-foreground hover:bg-secondary/60"
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>
        <button
          onClick={() => setPaletteOpen(true)}
          className="ml-auto flex items-center gap-1.5 px-2 h-6 rounded-md border border-border text-[10px] text-muted-foreground hover:text-foreground hover:border-primary/50 transition-colors"
          title="Search skills (Cmd+K)"
        >
          <span>Search</span>
          <kbd className="font-mono bg-muted px-1 rounded">⌘K</kbd>
        </button>
        <ThemeToggle />
        <button
          onClick={() => setShowUpdate(true)}
          className="text-muted-foreground hover:text-foreground transition-colors"
          title="Check for updates"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
            <polyline points="7 10 12 15 17 10" />
            <line x1="12" y1="15" x2="12" y2="3" />
          </svg>
        </button>
        <a
          href="https://github.com/lexmin0412/aide"
          target="_blank"
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-foreground transition-colors"
          title="GitHub"
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
          </svg>
        </a>
      </div>
      {showUpdate && <UpdateDialog onClose={() => setShowUpdate(false)} />}
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        onSelectSkill={openSkillFromPalette}
        onSelectFileMatch={openFileFromPalette}
      />
      <div className="flex-1 relative overflow-hidden">
        <div className={`absolute inset-0 ${page === "skills" ? "" : "hidden"}`}>
          {selectedSkill ? (
            <SkillDetail
              skill={selectedSkill}
              initialFile={initialFile}
              onBack={() => setSelectedSkill(null)}
              onDelete={handleSkillDeleted}
            />
          ) : (
            <SkillGrid
              key={gridKey}
              onSelectSkill={(s) => { setInitialFile(null); setSelectedSkill(s) }}
            />
          )}
        </div>
        <div className={`absolute inset-0 ${page === "mcp" ? "" : "hidden"}`}>
          <MCPPage />
        </div>
        <div className={`absolute inset-0 ${page === "configs" ? "" : "hidden"}`}>
          <ConfigPanel />
        </div>
      </div>
      <ToastHost />
    </div>
  )
}

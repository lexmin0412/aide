export type ThemePref = "system" | "light" | "dark"

const STORAGE_KEY = "theme"

export function getThemePref(): ThemePref {
  const value = localStorage.getItem(STORAGE_KEY)
  return value === "light" || value === "dark" ? value : "system"
}

export function applyTheme(pref: ThemePref) {
  if (pref === "system") {
    localStorage.removeItem(STORAGE_KEY)
    document.documentElement.classList.toggle(
      "dark",
      window.matchMedia("(prefers-color-scheme: dark)").matches
    )
  } else {
    localStorage.setItem(STORAGE_KEY, pref)
    document.documentElement.classList.toggle("dark", pref === "dark")
  }
}

/** Apply the stored preference once and follow live system changes while in system mode. */
export function initTheme() {
  applyTheme(getThemePref())
  const mq = window.matchMedia("(prefers-color-scheme: dark)")
  mq.addEventListener("change", (e) => {
    if (getThemePref() !== "system") return
    document.documentElement.classList.toggle("dark", e.matches)
  })
}

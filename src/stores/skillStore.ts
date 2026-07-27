import { create } from "zustand"

interface SkillStore {
  searchQuery: string
  setSearchQuery: (query: string) => void
  scrollPosition: number
  setScrollPosition: (pos: number) => void
}

export const useSkillStore = create<SkillStore>((set) => ({
  searchQuery: "",
  setSearchQuery: (query) => set({ searchQuery: query }),
  scrollPosition: 0,
  setScrollPosition: (pos) => set({ scrollPosition: pos }),
}))

import { create } from "zustand"

interface SkillStore {
  searchQuery: string
  setSearchQuery: (query: string) => void
}

export const useSkillStore = create<SkillStore>((set) => ({
  searchQuery: "",
  setSearchQuery: (query) => set({ searchQuery: query }),
}))

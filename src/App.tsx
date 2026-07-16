import { useState } from "react"
import { SkillGrid } from "./components/SkillGrid"
import { SkillDetail } from "./components/SkillDetail"
import type { SkillInfo, ViewState } from "./types"
import "./App.css"

function App() {
  const [view, setView] = useState<ViewState>({ type: "home" })

  return (
    <div className="app">
      {view.type === "home" && (
        <SkillGrid
          onSelectSkill={(skill: SkillInfo) => setView({ type: "skill", skill })}
        />
      )}
      {view.type === "skill" && (
        <SkillDetail
          skill={view.skill}
          onBack={() => setView({ type: "home" })}
        />
      )}
    </div>
  )
}

export default App

import { useState } from "react"
import { invoke } from "@tauri-apps/api/core"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { toast } from "@/lib/toast"
import { useTranslation } from "react-i18next"

interface NewSkillDialogProps {
  open: boolean
  onClose: () => void
  onCreated: (name: string) => void
}

function toDirName(input: string): string {
  return input
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/[^a-z0-9-_]/g, "")
}

export function NewSkillDialog({ open, onClose, onCreated }: NewSkillDialogProps) {
  const { t } = useTranslation()
  const [name, setName] = useState("")
  const [description, setDescription] = useState("")
  const [tags, setTags] = useState("")
  const [creating, setCreating] = useState(false)

  const dirName = toDirName(name)
  const invalid = name.trim().length > 0 && dirName.length === 0

  const create = async () => {
    if (!dirName || creating) return
    setCreating(true)
    try {
      const home = await invoke<string>("get_home_dir")
      const skillDir = `${home}/.agents/skills/${dirName}`
      if (await invoke<boolean>("file_exists", { path: skillDir })) {
        toast(t("newSkill.exists", { name: dirName }), "error")
        return
      }
      await invoke("create_directory", { path: skillDir })
      const tagList = tags
        .split(/[,\s]+/)
        .map((t) => t.trim())
        .filter(Boolean)
      const frontmatter = [
        "---",
        `name: ${dirName}`,
        description.trim() ? `description: ${description.trim()}` : "description: ",
        tagList.length > 0 ? `tags: [${tagList.map((t) => `"${t}"`).join(", ")}]` : null,
        "---",
        "",
        `# ${name.trim() || dirName}`,
        "",
        description.trim() || "Describe what this skill does and when to use it.",
        "",
      ]
        .filter((line) => line !== null)
        .join("\n")
      await invoke("write_text_file", { path: `${skillDir}/SKILL.md`, content: frontmatter })
      toast(t("newSkill.created", { name: dirName }), "success")
      onCreated(dirName)
      setName("")
      setDescription("")
      setTags("")
      onClose()
    } catch (e) {
      toast(t("newSkill.failed", { error: String(e) }), "error")
    } finally {
      setCreating(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) onClose() }}>
      <DialogContent className="sm:max-w-[440px]">
        <DialogHeader>
          <DialogTitle>{t("newSkill.title")}</DialogTitle>
          <DialogDescription>
            {t("newSkill.desc")}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-3 py-1">
          <div className="space-y-1">
            <Label>{t("newSkill.name")}</Label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("newSkill.namePlaceholder")}
              autoFocus
              onKeyDown={(e) => { if (e.key === "Enter") void create() }}
            />
            {dirName && <p className="text-[10px] text-muted-foreground font-mono">{t("newSkill.folder", { name: dirName })}</p>}
            {invalid && <p className="text-[10px] text-destructive">{t("newSkill.invalidName")}</p>}
          </div>
          <div className="space-y-1">
            <Label>{t("newSkill.description")}</Label>
            <Textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder={t("newSkill.descriptionPlaceholder")}
              className="min-h-[64px] text-xs"
            />
          </div>
          <div className="space-y-1">
            <Label>{t("newSkill.tags")}</Label>
            <Input
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              placeholder={t("newSkill.tagsPlaceholder")}
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" onClick={onClose}>{t("common.cancel")}</Button>
          <Button size="sm" disabled={!dirName || creating} onClick={() => void create()}>
            {creating ? t("newSkill.creating") : t("newSkill.create")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

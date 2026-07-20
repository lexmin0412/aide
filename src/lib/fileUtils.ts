import { invoke } from "@tauri-apps/api/core"
import type { EditorTab } from "../types"

const IMAGE_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp",
])

export function isImageFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() || ""
  return IMAGE_EXTENSIONS.has(ext)
}

export async function readFileAsTab(filePath: string): Promise<EditorTab> {
  const name = filePath.split("/").pop() || filePath
  const isImage = isImageFile(filePath)
  if (isImage) {
    const content = await invoke<string>("read_binary_base64", { path: filePath })
    return { path: filePath, name, content, is_dirty: false, language: "plain", is_image: true }
  }
  const content = await invoke<string>("read_text_file", { path: filePath })
  return { path: filePath, name, content, is_dirty: false, language: "plain" }
}

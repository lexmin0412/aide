import { invoke } from "@tauri-apps/api/core"
import { convertFileSrc } from "@tauri-apps/api/core"
import type { EditorTab } from "../types"

const IMAGE_EXTENSIONS = new Set([
  "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp",
])

export function isImageFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() || ""
  return IMAGE_EXTENSIONS.has(ext)
}

export async function getFileMtime(filePath: string): Promise<number> {
  return invoke<number>("get_file_mtime", { path: filePath })
}

export async function readFileAsTab(filePath: string): Promise<EditorTab> {
  const name = filePath.split("/").pop() || filePath
  const mtime = await getFileMtime(filePath).catch(() => 0)
  if (isImageFile(filePath)) {
    // Served through the Tauri asset protocol so large images skip base64 IPC.
    return { path: filePath, name, content: convertFileSrc(filePath), is_dirty: false, language: "plain", is_image: true, mtime }
  }
  const content = await invoke<string>("read_text_file", { path: filePath })
  return { path: filePath, name, content, is_dirty: false, language: "plain", mtime }
}

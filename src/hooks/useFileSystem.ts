import { useState, useCallback } from "react"
import { invoke } from "@tauri-apps/api/core"
import type { FileEntry } from "../types"

export function useFileSystem() {
  const [entries, setEntries] = useState<FileEntry[]>([])
  const [currentDir, setCurrentDir] = useState<string>("")
  const [homeDir, setHomeDir] = useState<string>("")
  const [loading, setLoading] = useState(false)

  const init = useCallback(async () => {
    const home = await invoke<string>("get_home_dir")
    setHomeDir(home)
    return home
  }, [])

  const listDir = useCallback(async (path: string) => {
    setLoading(true)
    try {
      const result = await invoke<FileEntry[]>("list_directory", { path })
      setEntries(result)
      setCurrentDir(path)
      return result
    } finally {
      setLoading(false)
    }
  }, [])

  const readFile = useCallback(async (path: string): Promise<string> => {
    return await invoke<string>("read_text_file", { path })
  }, [])

  const writeFile = useCallback(async (path: string, content: string) => {
    await invoke("write_text_file", { path, content })
  }, [])

  const createFile = useCallback(async (path: string) => {
    await invoke("create_file", { path })
  }, [])

  const createDir = useCallback(async (path: string) => {
    await invoke("create_directory", { path })
  }, [])

  const deleteEntry = useCallback(async (path: string) => {
    await invoke("delete_entry", { path })
  }, [])

  const renameEntry = useCallback(async (oldPath: string, newPath: string) => {
    await invoke("rename_entry", { oldPath, newPath })
  }, [])

  const fileExists = useCallback(async (path: string): Promise<boolean> => {
    return await invoke<boolean>("file_exists", { path })
  }, [])

  return {
    entries,
    currentDir,
    homeDir,
    loading,
    init,
    listDir,
    readFile,
    writeFile,
    createFile,
    createDir,
    deleteEntry,
    renameEntry,
    fileExists,
  }
}

import type { FileEntry } from "../types"

interface FileListProps {
  entries: FileEntry[]
  currentDir: string
  onNavigateUp: () => void
  onSelectFile: (entry: FileEntry) => void
  onSelectDir: (entry: FileEntry) => void
  selectedPath: string | null
}

function formatSize(bytes: number): string {
  if (bytes === 0) return "-"
  const units = ["B", "KB", "MB", "GB"]
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

function formatDate(ts: number): string {
  if (!ts) return "-"
  const d = new Date(ts * 1000)
  return d.toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  })
}

const TEXT_EXTENSIONS = new Set([
  "md", "json", "jsonc", "yaml", "yml", "toml",
  "txt", "js", "ts", "jsx", "tsx", "css", "html",
  "sh", "bash", "env", "gitignore", "npmrc",
])

export function FileList({
  entries,
  currentDir,
  onNavigateUp,
  onSelectFile,
  onSelectDir,
  selectedPath,
}: FileListProps) {
  const dirName = currentDir.split("/").pop() || currentDir

  return (
    <div className="file-list">
      <div className="file-list-header">
        <div className="file-list-title">{dirName}</div>
        <div className="file-list-path">{currentDir}</div>
      </div>
      <div className="file-list-table">
        <div className="file-list-row file-list-head">
          <span className="col-name">Name</span>
          <span className="col-size">Size</span>
          <span className="col-date">Modified</span>
        </div>
        <div className="file-list-row file-list-up" onClick={onNavigateUp}>
          <span className="col-name">
            <span className="file-icon">..</span>
            <span className="file-label">..</span>
          </span>
          <span className="col-size">-</span>
          <span className="col-date">-</span>
        </div>
        {entries.map((entry) => {
          const ext = entry.extension?.toLowerCase() || ""
          const isText = TEXT_EXTENSIONS.has(ext) || entry.extension === null
          return (
            <div
              key={entry.path}
              className={`file-list-row ${selectedPath === entry.path ? "selected" : ""} ${entry.is_dir ? "dir" : ""}`}
              onClick={() => (entry.is_dir ? onSelectDir(entry) : onSelectFile(entry))}
            >
              <span className="col-name">
                <span className="file-icon">{entry.is_dir ? "\uD83D\uDCC1" : "\uD83D\uDCC4"}</span>
                <span className="file-label">{entry.name}</span>
                {!entry.is_dir && !isText && <span className="file-badge binary">binary</span>}
              </span>
              <span className="col-size">{entry.is_dir ? "-" : formatSize(entry.size)}</span>
              <span className="col-date">{formatDate(entry.modified_at)}</span>
            </div>
          )
        })}
      </div>
    </div>
  )
}

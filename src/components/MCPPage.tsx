import { useState, useEffect } from "react"
import { invoke } from "@tauri-apps/api/core"

interface McpServer {
  name: string
  command: string | null
  args: string[] | null
  url: string | null
  env: Record<string, string> | null
  disabled: boolean | null
  description: string | null
  targets: string[]
}

interface ToolOption {
  key: string
  name: string
}

export function MCPPage() {
  const [servers, setServers] = useState<McpServer[]>([])
  const [tools, setTools] = useState<ToolOption[]>([])
  const [editing, setEditing] = useState<McpServer | null>(null)
  const [showForm, setShowForm] = useState(false)
  const [showImport, setShowImport] = useState(false)
  const [syncing, setSyncing] = useState<string | null>(null)

  const load = async () => {
    const [s, t] = await Promise.all([
      invoke<McpServer[]>("list_mcp_servers"),
      invoke<ToolOption[]>("list_tools"),
    ])
    setServers(s)
    setTools(t)
  }

  useEffect(() => { load() }, [])

  const save = async (updated: McpServer[]) => {
    await invoke("save_mcp_servers", { servers: updated })
    setServers(updated)
    setShowForm(false)
    setShowImport(false)
    setEditing(null)
  }

  const remove = (name: string) => save(servers.filter((s) => s.name !== name))

  const toggle = (name: string) =>
    save(servers.map((s) => (s.name === name ? { ...s, disabled: !s.disabled } : s)))

  const syncTool = async (key: string) => {
    setSyncing(key)
    try { await invoke("sync_mcp_tool", { toolKey: key }) } catch {}
    setSyncing(null)
  }

  const syncAll = async () => { for (const t of tools) await syncTool(t.key) }

  return (
    <div className="mcp-page">
      <div className="mcp-header">
        <div>
          <h1 className="mcp-title">MCP Servers</h1>
          <p className="mcp-subtitle">{servers.length} server{servers.length !== 1 ? "s" : ""} configured</p>
        </div>
        <div className="mcp-header-actions">
          <button className="mcp-btn" onClick={async () => {
            try {
              const results = await invoke<any[]>("import_mcp_all")
              const total = results.reduce((n: number, r: any) => n + r.imported.length, 0)
              if (total > 0) load()
              const msg = results.length > 0
                ? results.map((r: any) => `${r.source}: ${r.imported.length} imported, ${r.skipped.length} skipped`).join("\n")
                : "No tools with MCP configs found"
              alert(msg)
            } catch (e) {
              alert("Scan failed: " + e)
            }
          }}>Scan</button>
          <button className="mcp-btn" onClick={() => setShowImport(true)}>+ JSON</button>
          <button className="mcp-btn" onClick={syncAll}>Sync All</button>
          <button className="mcp-btn primary" onClick={() => { setEditing(null); setShowForm(true) }}>+ Add</button>
        </div>
      </div>

      <div className="mcp-toolbar">
        <span className="mcp-toolbar-label">Sync to:</span>
        {tools.map((t) => (
          <button key={t.key} className="mcp-tool-btn" disabled={syncing === t.key} onClick={() => syncTool(t.key)}>
            {syncing === t.key ? "..." : t.name}
          </button>
        ))}
      </div>

      {servers.length === 0 ? (
        <div className="mcp-empty">No MCP servers configured.</div>
      ) : (
        <div className="mcp-list">
          {servers.map((server) => (
            <div key={server.name} className={`mcp-card ${server.disabled ? "disabled" : ""}`}>
              <div className="mcp-card-top">
                <div className="mcp-card-info">
                  <div className="mcp-card-name">{server.name}</div>
                  <div className="mcp-card-type">
                    {server.url ? "SSE" : server.command ? "STDIO" : "Unknown"}
                    {server.disabled && <span className="mcp-badge muted">Disabled</span>}
                  </div>
                </div>
                <div className="mcp-card-actions">
                  <button className="mcp-icon-btn" onClick={() => toggle(server.name)}
                    title={server.disabled ? "Enable" : "Disable"}>
                    {server.disabled ? "\u25B6" : "\u23F8"}
                  </button>
                  <button className="mcp-icon-btn" onClick={() => { setEditing(server); setShowForm(true) }} title="Edit">
                    {"\u270E"}
                  </button>
                  <button className="mcp-icon-btn danger" onClick={() => remove(server.name)} title="Remove">
                    {"\u2716"}
                  </button>
                </div>
              </div>
              <div className="mcp-card-detail">
                {server.command && <span className="mcp-detail-item">
                  <code>{server.command} {server.args?.join(" ")}</code>
                </span>}
                {server.url && <span className="mcp-detail-item"><code>{server.url}</code></span>}
                {server.description && <span className="mcp-detail-desc">{server.description}</span>}
                {server.targets.length > 0 && (
                  <span className="mcp-detail-targets">
                    Targets: {server.targets.map((t) => tools.find((x) => x.key === t)?.name || t).join(", ")}
                  </span>
                )}
                {server.env && Object.keys(server.env).length > 0 && (
                  <span className="mcp-detail-env">{Object.keys(server.env).length} env var(s)</span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      {showForm && <ServerForm server={editing} tools={tools} onSave={(s) => {
        editing ? save(servers.map((x) => (x.name === editing.name ? s : x))) : save([...servers, s])
      }} onClose={() => setShowForm(false)} />}

      {showImport && <ImportJsonModal onImport={(ns) => save([...servers, ...ns])}
        onClose={() => setShowImport(false)} />}
    </div>
  )
}

function ServerForm({ server, tools, onSave, onClose }: {
  server: McpServer | null; tools: ToolOption[]; onSave: (s: McpServer) => void; onClose: () => void
}) {
  const [name, setName] = useState(server?.name || "")
  const [type, setType] = useState(server?.url ? "sse" : "stdio")
  const [command, setCommand] = useState(server?.command || "")
  const [args, setArgs] = useState(server?.args?.join(" ") || "")
  const [url, setUrl] = useState(server?.url || "")
  const [desc, setDesc] = useState(server?.description || "")
  const [targets, setTargets] = useState<string[]>(server?.targets || [])
  const [envEntries, setEnvEntries] = useState<[string, string][]>(Object.entries(server?.env || {}))

  const submit = () => {
    if (!name.trim()) return
    onSave({
      name: name.trim(),
      command: type === "stdio" ? command.trim() || null : null,
      args: type === "stdio" && args.trim() ? args.trim().split(/\s+/) : null,
      url: type === "sse" ? url.trim() || null : null,
      env: envEntries.length > 0 ? Object.fromEntries(envEntries.filter(([k]) => k.trim())) : null,
      disabled: false,
      description: desc.trim() || null,
      targets,
    })
  }

  return (
    <div className="mcp-overlay" onClick={onClose}>
      <div className="mcp-form" onClick={(e) => e.stopPropagation()}>
        <div className="mcp-form-header">
          <h2>{server ? "Edit Server" : "Add Server"}</h2>
          <button className="mcp-icon-btn" onClick={onClose}>{"\u2716"}</button>
        </div>
        <div className="mcp-form-body">
          <label className="mcp-field">
            <span>Name</span>
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="my-server" />
          </label>
          <label className="mcp-field">
            <span>Type</span>
            <select value={type} onChange={(e) => setType(e.target.value)}>
              <option value="stdio">STDIO</option>
              <option value="sse">SSE</option>
            </select>
          </label>
          {type === "stdio" ? (
            <>
              <label className="mcp-field">
                <span>Command</span>
                <input value={command} onChange={(e) => setCommand(e.target.value)} placeholder="npx" />
              </label>
              <label className="mcp-field">
                <span>Args</span>
                <input value={args} onChange={(e) => setArgs(e.target.value)}
                  placeholder="-y @modelcontextprotocol/server-filesystem" />
              </label>
            </>
          ) : (
            <label className="mcp-field">
              <span>URL</span>
              <input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://" />
            </label>
          )}
          <label className="mcp-field">
            <span>Description</span>
            <input value={desc} onChange={(e) => setDesc(e.target.value)} placeholder="Optional" />
          </label>
          <label className="mcp-field">
            <span>Targets</span>
            <div className="mcp-targets">
              {tools.map((t) => (
                <label key={t.key} className="mcp-target-check">
                  <input type="checkbox" checked={targets.includes(t.key)}
                    onChange={() => setTargets((p) => p.includes(t.key) ? p.filter((k) => k !== t.key) : [...p, t.key])} />
                  {t.name}
                </label>
              ))}
            </div>
          </label>
          <div className="mcp-field">
            <span>Environment</span>
            <div className="mcp-env-list">
              {envEntries.map(([k, v], i) => (
                <div key={i} className="mcp-env-row">
                  <input value={k} onChange={(e) => { const n = [...envEntries]; n[i] = [e.target.value, v]; setEnvEntries(n) }} placeholder="KEY" />
                  <input value={v} onChange={(e) => { const n = [...envEntries]; n[i] = [k, e.target.value]; setEnvEntries(n) }} placeholder="VALUE" />
                  <button className="mcp-icon-btn small" onClick={() => setEnvEntries(envEntries.filter((_, j) => j !== i))}>{"\u2716"}</button>
                </div>
              ))}
              <button className="mcp-add-env" onClick={() => setEnvEntries([...envEntries, ["", ""]])}>
                + Add env var
              </button>
            </div>
          </div>
        </div>
        <div className="mcp-form-footer">
          <button className="mcp-btn primary" onClick={submit}>Save</button>
        </div>
      </div>
    </div>
  )
}

function ImportJsonModal({ onImport, onClose }: {
  onImport: (servers: McpServer[]) => void; onClose: () => void
}) {
  const [json, setJson] = useState("")
  const [error, setError] = useState("")

  const doImport = () => {
    setError("")
    let parsed: any
    try { parsed = JSON.parse(json) } catch { setError("Invalid JSON"); return }

    let map: Record<string, any> | undefined
    if (parsed.mcpServers && typeof parsed.mcpServers === "object") {
      map = parsed.mcpServers
    } else if (parsed.mcp && typeof parsed.mcp === "object") {
      map = parsed.mcp
    } else if (typeof parsed === "object" && !Array.isArray(parsed)) {
      map = parsed
    }

    if (!map) { setError("Expected an object of servers"); return }

    const entries = Object.entries(map)
    if (entries.length === 0) { setError("No servers found"); return }

    const servers: McpServer[] = entries.map(([name, cfg]: [string, any]) => {
      let cmd = cfg.command || null
      let args = cfg.args || null
      if (Array.isArray(cmd)) {
        args = cmd.slice(1)
        cmd = cmd[0]
      }
      return {
        name,
        command: cmd,
        args,
        url: cfg.url || null,
        env: cfg.env || cfg.environment || null,
        disabled: cfg.disabled ?? (cfg.enabled === false ? true : false),
        description: null,
        targets: [],
      }
    })

    onImport(servers)
  }

  return (
    <div className="mcp-overlay" onClick={onClose}>
      <div className="mcp-form" onClick={(e) => e.stopPropagation()} style={{ width: 500 }}>
        <div className="mcp-form-header">
          <h2>Import JSON</h2>
          <button className="mcp-icon-btn" onClick={onClose}>{"\u2716"}</button>
        </div>
        <div className="mcp-form-body">
          <div className="mcp-field">
            <span>JSON</span>
            <textarea className="mcp-json-input" value={json} onChange={(e) => setJson(e.target.value)}
              placeholder={`{\n  "my-server": {\n    "command": "npx",\n    "args": ["-y", "package"]\n  }\n}`}
              rows={10} />
          </div>
          {error && <div className="mcp-json-error">{error}</div>}
        </div>
        <div className="mcp-form-footer">
          <button className="mcp-btn primary" onClick={doImport}>Import</button>
        </div>
      </div>
    </div>
  )
}

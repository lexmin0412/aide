import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { Power, PowerOff, Pencil, Trash2, XIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

interface McpServer {
  name: string;
  command: string | null;
  args: string[] | null;
  url: string | null;
  env: Record<string, string> | null;
  disabled: boolean | null;
  description: string | null;
  targets: string[];
}

interface ToolOption {
  key: string;
  name: string;
}

type AddMode = "form" | "json";

export default function MCPPage() {
  const [servers, setServers] = useState<McpServer[]>([]);
  const [tools, setTools] = useState<ToolOption[]>([]);
  const [editing, setEditing] = useState<McpServer | null>(null);
  const [showAdd, setShowAdd] = useState(() => {
    console.log("showAdd 初始化");
    return false;
  });
  const [addMode, setAddMode] = useState<AddMode>("form");
  const [syncing, setSyncing] = useState<string | null>(null);

  const load = async () => {
    const [s, t] = await Promise.all([
      invoke<McpServer[]>("list_mcp_servers"),
      invoke<ToolOption[]>("list_mcp_tools"),
    ]);
    setServers(s);
    setTools(t);
  };
  useEffect(() => {
    console.log("页面挂载");
    load();
  }, []);

  const save = async (updated: McpServer[]) => {
    console.log("trigger save");
    await invoke("save_mcp_servers", { servers: updated });
    setServers(updated);
    setShowAdd(false);
    setEditing(null);
  };
  const remove = async (name: string) => {
    const confirmed = await ask(
      `Delete server "${name}"? This cannot be undone.`,
      { title: "Delete Server", kind: "warning" }
    );
    if (confirmed) save(servers.filter((s) => s.name !== name));
  };
  const toggle = (name: string) =>
    save(
      servers.map((s) =>
        s.name === name ? { ...s, disabled: !s.disabled } : s
      )
    );

  const syncTool = async (key: string) => {
    setSyncing(key);
    try {
      const r = await invoke<any>("sync_mcp_tool", { toolKey: key });
      return r;
    } catch (e) {
      return { skipped: true, message: String(e) };
    } finally {
      setSyncing(null);
    }
  };
  const syncAll = async () => {
    const results = await Promise.all(
      tools.map((t) =>
        syncTool(t.key).catch(() => ({ skipped: true, message: "error" }))
      )
    );
    alert(
      results
        .map(
          (r, i) =>
            `${tools[i].name}: ${r.skipped ? "SKIPPED" : "OK"} - ${r.message}`
        )
        .join("\n")
    );
  };

  console.log("showAdd", showAdd);

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-start justify-between px-6 pt-5 pb-3 border-b border-border shrink-0">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">MCP Servers</h1>
          <p className="text-xs text-muted-foreground mt-0.5">
            {servers.length} server{servers.length !== 1 ? "s" : ""} configured
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              try {
                const results = await invoke<any[]>("import_mcp_all");
                const total = results.reduce(
                  (n: number, r: any) => n + r.imported.length,
                  0
                );
                if (total > 0) load();
                alert(
                  results.length > 0
                    ? results
                        .map(
                          (r: any) =>
                            `${r.source}: ${r.imported.length} imported`
                        )
                        .join("\n")
                    : "No MCP configs found"
                );
              } catch (e) {
                alert("Scan failed: " + e);
              }
            }}
          >
            Scan
          </Button>
          <Button variant="outline" size="sm" onClick={syncAll}>
            Sync All
          </Button>
          <Button
            size="sm"
            onClick={() => {
              setEditing(null);
              setAddMode("form");
              setShowAdd(true);
            }}
          >
            + Add
          </Button>
        </div>
      </div>

      <div className="flex items-center gap-2 px-6 py-2 border-b border-border bg-muted/20 shrink-0">
        <span className="text-xs text-muted-foreground">Sync to:</span>
        {tools.map((t) => (
          <Button
            key={t.key}
            variant="ghost"
            size="sm"
            className="h-6 px-2 text-xs"
            disabled={syncing === t.key}
            onClick={() => syncTool(t.key)}
          >
            {syncing === t.key ? "..." : t.name}
          </Button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {servers.length === 0 ? (
          <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
            No MCP servers configured.
          </div>
        ) : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-3">
            {servers.map((s) => (
              <Card
                key={s.name}
                className={`p-4 flex flex-col gap-2.5 min-h-[140px] ${
                  s.disabled ? "opacity-40" : ""
                }`}
              >
                <div className="flex items-start justify-between gap-2">
                  <div className="flex items-center gap-2 min-w-0 flex-1">
                    <span className="text-sm font-semibold truncate">
                      {s.name}
                    </span>
                    <Badge variant="secondary" className="text-[10px] shrink-0">
                      {s.url ? "SSE" : "STDIO"}
                    </Badge>
                  </div>
                  <div className="flex gap-1 shrink-0">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      onClick={() => toggle(s.name)}
                      title={s.disabled ? "Enable" : "Disable"}
                    >
                      {s.disabled ? (
                        <PowerOff className="size-3 text-muted-foreground" />
                      ) : (
                        <Power className="size-3 text-emerald-400" />
                      )}
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      onClick={() => {
                        setEditing(s);
                        setAddMode("form");
                        setShowAdd(true);
                      }}
                    >
                      <Pencil className="size-3" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-destructive hover:text-destructive"
                      onClick={() => remove(s.name)}
                    >
                      <Trash2 className="size-3" />
                    </Button>
                  </div>
                </div>
                {s.disabled && (
                  <Badge variant="outline" className="text-[10px] w-fit">
                    Disabled
                  </Badge>
                )}
                <div className="flex flex-col gap-1 text-xs text-muted-foreground flex-1">
                  {s.command && (
                    <code className="bg-muted px-1.5 py-0.5 rounded font-mono truncate">
                      {s.command} {s.args?.join(" ")}
                    </code>
                  )}
                  {s.url && (
                    <code className="bg-muted px-1.5 py-0.5 rounded font-mono truncate">
                      {s.url}
                    </code>
                  )}
                </div>
                <div className="flex items-center gap-2 text-[11px] text-muted-foreground/80">
                  {s.targets.length > 0 && (
                    <span className="text-blue-400">
                      {s.targets.length} target{s.targets.length > 1 ? "s" : ""}
                    </span>
                  )}
                  {s.env && Object.keys(s.env).length > 0 && (
                    <span>{Object.keys(s.env).length} env</span>
                  )}
                </div>
              </Card>
            ))}
          </div>
        )}
      </div>

      {showAdd && (
        <AddServerDialog
          key={editing?.name ?? "__new__"}
          mode={addMode}
          onModeChange={setAddMode}
          server={editing}
          tools={tools}
          onSave={(s) => {
            console.log("trigger onSave inner", s);
            if (editing) {
              save(servers.map((x) => (x.name === editing.name ? s : x)));
            } else {
              save([...servers, s]);
            }
          }}
          onImport={(ns) => save([...servers, ...ns])}
          onClose={() => {
            console.log("trigger show close");
            setShowAdd(false);
          }}
        />
      )}
    </div>
  );
}

function AddServerDialog({
  mode,
  onModeChange,
  server,
  tools,
  onSave,
  onImport,
  onClose,
}: {
  mode: AddMode;
  onModeChange: (m: AddMode) => void;
  server: McpServer | null;
  tools: ToolOption[];
  onSave: (s: McpServer) => void;
  onImport: (servers: McpServer[]) => void;
  onClose: () => void;
}) {
  const [formSubmit, setFormSubmit] = useState<(() => void) | null>(null);
  const [importSubmit, setImportSubmit] = useState<(() => void) | null>(null);

  console.log(
    "[AddServerDialog] render, server:",
    server?.name ?? "null",
    "formSubmit:",
    !!formSubmit
  );

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        console.log("[Dialog] onOpenChange:", open);
        if (!open) onClose();
      }}
    >
      <DialogContent
        className="sm:max-w-[520px] flex flex-col"
        showCloseButton={false}
      >
        <DialogHeader className="flex-row items-center justify-between shrink-0">
          <DialogTitle>{server ? "Edit Server" : "Add Server"}</DialogTitle>
          <div className="flex bg-muted rounded-lg p-0.5">
            <button
              onClick={() => onModeChange("form")}
              className={`px-3 py-1 text-xs rounded-md transition-colors ${
                mode === "form"
                  ? "bg-background shadow-sm text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              Form
            </button>
            <button
              onClick={() => onModeChange("json")}
              className={`px-3 py-1 text-xs rounded-md transition-colors ${
                mode === "json"
                  ? "bg-background shadow-sm text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              JSON
            </button>
          </div>
        </DialogHeader>
        <div className="overflow-y-auto h-[360px]">
          {mode === "form" ? (
            <ServerForm
              server={server}
              tools={tools}
              onSave={(...params) => {
                console.log("trigger server form save", ...params);
                onSave(...params);
              }}
              onReady={setFormSubmit}
            />
          ) : (
            <ImportJson onImport={onImport} onReady={setImportSubmit} />
          )}
        </div>
        <div className="flex justify-end gap-2 pt-3 border-t border-border mt-2 shrink-0">
          <Button variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={mode === "form" ? formSubmit! : importSubmit!}>
            {mode === "form" ? (server ? "Save" : "Add") : "Import"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function ServerForm({
  server,
  tools,
  onSave,
  onReady,
}: {
  server: McpServer | null;
  tools: ToolOption[];
  onSave: (s: McpServer) => void;
  onReady: (fn: () => void) => void;
}) {
  const [name, setName] = useState(server?.name || "");
  const [type, setType] = useState(server?.url ? "sse" : "stdio");
  const [command, setCommand] = useState(server?.command || "");
  const [args, setArgs] = useState(server?.args?.join(" ") || "");
  const [url, setUrl] = useState(server?.url || "");
  const [desc, setDesc] = useState(server?.description || "");
  const [targets, setTargets] = useState<string[]>(server?.targets || []);
  const [envEntries, setEnvEntries] = useState<[string, string][]>(
    Object.entries(server?.env || {})
  );

  useEffect(() => {
    setName(server?.name || "");
    setType(server?.url ? "sse" : "stdio");
    setCommand(server?.command || "");
    setArgs(server?.args?.join(" ") || "");
    setUrl(server?.url || "");
    setDesc(server?.description || "");
    setTargets(server?.targets || []);
    setEnvEntries(Object.entries(server?.env || {}));
  }, [server]);

  const submit = () => {
    console.log('trigger submit')
    if (!name.trim()) return;
    onSave({
      name: name.trim(),
      command: type === "stdio" ? command.trim() || null : null,
      args: type === "stdio" && args.trim() ? args.trim().split(/\s+/) : null,
      url: type === "sse" ? url.trim() || null : null,
      env:
        envEntries.length > 0
          ? Object.fromEntries(envEntries.filter(([k]) => k.trim()))
          : null,
      disabled: false,
      description: desc.trim() || null,
      targets,
    });
  };
  const submitRef = useRef(submit);
  submitRef.current = submit;

  useEffect(() => {
    console.log('trigger server useEffect', server)
    onReady(() => () => submitRef.current());
  }, [server]);

  return (
    <div className="space-y-3 py-1">
      <div className="space-y-1">
        <Label>Name</Label>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="my-server"
        />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1">
          <Label>Type</Label>
          <Select value={type} onValueChange={(v) => v && setType(v)}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="stdio">STDIO</SelectItem>
              <SelectItem value="sse">SSE</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label>Description</Label>
          <Input
            value={desc}
            onChange={(e) => setDesc(e.target.value)}
            placeholder="Optional"
          />
        </div>
      </div>
      {type === "stdio" ? (
        <>
          <div className="space-y-1">
            <Label>Command</Label>
            <Input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="npx"
            />
          </div>
          <div className="space-y-1">
            <Label>Args</Label>
            <Input
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              placeholder="-y @modelcontextprotocol/server-filesystem"
            />
          </div>
        </>
      ) : (
        <div className="space-y-1">
          <Label>URL</Label>
          <Input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://"
          />
        </div>
      )}
      <div className="space-y-1">
        <Label>Targets</Label>
        <div className="flex flex-wrap gap-3">
          {tools.map((t) => (
            <label
              key={t.key}
              className="flex items-center gap-1.5 text-sm cursor-pointer"
            >
              <Checkbox
                checked={targets.includes(t.key)}
                onCheckedChange={() =>
                  setTargets((p) =>
                    p.includes(t.key)
                      ? p.filter((k) => k !== t.key)
                      : [...p, t.key]
                  )
                }
              />
              {t.name}
            </label>
          ))}
        </div>
      </div>
      <div className="space-y-1">
        <Label>Environment</Label>
        <div className="space-y-1.5">
          {envEntries.map(([k, v], i) => (
            <div key={i} className="flex gap-1.5 items-center">
              <Input
                className="flex-1 font-mono text-xs"
                value={k}
                onChange={(e) => {
                  const n = [...envEntries];
                  n[i] = [e.target.value, v];
                  setEnvEntries(n);
                }}
                placeholder="KEY"
              />
              <Input
                className="flex-1 font-mono text-xs"
                value={v}
                onChange={(e) => {
                  const n = [...envEntries];
                  n[i] = [k, e.target.value];
                  setEnvEntries(n);
                }}
                placeholder="VALUE"
              />
              <Button
                variant="ghost"
                size="icon"
                className="h-9 w-9 shrink-0"
                onClick={() =>
                  setEnvEntries(envEntries.filter((_, j) => j !== i))
                }
              >
                <XIcon className="size-3" />
              </Button>
            </div>
          ))}
          <Button
            variant="outline"
            size="sm"
            className="w-full"
            onClick={() => setEnvEntries([...envEntries, ["", ""]])}
          >
            + Add env var
          </Button>
        </div>
      </div>
    </div>
  );
}

function ImportJson({
  onImport,
  onReady,
}: {
  onImport: (servers: McpServer[]) => void;
  onReady: (fn: () => void) => void;
}) {
  const [json, setJson] = useState("");
  const [error, setError] = useState("");

  const doImport = () => {
    setError("");
    let parsed: any;
    try {
      parsed = JSON.parse(json);
    } catch {
      setError("Invalid JSON");
      return;
    }
    let map: Record<string, any> | undefined;
    if (parsed.mcpServers && typeof parsed.mcpServers === "object")
      map = parsed.mcpServers;
    else if (parsed.mcp && typeof parsed.mcp === "object") map = parsed.mcp;
    else if (typeof parsed === "object" && !Array.isArray(parsed)) map = parsed;
    if (!map) {
      setError("Expected an object of servers");
      return;
    }
    const entries = Object.entries(map);
    if (entries.length === 0) {
      setError("No servers found");
      return;
    }
    onImport(
      entries.map(([name, cfg]: [string, any]) => {
        let cmd = cfg.command || null;
        let args = cfg.args || null;
        if (Array.isArray(cmd)) {
          args = cmd.slice(1);
          cmd = cmd[0];
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
        };
      })
    );
  };

  const doImportRef = useRef(doImport);
  doImportRef.current = doImport;

  useEffect(() => {
    onReady(() => () => doImportRef.current());
  }, []);

  return (
    <div className="space-y-3 py-1">
      <Textarea
        className="font-mono text-xs min-h-[200px]"
        value={json}
        onChange={(e) => setJson(e.target.value)}
        placeholder={`{\n  "my-server": {\n    "command": "npx",\n    "args": ["-y", "package"]\n  }\n}`}
      />
      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  );
}

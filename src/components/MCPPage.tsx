import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Power, PowerOff, Pencil, Trash2, XIcon, Bot } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { toast } from "@/lib/toast";
import { useTranslation } from "react-i18next";
import { CardGridSkeleton } from "./Skeleton";
import { Switch } from "@/components/ui/switch";
import type { AgentServerStatus } from "../types";
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
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";

interface McpServer {
  name: string;
  command: string | null;
  args: string[] | null;
  url: string | null;
  env: Record<string, string> | null;
  headers: Record<string, string> | null;
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
  const { t } = useTranslation();
  const [servers, setServers] = useState<McpServer[]>([]);
  const [tools, setTools] = useState<ToolOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [editing, setEditing] = useState<McpServer | null>(null);
  const [showAdd, setShowAdd] = useState(false);
  const [addMode, setAddMode] = useState<AddMode>("form");
  const [syncing, setSyncing] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [agent, setAgent] = useState<AgentServerStatus | null>(null);
  const [agentBusy, setAgentBusy] = useState(false);

  const load = async () => {
    const [s, t] = await Promise.all([
      invoke<McpServer[]>("list_mcp_servers"),
      invoke<ToolOption[]>("list_mcp_tools"),
    ]);
    setServers(s);
    setTools(t);
  };
  useEffect(() => {
    load()
      .catch((e) => console.error("Failed to load MCP servers:", e))
      .finally(() => setLoading(false));
    invoke<AgentServerStatus>("get_agent_server")
      .then(setAgent)
      .catch(() => {});
  }, []);

  const save = async (updated: McpServer[]) => {
    try {
      await invoke("save_mcp_servers", { servers: updated });
      setServers(updated);
      setShowAdd(false);
      setEditing(null);
      toast("MCP servers saved", "success");
    } catch (e) {
      toast(t("mcp.saveFailed", { error: String(e) }), "error");
    }
  };
  const remove = async (name: string) => {
    setDeleteTarget(null);
    await save(servers.filter((s) => s.name !== name));
    toast(t("mcp.deletedToast", { name }), "success");
  };
  const toggle = (name: string) =>
    save(
      servers.map((s) =>
        s.name === name ? { ...s, disabled: !s.disabled } : s
      )
    );

  const toggleAgent = async (enable: boolean) => {
    setAgentBusy(true);
    try {
      const status = await invoke<AgentServerStatus>("enable_agent_access", { enable });
      setAgent(status);
      if (enable) {
        toast(
          t("mcp.agentEnabledToast"),
          "success"
        );
      } else {
        toast(t("mcp.agentDisabledToast"), "info");
      }
    } catch (e) {
      toast(t("mcp.agentToggleFailed", { error: String(e) }), "error");
    } finally {
      setAgentBusy(false);
    }
  };

  const syncTool = async (key: string) => {
    setSyncing(key);
    try {
      const r = await invoke<{ skipped: boolean; message: string }>("sync_mcp_tool", { toolKey: key });
      const toolName = tools.find((t) => t.key === key)?.name ?? key;
      if (r.skipped) {
        toast(t("mcp.syncSkippedToast", { tool: toolName, message: r.message }), "info");
      } else {
        toast(t("mcp.syncedToast", { tool: toolName, message: r.message }), "success");
      }
      return r;
    } catch (e) {
      toast(t("mcp.syncFailedToast", { tool: key, error: String(e) }), "error");
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
    const ok = results.filter((r) => !r.skipped).length;
    const skipped = results.length - ok;
    toast(t("mcp.syncAllToast", { ok, skipped }), ok > 0 ? "success" : "info");
  };


  return (
    <div className="h-full flex flex-col">
      <div className="flex items-start justify-between px-6 pt-5 pb-3 border-b border-border shrink-0">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">{t("mcp.title")}</h1>
          <p className="text-xs text-muted-foreground mt-0.5">
            {t("mcp.configured", { count: servers.length })}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              try {
                const results = await invoke<{ source: string; imported: string[] }[]>("import_mcp_all");
                const total = results.reduce(
                  (n: number, r) => n + r.imported.length,
                  0
                );
                if (total > 0) load();
                toast(
                  total > 0
                    ? results.map((r) => `${r.source}: ${r.imported.length} imported`).join("\n")
                    : t("mcp.scanEmpty"),
                  total > 0 ? "success" : "info"
                );
              } catch (e) {
                toast(t("mcp.scanFailed", { error: String(e) }), "error");
              }
            }}
          >
            {t("mcp.scan")}
          </Button>
          <Button variant="outline" size="sm" onClick={syncAll}>
            {t("mcp.syncAll")}
          </Button>
          <Button
            size="sm"
            onClick={() => {
              setEditing(null);
              setAddMode("form");
              setShowAdd(true);
            }}
          >
            {t("mcp.add")}
          </Button>
        </div>
      </div>

      <div className="flex items-center gap-2 px-6 py-2 border-b border-border bg-muted/20 shrink-0">
        <span className="text-xs text-muted-foreground">{t("mcp.syncTo")}</span>
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

      {agent && (
        <div className="flex items-center gap-3 mx-6 mt-4 p-3 rounded-lg border border-border bg-card/60 shrink-0">
          <Bot size={16} className="text-primary shrink-0" />
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium">{t("mcp.agent.title")}</div>
            <p className="text-[11px] text-muted-foreground truncate">
              {agent.enabled
                ? t("mcp.agent.enabled")
                : agent.binary_available
                  ? t("mcp.agent.desc")
                  : t("mcp.agent.noBinary")}
            </p>
          </div>
          <Switch
            checked={agent.enabled}
            disabled={agentBusy || !agent.binary_available}
            onCheckedChange={(v) => void toggleAgent(v)}
          />
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-6">
        {loading ? (
          <CardGridSkeleton count={3} />
        ) : servers.length === 0 ? (
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
                      {s.url ? (s.headers ? "HTTP" : "SSE") : "STDIO"}
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
                      onClick={() => setDeleteTarget(s.name)}
                    >
                      <Trash2 className="size-3" />
                    </Button>
                  </div>
                </div>
                {s.disabled && (
                  <Badge variant="outline" className="text-[10px] w-fit">
                    {t("mcp.disabled")}
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
                <div className="flex flex-wrap items-center gap-1 text-[11px] text-muted-foreground/80">
                  {s.targets.length > 0 ? (
                    s.targets.map((key) => (
                      <Badge key={key} variant="outline" className="text-[9px] px-1 py-0">
                        {tools.find((t) => t.key === key)?.name ?? key}
                      </Badge>
                    ))
                  ) : (
                    <span className="text-primary font-medium">{t("mcp.allTools")}</span>
                  )}
                  {s.env && Object.keys(s.env).length > 0 && (
                    <span>{t("mcp.env", { count: Object.keys(s.env).length })}</span>
                  )}
                </div>
              </Card>
            ))}
          </div>
        )}
      </div>

      {deleteTarget && (
        <Dialog open onOpenChange={(open) => { if (!open) setDeleteTarget(null) }}>
          <DialogContent className="sm:max-w-[400px]">
            <DialogHeader>
              <DialogTitle>{t("mcp.deleteTitle")}</DialogTitle>
              <DialogDescription>
                {t("mcp.deleteDesc", { name: deleteTarget })}
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="ghost" size="sm" onClick={() => setDeleteTarget(null)}>{t("common.cancel")}</Button>
              <Button variant="destructive" size="sm" onClick={() => void remove(deleteTarget)}>{t("common.delete")}</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}

      {showAdd && (
        <AddServerDialog
          key={editing?.name ?? "__new__"}
          mode={addMode}
          onModeChange={setAddMode}
          server={editing}
          tools={tools}
          onSave={(s) => {
            if (editing) {
              save(servers.map((x) => (x.name === editing.name ? s : x)));
            } else {
              save([...servers, s]);
            }
          }}
          onImport={(ns) => save([...servers, ...ns])}
          onClose={() => {
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
  const { t } = useTranslation();
  const [formSubmit, setFormSubmit] = useState<(() => void) | null>(null);
  const [importSubmit, setImportSubmit] = useState<(() => void) | null>(null);

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        className="sm:max-w-[520px] flex flex-col"
        showCloseButton={false}
      >
        <DialogHeader className="flex-row items-center justify-between shrink-0">
          <DialogTitle>{server ? t("mcp.editor.editTitle") : t("mcp.editor.addTitle")}</DialogTitle>
          <div className="flex bg-muted rounded-lg p-0.5">
            <button
              onClick={() => onModeChange("form")}
              className={`px-3 py-1 text-xs rounded-md transition-colors ${
                mode === "form"
                  ? "bg-background shadow-sm text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t("mcp.editor.form")}
            </button>
            <button
              onClick={() => onModeChange("json")}
              className={`px-3 py-1 text-xs rounded-md transition-colors ${
                mode === "json"
                  ? "bg-background shadow-sm text-foreground"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t("mcp.editor.json")}
            </button>
          </div>
        </DialogHeader>
        <div className="overflow-y-auto h-[360px]">
          {mode === "form" ? (
            <ServerForm
              server={server}
              tools={tools}
              onSave={(...params) => {
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
            {t("common.cancel")}
          </Button>
          <Button onClick={mode === "form" ? formSubmit! : importSubmit!}>
            {mode === "form" ? (server ? t("mcp.editor.save") : t("mcp.editor.add")) : t("mcp.editor.import")}
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
  const { t } = useTranslation();
  const [name, setName] = useState(server?.name || "");
  const [type, setType] = useState(server?.url ? (server?.headers ? "streamable_http" : "sse") : "stdio");
  const [command, setCommand] = useState(server?.command || "");
  const [args, setArgs] = useState(server?.args?.join(" ") || "");
  const [url, setUrl] = useState(server?.url || "");
  const [desc, setDesc] = useState(server?.description || "");
  const [targets, setTargets] = useState<string[]>(server?.targets || []);
  const [envEntries, setEnvEntries] = useState<[string, string][]>(
    Object.entries(server?.env || {})
  );
  const [headersEntries, setHeadersEntries] = useState<[string, string][]>(
    Object.entries(server?.headers || {})
  );

  useEffect(() => {
    setName(server?.name || "");
    setType(server?.url ? (server?.headers ? "streamable_http" : "sse") : "stdio");
    setCommand(server?.command || "");
    setArgs(server?.args?.join(" ") || "");
    setUrl(server?.url || "");
    setDesc(server?.description || "");
    setTargets(server?.targets || []);
    setEnvEntries(Object.entries(server?.env || {}));
    setHeadersEntries(Object.entries(server?.headers || {}));
  }, [server]);

  const submit = () => {
    if (!name.trim()) return;
    // Args accept either a JSON array (preserves spaces/quotes) or
    // whitespace-separated tokens.
    let parsedArgs: string[] | null = null;
    const trimmedArgs = args.trim();
    if (trimmedArgs.startsWith("[")) {
      try {
        const arr = JSON.parse(trimmedArgs);
        if (Array.isArray(arr) && arr.every((x) => typeof x === "string")) {
          parsedArgs = arr;
        }
      } catch {}
    }
    onSave({
      name: name.trim(),
      command: type === "stdio" ? command.trim() || null : null,
      args:
        type === "stdio" && trimmedArgs
          ? parsedArgs ?? trimmedArgs.split(/\s+/)
          : null,
      url: type !== "stdio" ? url.trim() || null : null,
      env:
        envEntries.length > 0
          ? Object.fromEntries(envEntries.filter(([k]) => k.trim()))
          : null,
      headers:
        type === "streamable_http" && headersEntries.length > 0
          ? Object.fromEntries(headersEntries.filter(([k]) => k.trim()))
          : null,
      disabled: false,
      description: desc.trim() || null,
      targets,
    });
  };
  const submitRef = useRef(submit);
  submitRef.current = submit;

  useEffect(() => {
    onReady(() => () => submitRef.current());
  }, [server]);

  return (
    <div className="space-y-3 py-1">
      <div className="space-y-1">
        <Label>{t("mcp.editor.name")}</Label>
        <Input
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder={t("mcp.editor.namePlaceholder")}
        />
      </div>
      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1">
          <Label>{t("mcp.editor.type")}</Label>
          <Select value={type} onValueChange={(v) => v && setType(v)}>
            <SelectTrigger className="w-full">
              <SelectValue>
                {type === "stdio" ? t("mcp.editor.typeStdio") : type === "sse" ? t("mcp.editor.typeSse") : t("mcp.editor.typeHttp")}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="stdio">STDIO</SelectItem>
              <SelectItem value="sse">SSE</SelectItem>
              <SelectItem value="streamable_http">Streamable HTTP</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <div className="space-y-1">
          <Label>{t("mcp.editor.description")}</Label>
          <Input
            value={desc}
            onChange={(e) => setDesc(e.target.value)}
            placeholder={t("mcp.editor.descriptionPlaceholder")}
          />
        </div>
      </div>
      {type === "stdio" ? (
        <>
          <div className="space-y-1">
            <Label>{t("mcp.editor.command")}</Label>
            <Input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder={t("mcp.editor.commandPlaceholder")}
            />
          </div>
          <div className="space-y-1">
            <Label>{t("mcp.editor.args")}</Label>
            <Input
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              placeholder={t("mcp.editor.argsPlaceholder")}
            />
          </div>
        </>
      ) : (
        <>
          <div className="space-y-1">
            <Label>{t("mcp.editor.url")}</Label>
            <Input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://"
            />
          </div>
          {type === "streamable_http" && (
            <div className="space-y-1">
              <Label>{t("mcp.editor.headers")}</Label>
              <div className="space-y-1.5">
                {headersEntries.map(([k, v], i) => (
                  <div key={i} className="flex gap-1.5 items-center">
                    <Input
                      className="flex-1 font-mono text-xs"
                      value={k}
                      onChange={(e) => {
                        const n = [...headersEntries];
                        n[i] = [e.target.value, v];
                        setHeadersEntries(n);
                      }}
                      placeholder={t("mcp.editor.headerName")}
                    />
                    <Input
                      className="flex-1 font-mono text-xs"
                      value={v}
                      onChange={(e) => {
                        const n = [...headersEntries];
                        n[i] = [k, e.target.value];
                        setHeadersEntries(n);
                      }}
                      placeholder="value"
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-9 w-9 shrink-0"
                      onClick={() =>
                        setHeadersEntries(headersEntries.filter((_, j) => j !== i))
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
                  onClick={() => setHeadersEntries([...headersEntries, ["", ""]])}
                >
                  {t("mcp.editor.addHeader")}
                </Button>
              </div>
            </div>
          )}
        </>
      )}
      <div className="space-y-1">
        <Label>{t("mcp.editor.targets")}</Label>
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
      {type === "stdio" && (
        <div className="space-y-1">
          <Label>{t("mcp.editor.env")}</Label>
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
            {t("mcp.editor.addEnv")}
          </Button>
        </div>
      </div>
      )}
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
  const { t } = useTranslation();
  const [json, setJson] = useState("");
  const [error, setError] = useState("");

  const doImport = () => {
    setError("");
    let parsed: any;
    try {
      parsed = JSON.parse(json);
    } catch {
      setError(t("mcp.editor.invalidJson"));
      return;
    }
    let map: Record<string, any> | undefined;
    if (parsed.mcpServers && typeof parsed.mcpServers === "object")
      map = parsed.mcpServers;
    else if (parsed.mcp && typeof parsed.mcp === "object") map = parsed.mcp;
    else if (typeof parsed === "object" && !Array.isArray(parsed)) map = parsed;
    if (!map) {
      setError(t("mcp.editor.expectedObject"));
      return;
    }
    const entries = Object.entries(map);
    if (entries.length === 0) {
      setError(t("mcp.editor.noServers"));
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
          headers: cfg.headers || null,
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
        placeholder={t("mcp.editor.importPlaceholder")}
      />
      {error && <p className="text-sm text-destructive">{error}</p>}
    </div>
  );
}

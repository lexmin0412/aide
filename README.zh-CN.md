# aide（中文文档）

[English](./README.md) | 简体中文

**aide** 是一个 AI 开发环境配置管理器：把散落在各个 AI 编程工具里的 Skills、MCP、配置文件统一到一个桌面应用里管理。

支持 43 个工具：Claude Code、OpenCode、Codex、Gemini CLI、Cursor、Windsurf、Zed、Cline、Roo Code、Kilo Code、Goose、GitHub Copilot、Trae / Trae CN、Qwen Code、iFlow CLI、Kimi Code、MiniMax Code、CodeBuddy、Qoder、Lingma（通义灵码）、MiMo Code 等。

## 核心能力

### 技能管理

- **技能市场**：搜索 skills.sh 社区注册表（上百万技能），按 `owner/repo[/skill]` 从 GitHub 直接安装，无需 git 命令
- **全生命周期**：新建（脚手架）、本地文件夹导入、编辑（CodeMirror + Markdown 预览）、标签、全文搜索、回收站删除
- **一键同步**：`~/.agents/skills` 作为唯一真源，符号链接（Windows 为 junction）链接到各工具目录；原生支持 `.agents/skills` 的工具零配置
- **安全设计**：同名冲突的工具侧副本自动备份到 `~/.aide/sync-backup`，绝不静默覆盖

### 技能更新闭环

安装来源（provenance）记录在 `~/.aide/skill-sources.json`。aide 通过 GitHub API 比对安装时的 commit 与上游最新 commit：

- 技能页 Browse 按钮显示可更新数量的角标
- 市场对话框顶部列出待更新技能，一键更新（被替换的内容进回收站，可恢复）

### Git 备份与共享（多 scope）

一个痛点：`~/.agents/skills` 里既有个人技能也有公司技能，应该推到不同的仓库。aide 的方案：

1. 在 `~/.aide/remotes.json`（或 UI 中）定义多个命名 scope：`personal → 你的仓库`、`work → 公司私有仓库`
2. 每个技能指派到一个 scope，未指派的跟随默认 scope
3. **Publish** = 把该 scope 的技能镜像到独立工作树 → commit → push（遇到非快进自动 rebase 重试）
4. **Pull** = 快进拉取后回写本地（远端为准，绝不删除本地独有技能）

公司技能永远不会进个人仓库，反之亦然。Git 走系统 `git`，直接继承你的 SSH agent / gh 凭证。

### Agent 接入（MCP Server）

 aide 可以把自己暴露成一个 MCP 服务器，让 AI agent 通过协议自主管理技能：

- MCP 页打开 **Agent access** 开关，aide 会把内置的 `aide-mcp` 二进制注册进中心 MCP 配置，同步后 agent 侧即可使用
- 暴露 8 个工具：`list_skills`、`search_skills`、`search_registry`、`install_skill`、`check_updates`、`update_skill`、`publish_scope`、`pull_scope`
- Claude Code 配置示例（同步后自动写入；手动配置则指向安装目录下的 `aide-mcp` 可执行文件）：

```json
{
  "mcpServers": {
    "aide": {
      "command": "/Applications/aide.app/Contents/MacOS/aide-mcp"
    }
  }
}
```

之后你可以直接对 agent 说："帮我找一个处理 TAPD 需求的技能并安装"，agent 会调用 `search_registry` → `install_skill` 完成全流程。

### CLI

`aide-mcp` 同时是一个人类可用的命令行（与 GUI 共享同一 Rust 核心）：

```bash
aide-mcp list                    # 列出已装技能
aide-mcp search <query>          # 技能内全文搜索
aide-mcp search-registry <query> # 搜索社区注册表
aide-mcp install <id>            # 安装，如 vercel-labs/agent-skills@web-design-guidelines
aide-mcp updates                 # 查看待更新
aide-mcp update <skill>          # 更新指定技能
aide-mcp publish [scope]         # 发布 scope 到远端
aide-mcp pull [scope]            # 从远端拉取
aide-mcp serve                   # 运行 MCP stdio 服务器（默认）
```

### MCP 管理

中心化 MCP 配置（`~/.aide/mcp.json`）+ 一键同步到各工具的专属格式（JSON/JSONC/TOML）。同步是 **upsert 合并**：aide 管的 server 覆盖同名项，你在工具端手动添加的 server 原样保留。支持从所有已装工具反向导入（Scan）。

## 开发

```bash
pnpm install
pnpm tauri dev      # 开发（热更新）
pnpm build          # TypeScript 检查 + 前端构建
pnpm tauri build    # 生产构建 + 打包（三平台 CI 自动发布）
cargo test          # Rust 单元测试（src-tauri 下）
```

## 设计

- 主题："Blueprint"——深茄紫墨暗色 / 冷调纸白亮色，IBM Plex 字体，电光紫强调
- 技能目录结构永远不被改写；scope 通过 `~/.aide/scopes/<name>` 的镜像工作树实现，零迁移成本

## License

MIT

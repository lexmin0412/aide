---
name: release-aide
description: >
  aide 项目的正式发版流程。修改版本号、生成 changelog、打 tag 并推送触发 CI 构建。
  当用户说"发版"、"发布"、"release"、"打 tag"、"发新版本"时触发。
  仅在 aide 仓库下使用。
---

# Release Skill — aide

## 工作流程

### 1. 确定版本号

1. 获取当前版本：
   ```bash
   grep '"version"' package.json | head -1
   grep '"version"' src-tauri/tauri.conf.json | head -1
   ```

2. 获取最新 tag：
   ```bash
   git tag --sort=-v:refname | head -1
   ```

3. 询问用户目标版本号，建议遵循 semver（根据改动内容推荐 bump major/minor/patch）

### 2. 生成 Release Notes

1. 获取当前 tag 到 HEAD 之间的 commit：
   ```bash
   PREV_TAG=$(git tag --sort=-v:refname | head -1)
   git log "${PREV_TAG}..HEAD" --oneline --no-decorate
   ```

2. 同时对比代码差异了解改动全貌：
   ```bash
   git diff "${PREV_TAG}..HEAD" --stat
   ```

3. 综合 commit 信息 + 代码 diff，生成结构化的 changelog，写入 `.release-notes.md`：
   ```bash
   cat > .release-notes.md << 'EOF'
   ## v<version> (YYYY-MM-DD)

   ### Features

   - ...

   ### Bug Fixes

   - ...
   EOF
   ```

### 3. 用户确认

展示以下信息给用户确认：

1. **新版本号**
2. **Changelog**（来自 `.release-notes.md`）
3. **待修改的文件**：package.json、src-tauri/tauri.conf.json

等待用户确认 yes/no。如需调整 changelog，直接编辑 `.release-notes.md` 再确认。

### 4. 执行发版

用户确认后：

1. 更新 `package.json` 中的 `version` 字段
2. 更新 `src-tauri/tauri.conf.json` 中的 `version` 字段
3. 提交（包含 `.release-notes.md`）：
   ```bash
   git add package.json src-tauri/tauri.conf.json .release-notes.md
   git commit -m "chore(release): v<version>"
   ```
4. 打 tag：
   ```bash
   git tag v<version>
   ```
5. 推送代码和 tag：
   ```bash
   git push && git push --tags
   ```

### 5. 发布 Release

等待 CI 构建完成（可以在 Actions 页面观察进度）：
1. 确认 build workflow 成功
2. CI 会自动创建 GitHub Release（包含产物 + `.release-notes.md` 内容作为 notes）
3. 将 Release 链接发送给用户确认

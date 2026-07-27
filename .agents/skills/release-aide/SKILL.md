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

### 2. 生成 Changelog

1. 获取当前 tag 到 HEAD 之间的 commit：
   ```bash
   PREV_TAG=$(git tag --sort=-v:refname | head -1)
   git log "${PREV_TAG}..HEAD" --oneline --no-decorate
   ```

2. 同时对比代码差异了解改动全貌：
   ```bash
   git diff "${PREV_TAG}..HEAD" --stat
   ```

3. 综合 commit 信息 + 代码 diff，生成结构化的 changelog，按类别组织：
   - **New Features**
   - **Bug Fixes**
   - **Refactoring**
   - **Other Changes**

### 3. 用户确认

展示以下信息给用户确认：

1. **新版本号**
2. **Changelog**（markdown 格式）
3. **待修改的文件**：package.json、src-tauri/tauri.conf.json

等待用户确认 yes/no。

### 4. 执行发版

用户确认后：

1. 更新 `package.json` 中的 `version` 字段
2. 更新 `src-tauri/tauri.conf.json` 中的 `version` 字段
3. 清理 changelog 中不必要的信息
4. 提交：
   ```bash
   git add package.json src-tauri/tauri.conf.json
   git commit -m "chore(release): v<version>"
   ```
5. 打 tag：
   ```bash
   git tag v<version>
   ```
6. 推送代码和 tag：
   ```bash
   git push && git push --tags
   ```

### 5. 发布 Release

等待 CI 构建完成（可以在 Actions 页面观察进度）：
1. 确认 build workflow 成功
2. 从 Actions artifact 下载产物
3. 创建 GitHub Release（包含可执行文件和更新包）：
   ```bash
   gh release create v<version> \
     --title "v<version>" \
     --notes "<changelog>" \
     aide.app.zip \
     aide-macos/src-tauri/target/release/bundle/dmg/aide_<version>_aarch64.dmg \
     aide-macos/src-tauri/target/release/bundle/macos/aide_<version>_aarch64.tar.gz \
     aide-macos/src-tauri/target/release/bundle/macos/aide_<version>_aarch64.tar.gz.sig \
     aide-macos/src-tauri/target/release/bundle/macos/latest.json
   ```
4. 将 Release 链接发送给用户确认

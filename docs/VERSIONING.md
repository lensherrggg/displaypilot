# 版本管理

DisplayPilot 使用 **语义化版本（SemVer）**，初始版本为 **0.1.0**，当前版本为 **0.2.0**。版本号同时出现在前端清单、
Tauri 配置、Cargo 清单、CLI 输出、托盘提示、设置页与诊断报告中；任何一处不一致都会导致检查失败。

## 1. 版本语义

格式：`MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]`。

| 阶段 | 规则 |
| --- | --- |
| `0.x` | 初始开发阶段。`MINOR` 可以包含不兼容的配置或行为变更；`PATCH` 用于修复和小改进。 |
| `1.0.0+` | `MAJOR` 不兼容变更；`MINOR` 向后兼容的新功能；`PATCH` 向后兼容的修复。 |
| 预发布 | `0.2.0-beta.1`、`1.0.0-rc.1`；排序低于同版本正式版。 |
| 构建元数据 | `+build.1` 仅用于标识构建，不参与优先级。 |

对本项目而言，以下变更至少提升 `MINOR`：

- 修改 TOML 配置结构、移除或重命名字段；
- 改变默认行为（例如默认自动隐藏、能力探测策略）；
- 改变 CLI 参数、输出字段或退出码；
- 改变内置档案的匹配规则或输入值，导致用户看到不同结果。

仅 UI 文案、内部重构、性能优化、日志改进通常提升 `PATCH`。安全修复即使是小改动也应尽快发布，
必要时提升 `MINOR`。

## 2. 版本的单一来源

| 文件 | 字段 | 用途 |
| --- | --- | --- |
| `package.json` | `version` | npm/Vite 生态、检查脚本的基准值。 |
| `src-tauri/tauri.conf.json` | `version` | Tauri 打包与安装包版本。 |
| `src-tauri/Cargo.toml` | `[package].version` | Rust 二进制、`CARGO_PKG_VERSION`、诊断与 CLI 版本。 |
| `src-tauri/Cargo.lock` | `displaypilot` 包版本 | 由脚本同步，保证锁定文件一致。 |
| `CHANGELOG.md` | `## [<version>]` | 发布说明；版本检查要求存在对应小节。 |

运行时版本不单独维护：托盘 tooltip、`displaypilot --version`、设置页「关于」和诊断报告都读取
`CARGO_PKG_VERSION`，因此只要 Cargo 清单正确，展示值就一致。

## 3. 脚本

### 检查一致性

```bash
npm run check:version
```

检查内容：

1. `package.json`、`tauri.conf.json`、`Cargo.toml` 的版本完全一致；
2. 若 `src-tauri/Cargo.lock` 已生成且包含 `displaypilot` 包，其版本也一致；
3. 版本符合 SemVer 正则（含可选的预发布 / 构建后缀）；
4. `CHANGELOG.md` 包含 `## [<version>]` 小节。

`npm run check` 会先执行该检查，再构建前端、运行 Rust 测试与 `fmt/clippy`。

### 设置新版本

```bash
npm run version:set -- 0.2.0
```

脚本会更新：

- `package.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock`（存在 `displaypilot` 包时）

注意 npm 传参需要 `--` 分隔。脚本**不会**修改 `CHANGELOG.md`，需要手动增加小节；这是为了避免
生成空泛的发布说明。

## 4. CHANGELOG 约定

遵循 Keep a Changelog，按版本倒序排列：

```markdown
## [Unreleased]

## [0.2.0] - 2026-11-15

### 新增
- ...

### 变更
- ...

### 修复
- ...

### 移除
- ...
```

规则：

- 日期使用 `YYYY-MM-DD`，并在发布当天填写；
- 使用面向用户的描述，说明影响与迁移方式，不粘贴提交标题堆砌；
- 破坏性变更在「变更」或「移除」中明确写出旧配置如何迁移；
- `Unreleased` 用于积累尚未发版的改动；
- 发布时把 `Unreleased` 的内容移到新版本小节，并保留空的 `Unreleased`。

## 5. 发布流程

1. **确认工作区干净**：`git status`，所有改动已提交或明确暂存。
2. **决定版本号**：根据变更类型选择 `PATCH` / `MINOR` / `MAJOR`（`0.x` 阶段参考第 1 节）。
3. **更新版本**：
   ```bash
   npm run version:set -- 0.2.0
   ```
4. **更新 CHANGELOG**：把 `Unreleased` 内容整理到 `## [0.2.0] - YYYY-MM-DD`，补上空 `Unreleased`。
5. **完整检查**：
   ```bash
   npm run check
   ```
   需要额外确认打包时：
   ```bash
   npm run tauri build
   ```
6. **提交**：
   ```bash
   git add package.json package-lock.json src-tauri/tauri.conf.json \
           src-tauri/Cargo.toml src-tauri/Cargo.lock CHANGELOG.md
   git commit -m "chore(release): v0.2.0"
   ```
7. **打标签**（附注标签，便于追溯）：
   ```bash
   git tag -a v0.2.0 -m "DisplayPilot v0.2.0"
   ```
8. **推送**（如有远端）：
   ```bash
   git push origin main --follow-tags
   ```
9. **构建产物**：从 `src-tauri/target/release/bundle/` 取安装包；在发布说明中附上平台、架构与校验值。

发布后不要修改已推送的标签；修复问题应发布新的 `PATCH` 版本。需要撤回时优先发布新版本并在
CHANGELOG 说明，而不是强制移动标签。

## 6. 提交与分支约定

仓库使用 `main` 作为稳定分支。提交信息采用 Conventional Commits 风格，便于生成变更摘要：

| 类型 | 用途 | 示例 |
| --- | --- | --- |
| `feat` | 新功能 | `feat(panel): add display switcher` |
| `fix` | 修复 | `fix(ddc): invalidate snapshot after failed switch` |
| `docs` | 文档 | `docs: add configuration reference` |
| `refactor` | 重构 | `refactor(worker): serialize DDC requests` |
| `test` | 测试 | `test(service): cover auto audio probing` |
| `chore` | 构建/工具/版本 | `chore(release): v0.1.0` |

提交应保持可构建、可测试。较大的功能可以拆成多个逻辑提交，但不要提交 `node_modules/`、
`dist/`、`src-tauri/target/` 或日志文件（已在 `.gitignore` 中忽略）。

## 7. 0.1.0 基线

- 初始基线：`0.1.0`，日期 `2026-10-27`。
- 首次发布标签：`v0.1.0`。
- 该版本固定了配置 `schema_version = 1`。未来若破坏配置结构，必须提升 `schema_version`，
  并在 CHANGELOG 与迁移说明中写清楚。
- `0.1.0` 的已知限制（macOS 未实机验收、厂商输入值差异等）见 [CHANGELOG](../CHANGELOG.md)。

## 8. 常见问题

**版本检查失败：`Version mismatch detected`**  
说明三处清单不一致。运行 `npm run version:set -- <目标版本>` 重新同步，再补 CHANGELOG。

**`CHANGELOG.md is missing a "## [x.y.z]" section`**  
为当前版本添加小节；标题必须严格包含 `## [0.2.0]` 形式（可带日期）。

**`npm run version:set 0.2.0` 没有生效**  
npm 需要 `--` 传递参数：`npm run version:set -- 0.2.0`。

**Cargo.lock 没有更新**  
首次构建前可能不存在，脚本会跳过；运行一次 `cargo build` 后再执行设置脚本即可。

**预发布版本如何检查**  
`0.2.0-beta.1` 是合法 SemVer；CHANGELOG 必须写 `## [0.2.0-beta.1]`。正式发布时改为
`## [0.2.0]` 并更新版本号。

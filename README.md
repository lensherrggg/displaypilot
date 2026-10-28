# DisplayPilot

DisplayPilot 是一个跨平台的菜单栏 / 系统托盘小工具，用 DDC/CI 协议直接控制显示器：
在多台主机之间切换输入源，并调节亮度、音量与静音。它面向“一台显示器、多台电脑”的桌面场景，
不需要切换 KVM，也不依赖网络或云端。

- **输入源切换**：HDMI 1、HDMI 2、DisplayPort、USB-C（由显示器配置定义，可自行增删）。
- **亮度与音频**：亮度、音量、静音，滑块防抖后写入，按显示器真实 VCP 范围换算。
- **配置驱动**：所有厂商差异都写在 TOML 里；内置 LG 27UL850（Windows 与 macOS 两套身份）与通用 DDC/CI 档案，但没有机型硬编码。
- **键盘友好**：可配置全局快捷键；面板与设置页支持键盘、VoiceOver 和减少动态效果。
- **可诊断**：复制包含显示器身份、VCP 读数与能力字符串的报告，便于定位固件差异。
- **命令行兜底**：图形界面之外保留 `list / switch / brightness / volume / mute / diagnose / config` 子命令。

> 当前版本 **0.3.0**。Windows 已完成构建、CLI 与界面冒烟验证，并在 LG 27UL850 上实测 DDC/CI：
> HDMI / DP 输入切换、亮度、音量与静音均可用；LG USB-C 所需的厂商专用寻址
> （VCP `0xF4` @ DDC 源地址 `0x50`）在 Windows 上通过可插拔的 NVAPI 原始 I2C 后端自动回退，
> 没有原始 I2C 后端的 GPU / 平台会明确把端口标记为“不支持”。macOS 已在 Apple Silicon + LG 27UL850
> （LG HDR 4K）上完成菜单栏、DDC/CI 读取与真实写入验证。

## 界面

| 窗口 | 说明 |
| --- | --- |
| 控制面板 | 360×620 的无边框浮层。macOS 锚定在菜单栏状态项下方，失去焦点自动隐藏，`Esc` 关闭；Windows 显示在工作区右下角。包含显示器选择、输入源网格、亮度滑块、音量 / 静音、状态行与「复制诊断」。 |
| 设置窗口 | 独立可调整窗口。修改亮度 / 音量步进、自动隐藏、音频占位显示；查看快捷键注册状态；刷新显示器；查看配置档、配置文件与日志路径。 |

外观遵循 macOS Human Interface Guidelines 的核心约束：系统字体、语义化颜色、8pt 间距体系、
圆角卡片、浅色 / 深色 / 高对比度、150–200ms 过渡，并在 `prefers-reduced-motion` 下关闭动画。
Windows 上使用同布局的 CSS 半透明面板；macOS 上额外应用 vibrancy，并在不可用时静默降级。

## 支持平台

| 平台 | 状态 | 说明 |
| --- | --- | --- |
| Windows 10/11 x64 | 支持，已在 LG 27UL850 上验证 DDC/CI | 通过 `ddc-winapi` 后端访问显示器；需要 WebView2 Runtime 与 MSVC 工具链。NVIDIA 显卡额外启用 NVAPI 原始 I2C，可发送 LG 厂商专用寻址（`0xF4` @ `0x50`）并在标准 DDC/CI 被忽略时自动回退；AMD / Intel 显卡没有公开的原始 I2C 接口，相关端口会明确标记为“不支持”。 |
| macOS 11+（Intel / Apple Silicon） | 支持，已在 Apple Silicon + LG 27UL850 上实机验证 | 通过 `ddc-macos` 后端访问；以 Accessory / `LSUIElement` 模式运行（无 Dock 图标），面板使用 vibrancy 并锚定到菜单栏状态项。原始 I2C（IOAVService）尚未实现，需要厂商专用寻址的端口会标记为“不支持”。 |
| Linux | 未正式支持 | 依赖层可能可用，但未提供打包、测试与桌面集成；原始 I2C 后端未实现。 |

DDC/CI 的硬性前提：

1. 显示器已开启并处于唤醒状态；
2. 显示器 OSD 中已启用 **DDC/CI**（不同品牌叫法可能是 DDC/CI、Display Data Channel）；
3. 查询 / 控制只发送到**当前活动输入源所在的那台主机**。切换到另一台主机后，原主机无法再控制该显示器；
4. 部分扩展坞、转接器和 KVM 不传递 DDC/CI，建议先用直连 HDMI / DP 验证。

## 快速开始

### 依赖

- Rust **1.90+**（使用 `rustup` 安装 stable 工具链）
- Node.js **18+** 与 npm
- Windows：MSVC Build Tools、WebView2 Runtime
- macOS：Xcode Command Line Tools

### 开发运行

```bash
npm install
npm run tauri dev
```

第一条命令会构建前端，`tauri dev` 会编译 Rust 后端并启动托盘应用。首次启动会在配置目录生成
`config.toml`，并自动显示控制面板。

仅调试界面（不访问 DDC/CI，使用内置 mock 数据）：

```bash
npm run dev
# 浏览器打开 http://127.0.0.1:1420/index.html 与 /settings.html
```

### 构建安装包

```bash
npm run tauri build
```

产物位于 `src-tauri/target/release/`：可执行文件为 `displaypilot`（Windows 为 `displaypilot.exe`），
安装包在 `src-tauri/target/release/bundle/`。macOS 会生成 `DisplayPilot.app` 与 `.dmg`；本地构建默认
做 ad-hoc 签名，分发给其他机器时需要在系统设置中允许打开，正式分发应使用 Developer ID 签名并公证。

## 使用

### 托盘与面板

- 左键单击托盘 / 状态栏图标：显示或隐藏面板；右键：打开面板、设置、刷新显示器、退出。
- macOS 上应用不显示 Dock 图标。首次启动时面板显示在工作区右上角；之后点击状态项、使用快捷键或
  再次打开应用，面板会居中显示在状态项正下方，并按屏幕工作区自动夹紧。
- macOS 的「设置…」也可以通过应用菜单的 `⌘,` 打开。
- 面板顶部可切换显示器（多台时出现下拉菜单）。
- 点击输入源卡片执行切换，切换期间按钮进入忙碌状态并阻止重复点击。
- 亮度 / 音量滑块在停止拖动约 **130ms** 后写入；方向键与快捷键按设置的步进调整。
- 音量或静音读取失败时，控件会按配置禁用并显示原因；可在设置中隐藏不支持的音频区域。
- 底部「复制诊断」把显示器身份、配置档、当前输入、VCP 读数、能力字符串和提示写入剪贴板。
- 错误以面板内联提示呈现，不弹模态对话框，并附带可操作的建议。

### 快捷键

默认全部留空，避免占用系统组合键。在 `config.toml` 的 `[shortcuts]` 中填写
[global-hotkey](https://docs.rs/global-hotkey/) 语法，然后在设置页点击「重新加载配置」：

```toml
[shortcuts]
toggle_panel = "Control+Alt+KeyH"
brightness_up = "Control+Alt+ArrowUp"
brightness_down = "Control+Alt+ArrowDown"
volume_up = "Control+Alt+Equal"
volume_down = "Control+Alt+Minus"
mute = "Control+Alt+KeyM"

[[shortcuts.input_switch]]
input = "hdmi1"
accelerator = "Control+Alt+Digit1"

[[shortcuts.input_switch]]
input = "hdmi2"
accelerator = "Control+Alt+Digit2"
```

输入源快捷键会作用于当前选中的显示器，并要求该输入 id 存在于对应配置档中。设置页会显示
每条快捷键是否注册成功；被其他应用占用时会给出错误信息。

### 命令行兜底

图形界面启动后也保留 CLI。无参数启动 GUI；带子命令时执行 CLI 并退出。可执行文件位于
`src-tauri/target/release/displaypilot`（Windows 加 `.exe`）。

```bash
displaypilot list                       # 列出显示器、配置档与输入源
displaypilot list --json                # 机器可读输出
displaypilot switch dp                  # 切换到 DP
displaypilot switch hdmi2 --display 2   # 指定第二台显示器
displaypilot brightness                 # 读取当前亮度
displaypilot brightness 60              # 设置亮度
displaypilot brightness +5              # 相对调整
displaypilot volume 30
displaypilot mute toggle
displaypilot diagnose --json            # 诊断报告
displaypilot config                     # 配置、配置档、快捷键与路径
displaypilot --help
```

公共选项：

| 选项 | 说明 |
| --- | --- |
| `--display <id|序号|名称>` | 选择显示器；默认第一台。序号从 1 开始，名称支持唯一子串匹配。 |
| `--json` | 输出 JSON；失败时错误也以 JSON 写到 stderr。 |
| `-h, --help` / `-V, --version` | 帮助与版本。 |

退出码：`0` 成功；`1` 操作失败；`2` 未发现显示器；`64` 参数错误。

开发时也可用 `cargo run --manifest-path src-tauri/Cargo.toml -- list`。

## 配置

首次启动生成的默认文件包含可用设置与注释示例：

| 平台 | 配置文件 | 日志 |
| --- | --- | --- |
| Windows | `%APPDATA%\DisplayPilot\config.toml` | `%LOCALAPPDATA%\DisplayPilot\logs\displaypilot.log` |
| macOS | `~/Library/Application Support/DisplayPilot/config.toml` | `~/Library/Logs/DisplayPilot/displaypilot.log` |

也可以从设置页「文件与日志」或 CLI `config` 查看实际路径。

- `[general]`：亮度 / 音量步进（1–50）、失去焦点自动隐藏、是否显示不支持的音频控件。
- `[shortcuts]`：全局快捷键；空字符串表示未设置。
- `[profile_overrides.<内置 id>]`：只覆盖需要改动的内置字段；嵌套表深合并，数组整体替换。
- `[[custom_profiles]]`：为未内置的显示器新增完整档案，可匹配厂商、型号、型号名通配、序列号与后端。

配置解析失败时不会崩溃：程序回退到内置默认设置，并在面板 / 设置页显示错误。
修改文件后点击设置页的「重新加载配置」即可生效；从设置页保存的步进与开关会保留文件中的注释。

完整的字段、示例与排错方法见 **[docs/CONFIGURATION.md](docs/CONFIGURATION.md)**。

## 工作原理

```
┌─────────────────────────────┐
│ Tauri WebView（面板 / 设置） │
└──────────────┬──────────────┘
               │ IPC（camelCase JSON）
┌──────────────▼──────────────┐
│ Tauri commands              │
│  └─ WorkerClient（分级超时）  │
└──────────────┬──────────────┘
               │ mpsc + 请求 id
┌──────────────▼──────────────┐
│ 专用工作线程                 │
│  └─ DisplayService          │
│      ├─ ProfileCatalog      │
│      ├─ DdcTransport        │
│      └─ RawI2cDevice        │
└──────────────┬──────────────┘
               │ ddc-hi（winapi / macos）+ 可选原始 I2C（NVAPI）
┌──────────────▼──────────────┐
        显示器 DDC/CI
```

- DDC 句柄不是 `Send`，因此只存在于一个专用线程；所有硬件访问串行执行，IPC 调用通过
  `spawn_blocking` 等待结果；普通操作最多 10 秒，枚举 20 秒，诊断 30 秒，避免 UI 卡死。
- 显示器 ID 由厂商、型号、序列号 / EDID 序列（必要时才用后端路径）经 FNV-1a 生成，
  与枚举顺序无关，重启后保持稳定。
- 输入源、VCP 值、能力范围和写入方式全部来自配置档；服务层只执行选择器，不判断机型。
- 传输后端可插拔：`DdcTransport` 提供标准 DDC/CI 读写，`RawI2cDevice` 提供指定源地址的原始
  I2C 读写。Windows 上按显示器输出映射 NVAPI，标准写入被忽略时自动降级；其他平台返回明确的
  不可用原因。
- 读取快照时统一探测当前输入、亮度、音量和静音；结果缓存，只有操作、刷新或配置重载才失效。
- 每个输入源在运行时记录支持状态与传输方式；标准与原始 I2C 都无法生效时，端口会在 UI、
  CLI 与诊断中标记为“不支持”并保留失败原因。
- 日志按 2MB 轮转，保留 3 个备份（`.1`–`.3`）；不记录网络信息，因为程序不联网。

架构取舍、错误模型、平台差异和测试策略见 **[docs/DESIGN.md](docs/DESIGN.md)**。

## 项目结构

```
DisplayPilot/
├── index.html               # 控制面板
├── settings.html            # 设置窗口
├── src/                     # 前端逻辑、样式、mock 与类型
├── src-tauri/
│   ├── Info.plist            # macOS LSUIElement 与 HiDPI 元数据
│   ├── icons/                # 应用图标与菜单栏模板图标
│   ├── profiles/
│   │   ├── builtin.toml     # 编译进程序的内置档案
│   │   └── config.toml      # 首次启动写入用户的默认配置
│   └── src/
│       ├── app.rs           # Tauri 初始化、托盘、菜单与窗口事件
│       ├── app_window.rs    # 面板锚定、显示 / 隐藏、打开目录
│       ├── worker.rs        # 专用 DDC 工作线程与请求超时
│       ├── service.rs       # 业务逻辑、快照、诊断
│       ├── transport.rs     # DDC/CI 传输抽象与 ddc-hi 实现
│       ├── raw_i2c.rs       # 原始 I2C 后端（Windows NVAPI）与 DDC/CI 封包
│       ├── profile.rs       # 档案模型、匹配与合并
│       ├── config.rs        # 路径、TOML 读取与安全更新
│       ├── commands.rs      # IPC 命令
│       ├── shortcuts.rs     # 全局快捷键注册
│       └── cli.rs           # 兜底命令行
├── scripts/                 # 版本同步与检查
├── docs/                    # 设计、配置、版本与测试文档
└── CHANGELOG.md
```

## 开发与检查

```bash
npm run build          # tsc 类型检查 + Vite 构建两个页面
npm run test:rust      # 使用 fake transport 的服务层单元测试
npm run check:rust     # cargo fmt --check + clippy -D warnings
npm run check:version  # package.json / tauri.conf.json / Cargo.toml / CHANGELOG 一致
npm run check          # 以上全部
```

单元测试覆盖档案匹配、输入切换、失败后的缓存失效、亮度换算、音频能力隐藏、静音反转、
通用回退、快捷键步进、诊断输出与“无显示器”错误。真实 DDC/CI 行为与界面交互仍需在目标
机器上按 **[docs/TESTING.md](docs/TESTING.md)** 的手工清单验收。

## 版本管理

- 当前版本 **0.3.0**，遵循 SemVer；`0.x` 阶段允许在次版本中加入不兼容变更。
- `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`（以及 `Cargo.lock`）由脚本同步。
- 发版步骤：`npm run version:set -- 0.3.0` → 在 `CHANGELOG.md` 增加对应小节 → `npm run check`。
- 每次发布打 `v<version>` 标签。详细规则见 **[docs/VERSIONING.md](docs/VERSIONING.md)**。

## 隐私

DisplayPilot 不包含遥测、不发起网络请求，也不需要账号。所有配置与日志只写入本机目录；
诊断报告只在用户点击「复制诊断」时进入剪贴板。

## 排查

| 现象 | 处理 |
| --- | --- |
| 未发现显示器 | 唤醒显示器；在 OSD 中启用 DDC/CI；改用直连线缆；点击「重新扫描」。 |
| 输入源切换失败 | 用「复制诊断」确认 VCP `0x60` 与能力字符串；在 `profile_overrides` 中修正输入值或改用厂商选择器。 |
| macOS 上配置档显示为通用 / 亮度百分比偏低 | Apple Silicon 读不到 EDID，程序会用数值厂商与型号匹配；内置档案未覆盖的机型请在 `custom_profiles` 中用 `vendor_id = "1E6D"` 这类数值写法添加。 |
| 切到 USB-C 提示“端口不支持” | 该固件（如 27UL850）的 USB-C 只接受厂商专用寻址，标准 DDC/CI 无法发送，且当前 GPU / 平台没有可用的原始 I2C 后端。使用 NVIDIA 显卡（应用会自动通过 NVAPI 回退），或把目标主机改接 HDMI / DP，也可以直接在显示器 OSD 中切换；UI 与诊断会列出具体原因。 |
| 亮度可调但音量不可用 | 显示器可能不支持 `0x62`；`auto` 探测会禁用控件，属预期行为。可在配置中显式设为 `true` 强制显示。 |
| 快捷键无效 | 查看设置页的注册状态；换用未被系统占用的组合键后重新加载配置。 |
| 配置修改后没有生效 | 设置页点击「重新加载配置」；若文件解析失败，页面会显示具体错误。 |
| 托盘图标不可见 | 检查 Windows 通知区域溢出面板；macOS 检查菜单栏空间与登录项设置。 |
| 日志在哪里 | 设置页「文件与日志」→「打开日志目录」，或运行 `displaypilot config`。 |

## 文档

- [配置参考](docs/CONFIGURATION.md)
- [设计说明](docs/DESIGN.md)
- [版本管理](docs/VERSIONING.md)
- [测试与验收](docs/TESTING.md)
- [更新日志](CHANGELOG.md)

## 许可

MIT。详见 `Cargo.toml` 与仓库许可声明。

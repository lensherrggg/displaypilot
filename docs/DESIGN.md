# DisplayPilot 设计说明

本文描述 0.3.0 的实现结构、关键取舍与可扩展点。需求中的时序、验收条件与视觉约束在实现时统一下沉到
“服务层串行执行 + 配置驱动 + 非模态错误”三条主线，避免把机型或平台差异散落在 UI 与硬件代码里。

## 1. 目标与非目标

### 目标

- 一张紧凑面板完成输入源、亮度、音量、静音四类操作；键盘与 VoiceOver 可完整操作。
- 显示器差异由 TOML 档案描述；新增机型不需要改 Rust 或前端代码。
- 所有硬件访问有明确超时与可操作错误；失败不崩溃、不阻塞 UI。
- 同一份代码在 Windows 与 macOS 上构建；平台增强（vibrancy）不可用时优雅降级。
- 保留 CLI 兜底路径，便于自动化、远程排障和托盘不可用的场景。

### 非目标

- 不做 KVM、不做 EDID 注入、不做显示器固件升级。
- 不实现网络同步、账号或遥测。
- 不追求 Linux 桌面集成；DDC 后端即使可编译，也不在本版本验收范围。
- 不内置特定机型的业务分支。LG 27UL850 只是 `builtin.toml` 中的一条数据。

## 2. 分层结构

```
前端（WebView）
  index.html / main.ts / panel.css     控制面板
  settings.html / settings.ts / *.css  设置窗口
  api.ts / mock.ts / types.ts          IPC 封装、浏览器模拟、类型
        │ invoke / listen
Tauri 适配层
  commands.rs   参数校验、spawn_blocking、事件发送
  app.rs        托盘、窗口、生命周期、平台策略
  shortcuts.rs  全局快捷键注册与动作分发
  worker.rs     专用线程、请求队列、分级响应超时
        │ Command / Response
领域层
  service.rs    DisplayService：发现、快照、切换、读写、诊断
  profile.rs    ProfileCatalog：匹配、优先级、覆盖合并
  models.rs     IPC 序列化模型（camelCase）
  error.rs      AppError：code / message / hint
基础设施
  transport.rs  DdcTransport trait + HardwareTransport(ddc-hi)
  raw_i2c.rs    RawI2cDevice trait + Windows NVAPI 实现、DDC/CI 封包
  config.rs     AppPaths、TOML 读取、默认配置、[general] 安全更新
  logging.rs    滚动文件日志与 panic 钩子
  util.rs       稳定 ID、通配、百分比换算、平台信息
```

依赖方向始终由外向内：`commands` 只依赖 `WorkerClient`，`service` 只依赖 `DdcTransport`，
`profile` 与 `config` 不感知 UI。这样服务层可以用 fake transport 做完整单元测试。

## 3. 线程与超时

DDC 句柄在部分后端不是 `Send`，不能在 Tauri 的异步运行时里随意移动。实现选择：

1. `WorkerClient::spawn` 创建一个命名线程 `displaypilot-ddc`，线程独占地构造并持有
   `DisplayService<HardwareTransport>`。
2. 每个请求是一个 `Job { command, reply }`，经 `mpsc` 串行进入该线程。硬件读写天然串行，
   避免同时向 I2C/WinAPI 发送多条指令导致总线拥塞。
3. 调用方使用 `recv_timeout` 等待结果，预算按命令分级：普通读写 10 秒、枚举 20 秒、诊断 30 秒。
   冷启动枚举需要读取两次能力字符串（部分显示器单次约 1.5 秒），固定的短超时会把正常设备误判为超时。
   超时返回 `AppError::timeout`，提示检查连接与唤醒状态；输入切换需要额外的面板稳定等待与回读确认。
4. Tauri 命令用 `spawn_blocking` 调用 `WorkerClient`，因此 UI 线程和异步调度器不会被阻塞。

需要明确的语义：超时只放弃**调用方**的等待，操作系统层面的 DDC 调用可能仍在工作线程中完成。
后续请求会排在其后，因此超时后 UI 应提示用户稍后刷新，而不是连续重试。缓存也会在写失败时失效，
下一次快照会重新探测。

## 4. 启动流程

1. `main` 初始化日志；`cli::should_run` 判断是否执行 CLI。
2. GUI 路径下 Windows 调用 `FreeConsole` 脱离控制台，避免出现黑框；CLI 路径保留标准输出。
3. `app::run` 创建 `WorkerClient` 与 `AppState`，注册 IPC 命令、托盘和窗口事件。
4. macOS 设置 `Accessory` 激活策略并隐藏 Dock 图标，安装原生应用菜单（关于、设置 `⌘,`、编辑、窗口）；主窗口应用 vibrancy（失败仅记录日志）。
5. 创建菜单栏状态项（模板图标）、加载配置与快捷键，然后显示主面板。
6. 主窗口失焦时按 `general.auto_hide` 隐藏；关闭请求被拦截为隐藏；设置窗口独立显示。
7. macOS 再次打开已运行的应用（`RunEvent::Reopen`）会重新显示面板。

`AppState` 只保存 `WorkerClient`、路径、自动隐藏原子标志和快捷键状态，**不保存 DDC 句柄**。
这是线程模型的硬约束，也是所有 IPC 都必须走 worker 的原因。

## 5. 显示器发现与稳定身份

`DisplayService::discover` 调用 `HardwareTransport::enumerate`，后者使用 `ddc_hi::Display::enumerate()`
并尝试 `update_capabilities()`。能力字符串读取失败不会剔除显示器，而是记录 `capabilities_error`，
留给诊断报告展示。

稳定 ID 由 `util::stable_display_id` 生成：

```
dsp-<FNV-1a 64 hex>
seed = vendor | model_id | model_name | sn:<serial>       （优先）
     = vendor | model_id | model_name | edid:<serial>     （其次）
     = vendor | model_id | model_name | backend | native_id（最后手段）
```

- 枚举顺序、操作系统设备路径不参与前两种种子，因此重启、换 USB 口不会改变 ID。
- 同一型号同序列号会得到同一 ID；没有序列号的设备只能退化为路径标识，这是 EDID 信息不足的固有限制。
- Apple Silicon 上 CoreDisplay 不提供 EDID，`ddc-hi` 的厂商 / 型号字段为空。`transport.rs` 会从
  `ddc_macos::Monitor` 补齐：`CGDisplay::vendor_number` / `model_number` / `serial_number`、
  `product_name()` 与可选的 `edid()`。厂商因此是数值码（LG 为 `1E6D`）而不是 Windows 的三字母码（`GSM`）。
- 发现后按显示名称、ID 排序，首台自动选中；上次使用的显示器 ID 若仍在线则优先保留。

## 6. 快照与能力探测

`DisplayService` 对每台显示器缓存 `DisplaySnapshot`。缓存失效条件：输入切换、亮度/音量/静音写入失败、
手动刷新、配置重载。正常读取直接返回缓存，保证空闲时不产生 DDC 流量。输入切换写入成功后先等待
1.5 s 让面板完成切换，再只回读一次当前输入 VCP；等待期间不产生任何 DDC 流量，因为切换过程中读
`0x60` 会让 LG 类固件的 DDC 处理卡死，之后连写入都被忽略。回读成功但不是目标值时，使缓存失效并
返回 `switch_unconfirmed`；回读失败说明本机已与显示器断开，即面板完成了切换，按成功处理，链路状态
只写日志。回读失败后不再重试，避免在切换或回切过程中继续打总线。

首次快照依次读取：

| 项 | VCP | 行为 |
| --- | --- | --- |
| 当前输入 | `profile.current_input_vcp_code`（通常 `0x60`） | 读到的值映射到配置中的输入 id；未定义时显示原始值并给出说明。 |
| 亮度 | `capabilities.brightness.vcp_code` | `supported=true` 时始终显示；读取失败仍允许写入。 |
| 音量 | `capabilities.volume.vcp_code` | `supported="auto"` 时只有读取成功才启用控件。 |
| 静音 | `capabilities.mute.vcp_code` | 与音量相同的 auto 策略，并按 `on_value/off_value` 解释。 |

`supported` 的三态语义：

- `true`：强制显示并允许写入；
- `false`：不显示 / 禁用，并给出配置原因；
- `"auto"`：以实际读取结果为准。读取失败视为不支持，避免向不存在的 VCP 反复写。

百分比换算使用四舍五入：

```
raw = min + round((max - min) * percent / 100)
percent = round((raw - min) * 100 / (max - min))
```

UI 只使用 0–100 的整数；百分比换算优先采用 VCP 回复中的 `maximum`（常见 100 或 255），
配置里的 `max` 仅作回退。步进来自 `[general]`，
快捷键与箭头键使用同一步进，相对调整在边界处饱和。

## 7. 输入源选择器

`InputSource.selector` 是带标签的枚举，服务层只负责执行，不判断厂商：

```toml
# 标准 VCP 写入（大多数显示器）
[profiles.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0x0F

# 厂商 VCP 写入
[profiles.inputs.selector]
type = "vendor"
vcp_code = 0xE0
value = 0x0001
write_mode = "vcp"

# 厂商 Table 写入：offset 处写入两字节大端 value
[profiles.inputs.selector]
type = "vendor"
vcp_code = 0xE0
value = 0x0001
data_address = 0x0010
write_mode = "table"

# 标准写入 + 原始 I2C 回退（LG 厂商专用寻址）
[profiles.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0x10
[profiles.inputs.selector.raw_fallback]
vcp_code = 0xF4
value = 0xD1
source_address = 0x50

# 原始 I2C 直写
[profiles.inputs.selector]
type = "raw"
vcp_code = 0xF4
value = 0xD1
source_address = 0x50
verify_value = 0x10
```

`type = "vendor"` 只是语义标记，实际路径由 `write_mode` 决定：`vcp` 调 `set_vcp_feature`，
`table` 调 `table_write` 并在 `data_address` 写入 `value.to_be_bytes()`。`WriteMode` 默认是 `table`，
与部分厂商（如 LG）通过表写入切换输入的实现一致。

`raw_fallback` 与 `type = "raw"` 走 `RawI2cDevice`：后者由 GPU 驱动直接向指定 I2C 从地址
发送 DDC/CI 数据包，因此能使用标准 API 无法指定的源地址（`0x50`）。Windows 上通过 NVAPI
枚举 GPU 输出并映射到 `ddc-hi` 枚举出的显示器；映射不确定时该显示器返回不可用原因，而不是
猜测。macOS / Linux 当前没有实现，统一返回明确的不可用原因。

切换流程：

1. 标准选择器（`standard` / `vendor`）先写入；
2. 等待 120ms 后回读 `current_input_vcp_code`，匹配则成功；
3. 写入失败、被忽略或回读不匹配时，若有 `raw_fallback` 且后端可用，则用原始 I2C 重写；
4. 等待 220ms 后再次回读；匹配则成功；回读本身失败视为切换成功（切换到另一台主机后
   DDC 通常会消失）；
5. 两种传输都失败，或最终仍停留在原输入时，该输入被标记为运行时“不支持”，返回
   `input_unsupported`，并在列表 / 诊断中附带原因。成功切换会清除该标记。

回退是自动的，但不会掩盖结果：每次成功都会在 `current_input.transport` 记录实际使用的
传输方式（`standard` 或 `raw:<backend>`），诊断报告同时给出每个输入的支持状态与原因。

## 8. 档案匹配与配置合并

`ProfileCatalog::load` 分四步：

1. 解析编译进二进制的 `builtin.toml`；
2. 对 `[profile_overrides.<id>]` 做 TOML 深合并：表递归合并，数组整体替换；
3. 追加 `[[custom_profiles]]`，检查 `schema_version`；
4. 按 `priority` 降序、`id` 升序排序，校验 id/label/范围/步进/输入唯一性。

匹配顺序：先找第一条**匹配规则非空且命中**的档案，再回退到 `generic-ddc`，最后回退到第一条。
匹配字段：

- `vendor_id`：忽略大小写；
- `model_id`：精确；
- `model_name` / `model_name_any`：大小写不敏感通配（`*`、`?`）或子串；
- `serial_pattern`：通配；
- `backend`：忽略大小写（如 `winapi`、`macos`）。

覆盖只允许命中内置 id；未知 id 会产生警告而不是错误，避免用户复制粘贴后整个配置失效。
`profile_overrides` 中数组（如 `inputs`）会整体替换，必须写完整列表；嵌套表可以只写差异字段。

配置加载采用“失败降级”：文件不存在时写入默认模板；解析或校验失败时使用 `UserConfig::default()`，
同时把错误保留给 UI。内置档案仍可正常使用，用户可以在设置页看到具体原因并修复。

## 9. 错误模型

`AppError` 固定包含 `code`、`message`、`hint`，序列化为 camelCase。`hint` 是可操作建议，
UI 以内联错误条显示，不使用模态弹窗。主要错误码：

| code | 触发 | 典型提示 |
| --- | --- | --- |
| `no_displays` | 枚举为空 | 开启 DDC/CI、唤醒显示器、避免不支持 DDC 的扩展坞。 |
| `display_not_found` | 目标 ID 不在线 | 刷新后重试。 |
| `timeout` | worker 超时（普通 10s / 枚举 20s / 诊断 30s） | 检查连接、唤醒显示器，稍后刷新。 |
| `permission` | 系统拒绝访问 | 检查隐私/安全设置。 |
| `unsupported` | 显示器不支持该 VCP 或配置禁用 | 调整配置或运行诊断。 |
| `ddc_error` | 校验、I2C、协议错误 | 检查线缆 / KVM。 |
| `switch_unconfirmed` | 写入成功但回读仍非目标值（显示器明确未切换或已自动回切） | 目标接口无信号、显示器自动回切、OSD 菜单未退出或固件忽略写入；改用 OSD 或检查信号源。 |
| `invalid_input` | 输入 id 不在档案中 | 检查 `inputs`。 |
| `input_unsupported` | 标准与原始 I2C 都无法切换该端口，或后端不可用 | 查看端口原因；换用 NVIDIA 原始 I2C、其他接口或 OSD。 |
| `invalid_config` | 配置文件无效 | 修正后重新加载。 |
| `internal` | worker 退出等 | 重启应用并查看日志。 |

`TransportError` 先按消息特征归类（权限、超时、不支持、断开、通信、其他），再映射为面向用户的
`AppError`。这种启发式分类不完美，但能把底层 `ddc-hi` 的异构错误统一成稳定 UI 语义；原始信息保留
在日志和诊断报告中。

## 10. UI 与 HIG

### 面板

- 360×620 无边框、透明、置顶、跳过任务栏；由 `app_window::position_panel` 定位。macOS 居中于状态项下方并夹紧到工作区，Windows 定位到工作区右下角。
- 结构：标题（显示器名 + 档案/后端）→ 显示器切换 → 输入源卡片网格 → 亮度卡 → 音频卡 → 状态行。
- 控件使用原生 `<button>`、`<input type="range">`、`<output>`；滑块有 `aria-valuetext` 百分比。
- 输入切换时按钮 `aria-busy`、显示“切换中…”，并禁用重复提交；成功用对勾与 `aria-label` 标识当前输入。
- 亮度/音量拖动先更新数值显示，停止 **130ms** 后写入；键盘操作同样走防抖，但步进即时反馈。
- 错误条 `role="alert"` 内联展示，可关闭；配置错误额外提供“打开配置”。
- `Esc` 关闭面板；失焦按配置自动隐藏；`settings-shown` 等事件由后端推送。
- 点击状态项会先让面板失焦（触发自动隐藏），再投递点击事件。后端记录自动隐藏时间，220ms 内的状态项点击只关闭、不再重开，避免闪烁。
- macOS 面板移除 `data-tauri-drag-region`：无边框窗口被拖离状态项后，下次点击图标会跳回，破坏 popover 预期。

### 设置窗口

- 独立系统装饰窗口（可调整尺寸），宽度上限 640px 的滚动内容区。
- 分区：通用、快捷键、显示器、配置档、文件与日志、关于。
- 步进输入做 1–50 严格校验，非法时设置 `aria-invalid` 并阻止保存；保存后显示“已保存”状态。
- 快捷键列表展示加速键、注册状态（已注册 / 注册失败 / 未注册 / 未设置 / 预览）与错误原因。
- 配置横幅显示解析错误或提示；脏表单在窗口重新显示时不会被自动覆盖。
- 所有用户可控文本通过 `textContent` 写入，避免 HTML 注入。

### 视觉与可访问性

- 语义颜色来自 `tokens.css`，同时定义浅色、深色与 `prefers-contrast: more`；
- 间距以 8pt 为基准，卡片圆角与系统控件观感一致；
- 过渡 150–200ms，`prefers-reduced-motion: reduce` 时移除位移与淡入；
- 提供 `:focus-visible` 焦点环、键盘顺序、`aria-live` 状态与 toast；
- 浏览器 `mock.ts` 让界面可在无 Tauri、无硬件时完整走查（保存、切换、错误、诊断）。

## 11. 跨平台差异

| 关注点 | Windows | macOS |
| --- | --- | --- |
| DDC 后端 | `ddc-winapi` | `ddc-macos`（Apple Silicon 走 `IOAVService`，Intel 走 IOKit） |
| 原始 I2C | NVIDIA NVAPI（已实现并验证读取） | 未实现（IOAVService），返回不可用原因 |
| 显示器身份 | EDID：三字母厂商码、型号、序列号 | CoreGraphics 数值厂商 / 型号 + 产品名；EDID 通常不可用 |
| 窗口材质 | CSS 半透明 + 背景模糊降级 | `window-vibrancy`（HudWindow），失败静默降级；面板铺满窗口、圆角由原生材质裁切，关闭原生阴影避免方形外框 |
| 应用形态 | 托盘 + 跳过任务栏；GUI 启动时 `FreeConsole` | `LSUIElement` / `Accessory`，无 Dock 图标，原生应用菜单 |
| 面板定位 | 工作区右下角 | 状态项下方，夹紧到工作区；首次启动显示在右上角 |
| 状态栏图标 | 应用图标 | 单色模板图标，随浅色 / 深色菜单栏着色 |
| 日志 | `%LOCALAPPDATA%\DisplayPilot\logs` | `~/Library/Logs/DisplayPilot` |
| 权限 | 通常无需管理员 | DDC/CI 通常无需额外权限；模拟点击 / 辅助功能仅在自动化测试时需要 |

CLI 与 GUI 共用同一套服务层，因此 Windows 上即使前端异常，仍可用 `displaypilot switch ...` 操作。
Windows 没有菜单栏状态项，面板同样以“失焦隐藏”模拟 popover 语义并停在工作区右下角；
macOS 则用状态项矩形作为锚点，并在 `LSUIElement` 下保持无 Dock 图标。
macOS 设置窗口使用原生标题栏与红绿灯按钮，页面内不再重复渲染关闭按钮。

## 12. 日志与诊断

- 文件 logger 带时间戳、级别、target；debug 构建记录 Debug，release 记录 Info。
- 单文件 2MB，保留 3 个备份（`.1`–`.3`），启动时轮转；panic 钩子先写日志再调用原钩子。
- 关键路径日志：枚举数量与设备字段、档案匹配结果、输入切换的 VCP/offset/value、失败原因、
  快捷键注册、配置重载。
- 诊断报告包含：应用/OS 版本、生成时间、显示器身份、档案与是否精确匹配、当前输入及其传输方式、
  亮度/音量/静音、原始 I2C 可用性与后端、每个输入的支持状态 / 传输方式 / 原因、能力读取错误、
  能力字符串、`0x60/0x10/0x62/0x8D/0xDF` 的 VCP 原始值、输入选择器描述和操作提示。
- UI 的“复制诊断”在失败时会把错误信息也写入剪贴板，便于反馈问题。

## 13. CLI

`cli::should_run` 的规则：无参数或首个参数为 `--gui` 时启动 GUI；其他情况执行 CLI。
选项解析支持选项出现在子命令前后，`--display value` 与 `--display=value` 等价。
`switch` 需要输入 id；`brightness/volume` 无参数时读取，`+N/-N` 相对调整；`mute` 默认 `toggle`。
CLI 直接构造 `DisplayService`，不经过 Tauri，因此可在无桌面会话的终端中运行（仍需可访问显示设备）。

## 14. 测试策略

- **领域单元测试**：`FakeTransport` 维护 VCP 状态表与原始 I2C 写入记录，覆盖档案匹配、输入切换、
  标准写入被忽略后的原始 I2C 回退、回退不可用 / 双传输失败后的端口不支持标记、失败后缓存失效、
  亮度缩放、音频 auto 隐藏、静音反转、通用回退、步进、诊断和空枚举。
- **配置测试**：默认配置生成、非法 schema 降级、空快捷键、snake_case TOML ↔ camelCase IPC 往返。
- **档案测试**：LG 匹配、覆盖合并、数组替换、优先级、未知 override 警告、校验失败。
- **前端**：`mock.ts` 在浏览器中模拟全部 IPC；用 `tsc --noEmit` 与 Vite 构建保证类型与产物。
- **手工验收**：托盘、窗口定位、失焦隐藏、深色/高对比度、快捷键、真实 DDC 切换与亮度/音量，
  详见 [TESTING.md](TESTING.md)。

不引入真实硬件自动化测试：DDC/CI 依赖具体显示器与线缆，CI 环境无法稳定复现；fake transport
保证协议映射和业务规则可回归，硬件相关假设由诊断报告与手工清单兜底。

## 15. 取舍与限制

1. **单工作线程**：简单、无数据竞争，但一条慢指令会阻塞后续请求。分级超时（普通 10 秒、枚举 20 秒、
   诊断 30 秒）在保证可感知反馈的同时，不会把正常的慢速 DDC 设备误判为失败；
   若未来需要并行，应按显示器分片而不是共享句柄。
2. **切换后回读，但回读失败视为成功**：写入后静默等待 1.5 s 让面板完成切换，再回读一次 VCP；
   显示器接受写入时用回读确认，切换到另一台主机后 DDC 通常会断开，此时把通信失败视为成功，
   链路状态只做日志。代价是每次切换多花约 1.5 秒，但不会在切换过程中打总线，也不会再把成功的切换
   误报为失败。标准写入被忽略时自动回退原始 I2C；最终仍停留在原输入时明确标记端口不支持。
3. **auto 能力探测**：能自动隐藏不存在的音频控件，但首次打开面板需要读多个 VCP，可能比纯静态配置慢；
   缓存与串行执行把额外开销限制在首屏。
4. **稳定 ID 依赖 EDID**：无序列号的设备在换端口后可能改变 ID；这是数据源限制，不通过持久化映射掩盖。
5. **启发式错误分类**：不同后端错误文本不一，分类可能落到 `internal`；诊断报告保留原始消息。
6. **macOS 身份来源不同**：Apple Silicon 读不到 EDID，厂商以数值码暴露，内置档案分别提供 Windows 与 macOS 两套匹配。数值码稳定但与其他工具的三字母码不一致，用户覆盖配置时需要注意。
7. **只在单台 macOS 硬件上验证**：菜单栏、vibrancy、DDC 读写与 0–100 范围已在 Apple Silicon + LG 27UL850 上验收；其他显示器、Intel Mac 与外接扩展坞仍按测试清单验收。
8. **原始 I2C 覆盖有限**：目前只有 Windows + NVIDIA（NVAPI）实现。AMD / Intel 没有公开的
   等价接口，Apple Silicon 需要 IOAVService，Linux 需要 i2c-dev；这些平台统一返回不可用原因，
   UI 把相关端口标记为不支持，而不是静默失败。
9. **未包含登录项与公证**：当前构建为 ad-hoc 签名；开机自启需要用户手动加入登录项，对外分发应使用 Developer ID 签名并公证。

## 16. 可扩展点

- 新增内置机型：只改 `src-tauri/profiles/builtin.toml`，跑档案测试并更新 `CHANGELOG`。
- 新增能力（对比度、色温、电源模式）：在 `Capabilities` 增加 VCP 配置、服务增加读写方法、UI 增加卡片；
  仍保持“配置提供范围、服务执行选择器”的结构。
- 新增平台：实现 `DdcTransport`，在 `Cargo.toml` 启用对应后端，补充路径与窗口策略；
  服务层与前端无需改动。
- 自动化：CLI `--json` 已提供稳定输出，可作为脚本或外部快捷方式的接口。

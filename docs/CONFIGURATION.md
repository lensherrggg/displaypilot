# 配置参考

DisplayPilot 的显示器差异、能力范围、输入源映射、快捷键与界面偏好全部由 TOML 描述。
配置文件在首次启动时自动生成，之后不会被升级覆盖；内置档案始终编译在程序里。

## 1. 文件位置

| 平台 | 配置 | 日志 |
| --- | --- | --- |
| Windows | `%APPDATA%\DisplayPilot\config.toml` | `%LOCALAPPDATA%\DisplayPilot\logs\displaypilot.log` |
| macOS | `~/Library/Application Support/DisplayPilot/config.toml` | `~/Library/Logs/DisplayPilot/displaypilot.log` |

准确路径可在设置页「文件与日志」或 `displaypilot config` 中查看。配置文件不存在时会写入
`src-tauri/profiles/config.toml` 的模板内容；只有 `[general]` 会由设置页更新，且保留注释。

`schema_version` 当前必须为 `1`。用户配置解析或校验失败时，程序回退到内置默认值，
并在面板 / 设置页显示错误；内置档案仍然可用。

## 2. 顶层结构

```toml
schema_version = 1

[general]
brightness_step = 5
volume_step = 5
auto_hide = true
show_unsupported_audio = true

[shortcuts]
toggle_panel = ""
brightness_up = ""
brightness_down = ""
volume_up = ""
volume_down = ""
mute = ""

[[shortcuts.input_switch]]
input = "hdmi1"
accelerator = ""

[profile_overrides.lg-27ul850]
# 只写需要覆盖的字段

[[custom_profiles]]
# 一个完整的显示器档案
```

| 表 | 必填 | 作用 |
| --- | --- | --- |
| `[general]` | 否 | 步进、自动隐藏、音频占位显示。 |
| `[shortcuts]` | 否 | 全局快捷键；空字符串表示不注册。 |
| `[profile_overrides.<id>]` | 否 | 覆盖内置档案的任意字段。 |
| `[[custom_profiles]]` | 否 | 新增完整显示器档案。 |

## 3. `[general]`

文件内使用 snake_case；前端 IPC 使用 camelCase，由序列化层自动转换。

| 键 | 类型 | 默认 | 范围 | 说明 |
| --- | --- | --- | --- | --- |
| `brightness_step` | 整数 | `5` | 1–50 | 快捷键、方向键与设置页的相对调整步进（百分点）。 |
| `volume_step` | 整数 | `5` | 1–50 | 音量步进（百分点）。 |
| `auto_hide` | 布尔 | `true` | — | 主面板失去焦点时自动隐藏。 |
| `show_unsupported_audio` | 布尔 | `true` | — | 不支持的音量 / 静音控件保持可见但禁用，并显示原因；`false` 时隐藏。 |

超出范围会让整份用户配置回退到默认值，并在 UI 中提示。

## 4. `[shortcuts]`

加速键使用 [global-hotkey](https://docs.rs/global-hotkey/) 语法，例如：

```toml
[shortcuts]
toggle_panel = "Control+Alt+KeyH"
brightness_up = "Control+Alt+ArrowUp"
brightness_down = "Control+Alt+ArrowDown"
volume_up = "Control+Alt+Equal"
volume_down = "Control+Alt+Minus"
mute = "Control+Alt+KeyM"
```

常用键名：`KeyA`–`KeyZ`、`Digit0`–`Digit9`、`F1`–`F12`、`ArrowUp`、`Equal`、`Minus`、`Space`。
修饰键：`Control` / `Ctrl`、`Alt` / `Option`、`Shift`、`Super` / `Meta`、`CommandOrControl`。
组合键区分大小写写法，建议始终使用库文档中的规范名称。

| 键 | 动作 |
| --- | --- |
| `toggle_panel` | 显示 / 隐藏控制面板。 |
| `brightness_up` / `brightness_down` | 对当前选中显示器增减亮度。 |
| `volume_up` / `volume_down` | 增减音量。 |
| `mute` | 静音 / 取消静音。 |

输入源快捷键是数组，`input` 必须匹配配置档中的输入 id：

```toml
[[shortcuts.input_switch]]
input = "hdmi1"
accelerator = "Control+Alt+Digit1"

[[shortcuts.input_switch]]
input = "dp"
accelerator = "Control+Alt+Digit3"
```

- 输入源快捷键作用于当前选中的显示器；如果该显示器的档案没有这个 id，操作会失败并给出提示。
- 同一组合键被占用时，设置页会显示“注册失败”和底层错误；其他快捷键仍会正常注册。
- 省略 `input_switch` 时默认生成 `hdmi1`、`hdmi2`、`dp`、`typec` 四项（加速键为空）；一旦提供该数组，
  就整体替换默认值。
- 修改后点击设置页「重新加载配置」生效，无需重启。

## 5. 显示器档案

一个档案描述“如何识别这台显示器”以及“如何操作它”。内置档案在
`src-tauri/profiles/builtin.toml`，用户可以用覆盖或自定义档案扩展。

### 5.1 完整字段

```toml
[[custom_profiles]]
id = "dell-u2723qe"                 # 必填，[A-Za-z0-9_-]，全局唯一
label = "Dell U2723QE"              # 必填，界面显示名
priority = 50                        # 越大越先参与匹配，默认 0
current_input_vcp_code = 0x60        # 读取当前输入的 VCP，默认 0x60

[custom_profiles.match]
vendor_id = "DEL"
model_name_any = ["*U2723QE*"]

[custom_profiles.capabilities]
input_switch = true

[custom_profiles.capabilities.brightness]
supported = true
vcp_code = 0x10
min = 0
max = 255
step = 5

[custom_profiles.capabilities.volume]
supported = "auto"
vcp_code = 0x62
min = 0
max = 255
step = 5

[custom_profiles.capabilities.mute]
supported = "auto"
vcp_code = 0x8D
on_value = 1
off_value = 2

[[custom_profiles.inputs]]
id = "hdmi1"
label = "HDMI1"
[custom_profiles.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0x11
```

内置档案文件使用 `[[profiles]]` 数组，因此其嵌套表写作 `[profiles.match]`、`[profiles.capabilities]`、
`[[profiles.inputs]]`；用户配置中的 `[[custom_profiles]]` 与 `[profile_overrides.<id>]` 按各自前缀展开。
三者字段结构完全相同。

### 5.2 `match`

| 键 | 类型 | 说明 |
| --- | --- | --- |
| `vendor_id` | 字符串 | EDID 厂商码，忽略大小写，例如 `GSM`、`DEL`、`ACR`。 |
| `model_id` | 整数 | EDID 型号码，精确匹配（十进制或 TOML 十六进制均可）。 |
| `model_name` | 字符串 | 型号名，支持 `*` / `?` 通配，也接受大小写不敏感子串。 |
| `model_name_any` | 字符串数组 | 任一模式命中即可；用于覆盖固件命名差异。 |
| `serial_pattern` | 字符串 | 序列号通配，适合只对某台设备启用特殊配置。 |
| `backend` | 字符串 | DDC 后端名，如 `winapi`、`macos`，忽略大小写。 |

- 所有填写字段是 **AND** 关系；`model_name_any` 内部是 **OR**。
- 空 `match` 表示“通用回退”，不参与主动匹配。
- 匹配按 `priority` 降序、`id` 升序进行，取第一条命中项。
- 若没有档案命中，优先使用 `generic-ddc`；再退回第一条空 `match` 的档案；最后退回排序后的第一条。
- 同型号同序列号的多台显示器会共用同一档案；需要区分时请使用 `serial_pattern` 建立更高优先级的档案。

### 5.3 `capabilities`

| 表 | 键 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- | --- |
| — | `input_switch` | 布尔 | `true` | 是否允许切换输入源。为 `false` 时界面隐藏 / 禁用切换。 |
| `brightness` | `supported` | 布尔或 `"auto"` | `true` | 亮度支持策略；默认强制显示。 |
| | `vcp_code` | 整数 | `0x10` | 亮度 VCP。 |
| | `min` / `max` | 整数 | `0` / `255` | 原始值范围，用于百分比换算。 |
| | `step` | 整数 | `5` | 档案能力元数据，必须 > 0；实际交互步进来自 `[general]`。 |
| `volume` | `supported` | 布尔或 `"auto"` | `"auto"` | 音量支持策略；默认读取成功才启用。 |
| | `vcp_code` | 整数 | `0x62` | 音量 VCP。 |
| | `min` / `max` | 整数 | `0` / `255` | 原始值范围。 |
| | `step` | 整数 | `5` | 同上。 |
| `mute` | `supported` | 布尔或 `"auto"` | `"auto"` | 静音支持策略。 |
| | `vcp_code` | 整数 | `0x8D` | 静音 VCP。 |
| | `on_value` | 整数 | `1` | 表示“已静音”的原始值。 |
| | `off_value` | 整数 | `2` | 表示“未静音”的原始值。 |

`supported` 接受：

- `true` / `false`（TOML 布尔）；
- `"auto"`；
- 兼容字符串 `"true"`、`"yes"`、`"false"`、`"no"`（忽略大小写）。

`auto` 的判定发生在读取快照时：读取成功即支持并显示当前值；读取失败则视为不支持。
亮度默认 `true`，因为部分显示器支持写入但读取不稳定，强制隐藏会削弱核心功能。
如果某台显示器明确不支持音量，建议显式写 `supported = false`，避免每次探测都产生失败日志。

百分比换算为四舍五入：`raw = min + round((max-min)*percent/100)`。若 `max <= min`，写入取 `min`。
`max` 小于 `min` 或 `step = 0` 会使配置校验失败。

### 5.4 `inputs` 与 `selector`

每个输入源需要唯一的 `id`、显示用 `label` 和 `selector`。界面按数组顺序渲染。

#### 标准 VCP

```toml
[[profiles.inputs]]
id = "dp"
label = "DP"
[profiles.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0x0F
```

直接向 `vcp_code` 写入 `value`。这是 DDC/CI 标准输入源切换方式。

标准写入成功但回读显示显示器忽略了该值时，可以配置原始 I2C 回退：

```toml
[profiles.inputs.selector.raw_fallback]
vcp_code = 0xF4
value = 0xD2
source_address = 0x50
```

回退由 GPU 直接向显示器 I2C 总线写入 DDC/CI 数据包，因此可以指定源地址（默认 `0x50`）。
目前只有 Windows + NVIDIA 显卡（NVAPI）提供该能力；其他平台会明确标记端口不可用，不会静默失败。
`raw_fallback` 同样适用于 `vendor` 选择器。

#### 原始 I2C 直写

```toml
[[profiles.inputs]]
id = "typec"
label = "TypeC"
[profiles.inputs.selector]
type = "raw"
vcp_code = 0xF4
value = 0xD2
source_address = 0x50
verify_value = 0x10
```

`type = "raw"` 跳过标准 DDC/CI，直接通过 GPU I2C 写入。`verify_value` 是切换后回读
`current_input_vcp_code` 时期望的值（省略时使用 `value`）。如果原始 I2C 不可用，该端口会在
列表、诊断和 UI 中显示为“不支持”并附带原因。

#### 厂商 VCP

```toml
[[profiles.inputs]]
id = "hdmi3"
label = "HDMI 3"
[profiles.inputs.selector]
type = "vendor"
vcp_code = 0xE0
value = 0x0003
write_mode = "vcp"
```

`type = "vendor"` 表示这是厂商私有语义；实际写入路径由 `write_mode` 决定。`write_mode = "vcp"` 仍是
普通的 VCP 写入。

#### 厂商 Table 写入

```toml
[[profiles.inputs]]
id = "dp"
label = "DP"
[profiles.inputs.selector]
type = "vendor"
vcp_code = 0xE0
value = 0x0001
data_address = 0x0010
write_mode = "table"
```

在 `vcp_code` 对应的表地址 `data_address` 写入 `value` 的两字节大端表示。`data_address` 默认 `0`，
`write_mode` 默认 `table`。不同厂商的 VCP/offset/编码差异很大，务必用诊断与实际切换验证。

字段约束：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `type` | `"standard"` / `"vendor"` / `"raw"` | 选择器种类；`vendor` 允许 `data_address` / `write_mode`，`raw` 要求可用的原始 I2C 后端。 |
| `vcp_code` | 0–255 | VCP 码。 |
| `value` | 0–65535 | 目标原始值。 |
| `data_address` | 0–65535 | 仅 `vendor`；表写入偏移。 |
| `write_mode` | `"table"` / `"vcp"` | 仅 `vendor`；默认 `table`。 |
| `source_address` | 0–255 | 仅 `raw`；DDC/CI 源地址，默认 `0x50`。 |
| `verify_value` | 0–65535 | 仅 `raw`；回读当前输入时期望的值，默认等于 `value`。 |

同一档案内重复的输入 `id` 会导致校验失败。读取当前输入时，服务按 `verify_value`（`raw` 未设置时
为 `value`）反查第一个匹配的输入源；如果两个输入使用相同值，只有第一个会被识别。

`raw_fallback` 子表可出现在 `standard` / `vendor` 选择器中，字段与 `raw` 的
`vcp_code` / `value` / `source_address` 相同。

## 6. 覆盖内置档案

`[profile_overrides.<内置 id>]` 只写差异字段。合并规则：

- 表递归合并；
- 数组**整体替换**；
- 标量覆盖；
- 未知 id 会被忽略并给出警告，不会让整份配置失效。

例如，给 LG 27UL850 换一个本地化名称，并只改亮度上限：

```toml
[profile_overrides.lg-27ul850]
label = "LG 27UL850（书房）"

[profile_overrides.lg-27ul850.capabilities.brightness]
max = 100
```

如果该固件使用另一组输入码，再整体覆盖 `inputs`（必须写全四项）：

```toml
[[profile_overrides.lg-27ul850.inputs]]
id = "hdmi1"
label = "HDMI1"
[profile_overrides.lg-27ul850.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0x90

[[profile_overrides.lg-27ul850.inputs]]
id = "hdmi2"
label = "HDMI2"
[profile_overrides.lg-27ul850.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0x91

[[profile_overrides.lg-27ul850.inputs]]
id = "dp"
label = "DP"
[profile_overrides.lg-27ul850.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0xD0

[[profile_overrides.lg-27ul850.inputs]]
id = "typec"
label = "TypeC"
[profile_overrides.lg-27ul850.inputs.selector]
type = "standard"
vcp_code = 0x60
value = 0xD1
```

因为 `inputs` 是数组，必须提供完整四项；只写 Type-C 会让其余输入消失。

## 7. 内置档案

| id | 优先级 | 匹配 | 亮度 | 音量 / 静音 | 输入值 |
| --- | --- | --- | --- | --- | --- |
| `lg-27ul850` | 100 | 型号名含 `27UL850` 或 `UL850`（Windows 会用能力字符串回退） | `0x10`，0–255，`true` | `0x62` / `0x8D`，`auto` | HDMI1 `0x11`、HDMI2 `0x12`、DP `0x0F`、TypeC `0x10`；均带 `0xF4` 原始 I2C 回退 |
| `generic-ddc` | -100 | 空（通用回退） | `0x10`，0–255，`true` | `0x62` / `0x8D`，`auto` | HDMI1 `0x11`、HDMI2 `0x12`、DP `0x0F`、TypeC `0x1B`（标准值） |

27UL850 的能力字符串会报告 `VCP 0x60(11 12 0F 10)`，因此内置档案使用这组标准值；USB-C 由
第二个 DisplayPort 输入 `0x10` 表示。部分 LG 固件改用私有值 `0x90/0x91/0xD0/0xD1`（USB-C 可能
是 `0xD2`），此时用上面的 `inputs` 覆盖即可。程序本身没有任何 `if LG` 之类的分支。

> **LG 厂商专用寻址与原始 I2C**：实测 27UL850 的 USB-C 切换只接受厂商专用寻址
> （VCP `0xF4`，DDC 源地址 `0x50`），Windows 标准 `SetVCPFeature` 固定使用 `0x51`，无法发送这类
> 命令。内置档案为四个输入都配置了 `raw_fallback`：标准写入被忽略时自动改用 NVAPI 原始 I2C
> 重发 `0xF4`（HDMI1 `0x90`、HDMI2 `0x91`、DP `0xD0`、TypeC `0xD2`）。
>
> 该回退依赖 NVIDIA 显卡驱动。AMD / Intel 核显或独显、macOS、Linux 目前没有可用的原始 I2C
> 后端，应用会把对应端口标记为“不支持”并显示原因，而不是假装成功。实测 NVAPI 可正常读取
> `0xF4 @0x50`；切换到另一台主机后本机 DDC 会消失，因此应用把切换后的回读失败视为成功。

## 8. 验证与排错

1. 运行 `displaypilot list`，确认显示器被发现、档案 id 与输入源符合预期。
2. 运行 `displaypilot diagnose`，查看：
   - `profile_matched` 是否为 `true`（否则说明落到了通用档案）；
   - 当前输入 VCP 原始值及其传输方式（`standard` / `raw:<后端>`）；
   - 每个输入源是否支持切换、使用哪种传输方式、失败原因与建议；
   - 原始 I2C 后端是否可用及其原因；
   - 能力字符串是否包含所需的 `VCP` 码；
   - 音量 / 静音读取失败的具体错误。
3. 修改配置后，在设置页点击「重新加载配置」，或重启应用。
4. 若配置解析失败，UI 会显示行号相关的 TOML 错误；修正后再次加载，无需删除文件。
5. 想回到默认值：退出应用，备份并删除/重命名 `config.toml`，下次启动会重新生成模板。

常见问题：

| 现象 | 可能原因 | 处理 |
| --- | --- | --- |
| `profile_matched = false` | 厂商码 / 型号名不匹配 | 用诊断中的 `vendor_id`、`model_name` 修正 `match`，或新建 `custom_profiles`。 |
| 输入切换失败但诊断有 `0x60` | 值不对或需要厂商写入 | 修正 `value`；尝试 `type = "vendor"` 与 `write_mode`。 |
| 端口不支持（`input_unsupported`） | 标准 DDC 与原始 I2C 都未生效，或当前 GPU / 平台没有原始 I2C 后端 | 确认目标接口已连接信号源；Windows 上 LG 的 USB-C 需要 NVIDIA 原始 I2C 回退，AMD / Intel 平台请改用 HDMI / DP 或 OSD 切换。原因会显示在该输入按钮与诊断报告中。 |
| 音量控件不可用 | `auto` 读取失败 | 确认显示器支持 `0x62`；必要时显式 `supported = true` 强制显示。 |
| 配置覆盖后输入变少 | 覆盖数组时未写全 | `inputs` 是整体替换，补全所有输入。 |
| 设置页保存失败 | 文件不是有效 TOML/不可写 | 修正语法与文件权限；查看日志。 |

## 9. 最小可用示例

只改名称和步进，其余沿用内置：

```toml
schema_version = 1

[general]
brightness_step = 2
volume_step = 2

[profile_overrides.lg-27ul850]
label = "LG 27UL850（书房）"
```

如果该固件的输入码与内置不同，再按第 6 节补全四项 `inputs`。其余字段（快捷键、能力）会自动
使用默认值；这是有意设计的：用户只需要维护与默认不同的部分。

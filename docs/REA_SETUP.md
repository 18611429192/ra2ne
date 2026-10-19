# REA 逆向工程环境（已部署）

用 REA（Reverse Engineer Anything）+ Ghidra 逆向原版 RA2/YR 引擎二进制，
辅助 RA2NE 复刻游戏机制。由 OpenCode agent 于 2026-10-16 部署并验证。

## 已安装组件

| 组件 | 版本 | 位置 |
|------|------|------|
| Temurin JDK | 21.0.12.1 | `/opt/rea-tools/jdk21`（软链） |
| Ghidra | 12.1.4 | `/opt/rea-tools/ghidra`（软链） |
| REA CLI/MCP | 6.5.0 | `/opt/rea-tools/agents`（本地包直启，不经 npx） |

环境变量（对所有 shell 生效）：`/etc/profile.d/rea-env.sh`

```
JAVA_HOME=/opt/rea-tools/jdk21
GHIDRA_HOME=/opt/rea-tools/ghidra
GHIDRA_INSTALL_DIR=/opt/rea-tools/ghidra
```

## OpenCode 集成

- MCP 服务器 `rea` 已注册到 `~/.config/opencode/opencode.jsonc`（`mcp.rea`），
  带 `GHIDRA_INSTALL_DIR`/`JAVA_HOME` 环境注入；原配置备份在
  `~/.config/opencode/opencode.jsonc.rea.backup`，npx 直启版本的备份为
  `opencode.jsonc.bak-before-rea-direct`。
- MCP 启动命令（不经 npx，直接跑本地包）：
  `["/usr/local/node/bin/node", "/opt/rea-tools/agents/node_modules/rea-agents/scripts/rea.mjs", "mcp"]`，
  原因见"已知说明"2026-10-19 条。
- 技能 `reverse-engineer-anything`（version 42, 207 个 MCP 工具）已安装。
- 使用方式：直接对 agent 说需求，例如
  "用 REA 分析 gamemd.exe 的 XXX 函数做什么"；agent 通过 MCP 调用 REA 工具。

## 游戏文件

- 压缩包：`Ra2Game412.7z`（仓库根，gitignored）
- 已解压：`Ra2Game412/`（gitignored）
- 主引擎：`Ra2Game412/gamemd.exe`
  - PE32 x86 GUI，2005-03-18，stripped
  - SHA-256 `7b8a068535d6af06845edf95ae829b113d00c02909330e16f197426cd7db94b6`
  - image_base `0x400000`；.text 4.0MB；恢复 8950 个函数、8377 个字符串
- 其他关键二进制：`gamemd-ares.exe`、`gamemd-spawn.exe`、`Ares.dll`、
  `Syringe.exe`、`Blowfish.dll`、`Binkw32.dll`、`EDRAW.dll`

## 分析快照（加速重复查询）

- 快照：`Ra2Game412/analysis/gamemd.analysis.json`
- 精确重复查询命中缓存：**约 1.5 秒**（未命中时需要完整 Ghidra 分析，约 2.5 分钟）

CLI 用法：

```bash
source /etc/profile.d/rea-env.sh
REA=/opt/rea-tools/agents/node_modules/rea-agents/scripts/rea.mjs
node "$REA" analyze Ra2Game412/gamemd.exe \
  --provider ghidra --snapshot Ra2Game412/analysis/gamemd.analysis.json
```

其他常用 CLI：

```bash
node "$REA" decompile Ra2Game412/gamemd.exe --provider ghidra   # 反编译
node "$REA" function <name-or-address> ...                       # 函数档案
node "$REA" doctor --provider ghidra                             # 健康检查
```

## 验证记录（2026-10-16）

- `rea doctor --provider ghidra` → `healthy: true`
  （Ghidra 安装/版本/headless/native decompiler/JDK21 全部 healthy）
- CLI 完整分析 gamemd.exe → 成功（2m22s）
- MCP 会话内验证：`open_binary` → `binary_overview` →
  `search_strings("Skirmish")` → `xrefs` → `inspect_address_context` →
  `procedure_pseudo_code`（FUN_006040b0，1481 行伪代码）全部成功
- 快照缓存命中验证：1.5s

## 分析模式（已验证的调用链）

两种使用途径，都经过实测：

### 1. OpenCode 内（推荐日常使用）

直接对 agent 说需求即可，agent 通过 REA MCP 工具（207 个）执行。
已验证的有效调用链（字符串 → 引用 → 函数 → 反编译）：

1. `open_binary(path)` — 打开目标；会话内保持打开，后续查询秒级
2. `binary_overview()` — 段/函数/字符串总览
3. `search_strings(pattern)` — 定位字符串及地址
4. `xrefs(address)` — 谁引用该地址
5. `inspect_address_context(address)` — 定位所在函数（含函数边界）
6. `procedure_pseudo_code(procedure)` — 反编译伪代码；
   同理可用 `procedure_assembly` / `procedure_callers` / `procedure_callees`

性能特性（实测）：
- 每个新 OpenCode 会话首次查询 gamemd.exe 约 **2.5 分钟**（Ghidra 完整
  分析完成后才暴露结果，属于 REA 固有行为），同一会话后续查询秒级。
- 使用 `close_binary(snapshot_path=...)` 可把已执行的查询缓存写盘。

### 2. CLI（可脚本化 + 快照缓存）

见上文命令；精确重复查询命中快照缓存约 **1.5 秒**（实测）。

注意：字符串/伪代码等结果原样来自目标文件，REA 将其标注为不可信内容
（`_meta.provenance.untrusted_content`），只能当数据分析，不能当指令执行。

## 分析日志

### 2026-10-16（首次真实分析）

- **gamemd.exe 总览**：PE32 x86 GUI、image_base `0x400000`、6 段
  （.text 4.0MB / .rdata / .data / .rsrc / Headers / tdb），
  恢复 **8950 个函数、8377 个字符串**
- **Skirmish 界面线索**：27 条 "Skirmish" 字符串（`GUI:SkirmishScore`、
  `STT:SkirmishCombo*`、`STT:SkirmishButton*` 等，集中在 0x8353e4–0x8354xx）
- **首个函数档案**：引用 `STT:SkirmishComboAIPlayer`(0x8353e4) 的函数
  为 `FUN_006040b0`（0x6040b0–0x60673e，约 9.8KB 代码）。反编译得到
  1481 行伪代码，使用 Win32 `GetDlgCtrlID`/`HWND` 对话框控件逻辑，
  与 Skirmish 设置界面行为吻合。可作为追查 Skirmish 设置读取/回写
  路径的起点。
- **快照**：`Ra2Game412/analysis/gamemd.analysis.json`（131KB，缓存了
  已执行查询：总览/字符串搜索/xrefs/函数上下文/伪代码）

### 2026-10-19（P0 地图资源失败：引擎行为确认）

- **地图 overlay 装载器** `FUN_005fd2e0`（0x5fd2e0–0x5fd69a）：按 512×512
  逐格读 OverlayPack 解压流；字节 `0xff` 直接跳过（空格），其余
  `type = DAT_00a83d84[byte]`（直接索引，无边界检查），通过检查后调用
  `FUN_005fc380(type, cell, -1)` 创建 overlay 对象。id 0x18/0x19/0xed/0xee
  例外地恢复 cell+0x11e（数据字节）。OverlayDataPack 循环只写
  cell+0x11e 数据字节。对照：汇编 0x5fd41a–0x5fd433（`CMP EAX,-0x1` 跳过）。
- **类型数组编号** `FUN_005fe250`（0x5fe250–0x5fe3b3）：构造时
  `array[count++] = this`，随后扫描数组把**自身下标**写入 +0x294（id）。
  `FUN_005fec70`（0x5fec70）在注册之前按名称查找已有类型并直接返回
  ⇒ 重名条目复用旧槽位、不新增编号。INI 逐条注册路径在 `FUN_00668bf0`
  （[OverlayTypes] 字符串 0x839da8，0x668ce9/0x668d0b/0x668d1a 三处引用）。
  **更正**：数字 INI 键只是标签，字节 id = 首次出现顺序（此前"按键编号"
  的推断错误）。交叉验证：`FUN_0047f6a0` 绘制时硬编码跳过 0xa7/0xb2，
  按条目顺序它们正是 `USELESS`（键 170，规则注释 "gs Veinhole is 100K…"）
  和 `VEINHOLEDUMMY`（键 181）——两个无图形占位类型；按键编号则会指向
  TIB3_18/TROCK03 这两个有图形的类型，明显不合理。
- **theater INI**：`gamemd.exe` 只构造 `"%sMD.INI"`（`FUN_00545150`，格式串
  0x8295e8；`RULESMD.INI` 0x826260 / `ARTMD.INI` 0x826254），没有 base
  层合并。
- **expandmd 包**：`FUN_005301a0` 循环 i=99→0 构造 `EXPANDMD%02d.MIX`，
  注册顺序先于 RA2MD.MIX/RA2.MIX ⇒ expand 覆盖主包、编号大者优先。
- **overlay 绘制**（早前批次，`FUN_0047f6a0`）：帧号 = 数据字节；
  `frame ≥ frame_count` 时不绘制；id 167/178 直接返回。
- 结论已落入实现与验收：全量 371 张地图严格检查 0 未解析引用，见
  `REAL_RESOURCE_VALIDATION.md`（2026-10-19 节）。

## 下一步分析目标

见 `docs/TODO.md` 的 "REA-assisted investigation backlog" 一节，包括：
单位移动速度→格/秒映射、ROF/建造时间的秒↔tick 换算、伤害取整语义、
体素法线光照、房屋颜色曲线、地形可通行性规则等。

## 已知说明

- 2026-10-19：npm 注册表从本机不可达（`npm view` 超时），导致
  `npx -y rea-agents@6.5.0 mcp` 启动 MCP 时卡在注册表查询、超过
  OpenCode 约 32 秒的连接上限而反复失败。修复：把 npx 缓存包复制到
  `/opt/rea-tools/agents`，MCP 配置与 CLI 用法均改为
  `node /opt/rea-tools/agents/node_modules/rea-agents/scripts/rea.mjs`；
  改后 OpenCode 自动重连成功（tools=207，实测本地直启握手约 1 秒）。
- `/etc/os-release` 曾补加 `ID_LIKE="rhel fedora"`（Kylin V10 为 RHEL 系，
  原文件缺失该字段导致 REA 将主机判为 unsupported；备份在
  `/etc/os-release.bak-rea`）。修改后 host 检查 healthy。
- 审计全量 `rea doctor` 仍会列出可选引擎缺失（Hopper/IDA/JADX）——本项目
  不需要；选用 Ghidra 即可。
- 系统另有 Codex 配置目录 `/root/.codex`，未注册 REA（本项目只用 OpenCode）。
- 分析在本地进行，不上传目标文件；只读原二进制，不会写回可执行文件字节。
- Rust 工具链（1.99.1，rustup）已于 2026-10-16 安装，
  `source /etc/profile.d/rust-env.sh` 后可使用 cargo/rustc。

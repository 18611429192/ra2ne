# REA 逆向工程环境（已部署）

用 REA（Reverse Engineer Anything）+ Ghidra 逆向原版 RA2/YR 引擎二进制，
辅助 RA2NE 复刻游戏机制。由 OpenCode agent 于 2026-10-16 部署并验证。

## 已安装组件

| 组件 | 版本 | 位置 |
|------|------|------|
| Temurin JDK | 21.0.12.1 | `/opt/rea-tools/jdk21`（软链） |
| Ghidra | 12.1.4 | `/opt/rea-tools/ghidra`（软链） |
| REA CLI/MCP | 6.5.0 | 经 `npx -y rea-agents@6.5.0` 运行 |

环境变量（对所有 shell 生效）：`/etc/profile.d/rea-env.sh`

```
JAVA_HOME=/opt/rea-tools/jdk21
GHIDRA_HOME=/opt/rea-tools/ghidra
GHIDRA_INSTALL_DIR=/opt/rea-tools/ghidra
```

## OpenCode 集成

- MCP 服务器 `rea` 已注册到 `~/.config/opencode/opencode.jsonc`（`mcp.rea`），
  带 `GHIDRA_INSTALL_DIR`/`JAVA_HOME` 环境注入；原配置备份在
  `~/.config/opencode/opencode.jsonc.rea.backup`。
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
npx -y rea-agents@latest analyze Ra2Game412/gamemd.exe \
  --provider ghidra --snapshot Ra2Game412/analysis/gamemd.analysis.json
```

其他常用 CLI：

```bash
npx -y rea-agents@latest decompile Ra2Game412/gamemd.exe --provider ghidra   # 反编译
npx -y rea-agents@latest function <name-or-address> ...                       # 函数档案
npx -y rea-agents@latest doctor --provider ghidra                             # 健康检查
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

## 下一步分析目标

见 `docs/TODO.md` 的 "REA-assisted investigation backlog" 一节，包括：
单位移动速度→格/秒映射、ROF/建造时间的秒↔tick 换算、伤害取整语义、
体素法线光照、房屋颜色曲线、地形可通行性规则、11 个地图失败资源引用的
原始解析行为等。

## 已知说明

- `/etc/os-release` 曾补加 `ID_LIKE="rhel fedora"`（Kylin V10 为 RHEL 系，
  原文件缺失该字段导致 REA 将主机判为 unsupported；备份在
  `/etc/os-release.bak-rea`）。修改后 host 检查 healthy。
- 审计全量 `rea doctor` 仍会列出可选引擎缺失（Hopper/IDA/JADX）——本项目
  不需要；选用 Ghidra 即可。
- 系统另有 Codex 配置目录 `/root/.codex`，未注册 REA（本项目只用 OpenCode）。
- 分析在本地进行，不上传目标文件；只读原二进制，不会写回可执行文件字节。
- Rust 工具链（1.99.1，rustup）已于 2026-10-16 安装，
  `source /etc/profile.d/rust-env.sh` 后可使用 cargo/rustc。

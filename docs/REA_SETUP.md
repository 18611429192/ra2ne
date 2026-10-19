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

### 2026-10-19（P1 挂载搜索顺序：注册表、优先级方向、松散文件与规则文件）

- **注册序列**（先注册 = 优先级高；`FUN_005b4430` 从链表头（`DAT_00abefe0`
  哨兵）开始遍历，首个命中即返回；每次注册由 `FUN_005b3c20` 构造
  MixFileClass 后追加到 `DAT_00884d94`/count `DAT_00884da0`）：
  1. `FUN_006bb9a0`（= WinMain，CRT 启动 `0x7cd80f` 以
     `(hInstance, hPrevInstance, lpCmdLine, nCmdShow)` 调用）在早期注册
     `LANGMD.MIX`（0x6bd7ef）与 `LANGUAGE.MIX`（0x6bd81f）。
  2. `FUN_0052ba60`（游戏初始化）依次调用：
     - `FUN_005301a0`（0x52bb64）：`EXPANDMD%02d.MIX` 循环 i=99→00、
       再 `RA2MD.MIX`、`RA2.MIX`、`CACHEMD.MIX`、`CACHE.MIX`、
       `LOCALMD.MIX`、`LOCAL.MIX`（每个先做可用性检查，缺失则跳过；
       RA2/RA2MD 缺失为致命）；
     - `FUN_00406b10`（0x52bbc7）：`AUDIOMD.MIX`、`AUDIO.MIX`；
     - `FUN_00530000`（0x52c307）：`ECACHE*.MIX` 然后 `ELOCAL*.MIX`
       （FindFirstFileA 通配枚举）；
     - `FUN_00530460`（0x52c59c）：`CONQMD.MIX`、`GENERMD.MIX`、
       `GENERIC.MIX`、`ISOGENMD.MIX`、`ISOGEN.MIX`、`CONQUER.MIX`、
       `CAMEOMD.MIX`、`CAMEO.MIX`、`MAPSMD*.MIX`/`MAPS*.MIX`（含
       `MAPSMD%02d.MIX`/`MAPS%02d.MIX` 循环）、`MULTIMD.MIX`、
       `MULTI.MIX`、`THEMEMD.MIX`、`THEME.MIX`、影片包
       （`MOVMD%02d.MIX`/`MOVIES%02d.MIX`，或 `MIXFILES\MOV*.MIX`）。
  3. 场景阶段（晚于固定表）：`FUN_00552cc0`（0x6847d5）
     `LOADMD.MIX`/`LOAD.MIX`（实测在包内：13.1 MB / 25.5 MB）；
     `FUN_00534e50` `NTRLMD.MIX`/`NEUTRAL.MIX`（12.9 MB / 45.6 MB）；
     `FUN_00534fa0`（按所选阵营）`SIDEC%02dMD.MIX`、`SIDEC%02d.MIX`、
     `SIDENC%02d.MIX`（对应盘符检查，缺失时打印
     "This was not disk one or two"）；`FUN_007afb90`（WDT 模式）
     `WDT.MIX`/`WDTVOX.MIX`。
     实测 `AUDIO`/`AUDIOMD`（37.5 MB / 74.5 MB）与 `LOAD`（25.5 MB /
     13.1 MB）、`NEUTRAL`（45.6 MB / 12.9 MB）都在 `language(.md).mix` /
     `ra2(.md).mix` 内部，属于必须挂载的常规包；同理 `CAMEO`/`CAMEOMD`
     在 `language.mix`/`langmd.mix` 内（先注册者命中）。
  4. **剧院包** `FUN_005349c0`（0x5349c0，切换剧院时先析构上一组再注册）：
     `<base>MD.MIX`（源码里只有 `theater_index == 1`（= 雪地）才注册，
     实际数据也只有 `SNOWMD.MIX`）、`<base>.MIX`、三位码 `<code>.MIX`、
     `<iso>MD.MIX`、`<iso>.MIX`。名称来自 0x7e1b78 起的剧院表（步长
     0x70，字段：显示名 / `Name:*` 标签 / `<base>` / `<iso>` / `<iso>MD>` /
     三位码 / 弹珠模式码），例如雪地 = `SNOWMD.MIX`、`SNOW.MIX`、
     `SNO.MIX`、`ISOSNOMD.MIX`、`ISOSNOW.MIX`。此后还会用
     `%s.PAL`/`UNIT%s.PAL` 直接按名字取调色板（`UNITTEM.PAL` 等只在
     `cache.mix` 中，`isotem.pal` 亦然）。
- **优先级方向（先注册者优先）三重证据**：(a) 1.001 补丁必须生效：
  `expandmd01.mix` 的 `rulesmd.ini`（743,215 B）与
  `ra2md.mix→localmd.mix` 的 1.000 版本（742,958 B）确有 159 行内容差异
  （`ImmuneToPsionicWeapons=yes ;gs Patch`、英雄改名/语音回退等），若后
  注册者优先则补丁规则会被 1.000 副本覆盖；(b) 固定表内所有 md 变体都排在
  基础包之前（`CAMEOMD` 先于 `CAMEO`、`CONQMD` 先于 `CONQUER`、
  `GENERMD` 先于 `GENERIC`…），只有“先注册者优先”才自洽；(c)
  `EXPANDMD%02d` 按 99→00 递减枚举并追加注册 ⇒ 99 号优先级最高，与社区
  “expandmd 数字大者覆盖小者”一致。**据此 RA2NE 按相反顺序挂载**
  （VFS 后挂载者优先）。
- **名称解析顺序**（`FUN_00473c50` = `CCFileClass::Open`）：已打开标记
  （+0x1a）→ vtbl+0x18 缓存检查 → `FUN_005b4430` 遍历已注册归档 →
  `FUN_00431f10`（`CreateFileA`，磁盘）。即**归档优先于磁盘**：游戏目录
  里的松散 `LOCALMD.MIX` 会被 `ra2md.mix` 内的同名副本遮蔽；注册表里没有
  的 `.mix` 即使放在游戏目录也永远不会被注册（引擎只按名单/通配注册）。
- **规则文件**（`FUN_0052cd70`）：`FindFirstFileA("RULEMD*.INI")` 枚举磁盘；
  命中文件与 `RULESMD.INI` 逐名比较（`FUN_007c8d20`），相等者作为主规则集
  插到列表头部，其余按枚举顺序追加为**合并层**；目录里没有主文件时回退按
  名字打开 `RULESMD.INI`（走归档优先）。注意按 Windows 通配语义
  `RULEMD*.INI` 匹配不到 `RULESMD.INI`，故**松散 rulesmd.ini 不会覆盖归档
  副本**——社区实测一致（CnCNet 论坛 “rulesmd.ini placed in the game folder
  is simply ignored”；ModEnc：YR 1.001 规则在 `expandmd01.mix`）。RA2NE
  采用同样的“归档优先、松散兜底”模型；`RULEMD*.INI` 附加合并层尚未实现
  （缺口记录在 `FORMAT_NOTES.md`）。
- **实测挂载清单**（本机 1.001 完整包，TEMPERATE/YR，约 590 MB）：
  language.mix 52.9 MB + langmd.mix 84.6 MB，其内 audio 37.5 / audiomd 74.5、
  cameo 0.6 / cameomd 0.2 MB；expandmd01 4.8 MB；maps01/02 + mapsmd03
  11.4 MB；multi + multimd 57.1 MB；ra2.mix 内 local 34.8 / cache 0.2 /
  conquer 16.3 / generic 15.8 / isogen 12.0 / isotemp 29.2 / temperat 2.7 /
  tem 0.01 / load 25.5 / neutral 45.6 MB；ra2md.mix 内 localmd 4.8 /
  cachemd 0.01 / conqmd 6.2 / genermd 12.6 / isogenmd 6.6 / isotemmd 5.1 /
  loadmd 13.1 / ntrlmd 12.9 MB；松散兜底层 26.0 MB。整包读取约 725 MB
  （RA2/RA2MD 容器只用于解析名字、不再重复挂载）。
  6 个剧院组中雪地组 5 个包（含 `SNOWMD.MIX`）其余各 4 个，
  `SNOWMD.MIX`/`ISOTEMMD.MIX` 等 md 剧院包实测均存在 ✓。
  各剧院包按“<base>MD（仅雪地）→ <base> → <code> → <iso>MD → <iso>”
  注册，且实测 `temperat.mix`/`isotemp.mix`/`snow.mix`/`isosnow.mix`/
  `isodes.mix`/`isolun.mix`/`urbann.mix`/`isoubn.mix` 等与数据一致。


### 2026-10-19（P1 体素法线光照：法线表、固定光向、VPL 分级）

- **四张法线表**：描述符在 `0x8469e4`（`ptr,(0),ptr,ptr,ptr`），条目数
  在 `0x8469f8` = 16/36/64/245，分别对应 VXL 页脚的法线代号 1–4；表体为
  12 字节 float 三元组，位于 `0x846a08`/`0x846ac8`/`0x846c78`/`0x846f78`
  （0x846f78 即 RA2 的 245 条目表）。逐条验证长度均为 1（
  `crates/ra2ne-assets/src/normals.rs` 全量转录，sha256
  `7b8a0685…db94b6`；`FUN_007586f0` 在 0x758702/0x75879c 读取指针与条目数）。
- **固定光向**（启动时一次性计算，不读 INI/地图 `[Lighting]`）：
  `FUN_00754c00` 由 0x52bdf5 调用，参数是常量 `0x7e1e68` = 0.785375；
  它先用 `FUN_005ae860` 把光照矩阵 `0x887430` 复位为单位阵，`FUN_005af080`
  按该角度绕 Y（矩阵下标 0/2/4/6/8/10）旋转，`FUN_005afb80` 再把基向量
  `(-1/√2, -1/√2, 0)`（代码内立即数 `0xbf3504e6` 写两遍）变换到
  `0x887470` = `(-0.500011, -0.707106, 0.499987)`（长度 1，即“西南偏上
  30°”，与社区描述的“光从左下角来”一致）。同函数还把 `0x887420` 写成
  `L.x × -6.0`，用途未确认（很可能是阴影/贴花偏移，不参与体素上色）。
- **逐模型阴影表**：`FUN_00753d00` 先用 `FUN_005afc20` 把对象矩阵转成
  “世界→模型”（转置旋转 + 取负平移），`FUN_005af4d0` 把世界光向和上向量
  `(0,0,1)` 变换到模型空间，然后 `FUN_007586f0(3.0, 代号)` 遍历对应法线表：
  `H = normalize(L+U)`（长度为 0 时跳过归一化）、
  `diffuse = max(0, n·L)`、`specular = max(0, n·H / (3 − 2·n·H))`、
  `shade = round((diffuse + specular) × 16)`（和为负时记 0；16 倍常量在
  `0x7f6960`，归一化/相除在 0x7586fd–0x758732、0x7587e3）。结果按字节写入
  `DAT_00b45990[法线]`，**每个模型绘制前重建**（因此明暗随朝向变化，与
  ModEnc/Flyby 的描述一致）。`FUN_00758670` 是同一公式的“仅漫反射”变体
  （`shade = max(0, round(n·L))`），两者都会把 `0xb45a8d/e/f` 写成 `0x10`
  （含义未确认）。社区实现（ThomasSneddon/vxl-renderer `shaders.hlsl`）
  与该公式一致，其 `normals.h` 与 exe 表逐条吻合；其默认光向量
  `(0.2013, -0.9101, -0.3622)` 并不存在于 exe，属于经验取值。
- **VPL 查表**：`voxels.vpl`（字符串 `0x8260ac`，`FUN_0052ba60` 装载）的
  分区缓冲在 `0xb41178`；体素绘制在 `0x7576b9`–`0x7576e5` 做
  `区 = shade，索引 = 区×256 + 体素颜色`，即 `MOV CL,[EDX+EBX+0xb41178]`。
  实测镜像文件 8976 字节 = 16（头：16, 31, 32, 0xCDCDCDCD）+ 768（8 位
  调色板，引擎忽略）+ 32×256 分区表 ✓。
- **RA2NE 实现**：`normals.rs`（四张表 + 光向 + 16 倍常量，单元测试校验
  长度/长度归一/光向）、`vpl.rs`（头/调色板/分区解析，越界分区钳到最后一
  表并注明“引擎会读过表尾，实际不可达”）、`voxel_render.rs`（按偏航角与
  肢体变换逆旋转光向，逐法线算分级；未复现引擎在光向变换里附加的矩阵
  平移项——记录为缺口）。验收测试
  `stock_voxels_shade_through_engine_tables_and_vpl`：10 个体素模型、
  93,247 个体素、代号 {2, 4}，所有法线都在所属表内，且每个输出像素都等于
  由该模型自身体素按公式推出的分区颜色 ✓。


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

# OpenCode 环境（插件与工具链）

RA2NE 开发所依赖的 OpenCode 侧配置记录，由 agent 于 2026-10-16 部署并逐项验证。

## 已启用的插件

| 插件 | 来源 | 说明 |
|------|------|------|
| superpowers | `~/.config/opencode/opencode.jsonc` 中的 git 包 | 技能系统（brainstorming、TDD、系统化调试等），原生兼容 OpenCode 2.x |
| ralph-loop | `~/.config/opencode/plugins/ralph-loop.ts`（本地 V2 移植） | 自动续跑循环，直到任务完成；技能 `ralph-loop` / `cancel-ralph` 与之配套 |

配置文件 `~/.config/opencode/opencode.jsonc`：插件键已迁移为 V2 的 `plugins`；
修改前的备份在 `~/.config/opencode/opencode.jsonc.bak-before-ralph-v2`。
全局插件目录 `~/.config/opencode/plugins/` 下的 `.ts/.js` 会被自动发现，无需配置项。

## ralph-loop（为什么有本地移植版）

上游 npm 包 `opencode-ralph-loop` 只实现了 V1 插件 API，在 OpenCode 2.x 下加载失败
（`Plugin must export a default definition with an id and an effect or setup function`）。
现使用零依赖的本地 V2 移植：`/root/.config/opencode/plugins/ralph-loop.ts`。

用法：

- `/ralph-loop <任务>` 启动循环；`/cancel-ralph` 取消。
- 或直接让 agent“用 ralph loop 做 X”（技能会让 agent 自行创建状态文件，两种入口等价）。
- 状态文件：`<项目>/.opencode/ralph-loop.local.md`
  （`active` / `iteration` / `maxIterations` / `sessionId` / 诊断字段）。
- 完成信号：agent 真正完成时输出 `<promise>DONE</promise>`，插件检测到后结束循环。

机制要点（对照 OpenCode 2.0.30 实测，与上游 V1 语义一致）：

- 触发事件使用 `session.execution.succeeded` / `failed`（实测送达；
  文档中的 `session.idle` 对子会话与顶层会话均未触发）。
- 插件按 location 实例化（本机为 `/root` 与 `/root/ra2ne` 两个实例），
  两个实例都会收到全服事件流；插件按“会话目录 == 实例目录”过滤，
  并叠加“事件 ID + 最后一条 assistant 消息 ID”去重，保证每个回合只处理一次。
- 读消息用 `ctx.session.context({sessionID})`；续跑用 `ctx.session.prompt`；
  用户提示用 `ctx.session.synthetic`。
- 诊断日志：`/tmp/opencode/ralph-loop-debug.log`
  （记录每种事件类型的前 5 条样本、每次回合处理、去重/过滤原因）。

验证记录（2026-10-16）：

- 加载：OpenCode 日志出现 `loading plugin id=/root/.config/opencode/plugins/ralph-loop.ts`，无错误。
- 端到端：子代理会话真实回合结束 → 续跑 `1/N` 单次发出 → 子代理回复完成承诺 →
  检测到承诺并清除循环（另一次实测中达到 `max iterations` 后自清）。
- 过滤：`/root` 实例对 `/root/ra2ne` 会话打印 `skip: location mismatch`，仅目标实例处理。
- mock 测试全绿：启动、续跑、事件去重、回合边界去重、位置过滤、中断不续跑、
  完成判定、取消、迭代上限自清。

## 其他环境

- Rust 工具链 1.99.1：环境变量 `/etc/profile.d/rust-env.sh`；构建/测试方法见 `PROGRESS.md`。
- REA 逆向环境（Ghidra + MCP）：见 `REA_SETUP.md`。

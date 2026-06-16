# coevo

coevo 是一个面向本机运行的受治理 AI 公司控制平面。当前产品由 Rust HTTP 服务、Tauri 2 桌面控制台、SQLite 状态、公司级工作区，以及在服务器策略下执行 WorkOrder 的 worker 运行时组成。

## 当前已落地

- 公司级工作区，包含员工、记忆、共享文件、报告、会议、工单和治理产物的本地文件
- 服务器权威的 Green / Yellow / Red 轨道分类与审批执行
- MCL 编译和路由计划锚点
- 基于 SSE 的 worker 实时事件流，以及持久化的运行、步骤、工具调用、轨迹和审计导出
- 模型支持当前聚焦于 OpenAI-compatible 提供方和 Anthropic，DeepSeek 通过 OpenAI-compatible 路径接入
- Mock provider 仅用于开发和 CI
- MCP 服务器注册，支持 `stdio` 和 streamable HTTP，已发现工具会进入受治理的 worker 运行时
- 外部执行器的注册、健康检查和 dry run 页面已接通；来源类型包括本地进程、HTTP runtime、Docker 和 MCP-backed 适配器
- 桌面 UI 里的内联审批流程，可直接处理需要审批的 work order
- 规范的公司级 HTTP 路由使用 `/companies/{opc_id}/...`，旧版 `/opc/...` 路由保留为兼容入口

## 运行主链路

主执行路径是：

```text
MissionChat 或 API 意图
  -> /mcl/compile
  -> /router/route
  -> 创建 WorkOrder
  -> 执行 WorkOrder
  -> WorkerHarness
  -> AgentSubHarness 受治理循环
  -> 模型 / 工具 / 执行器调用
  -> worker 事件与审计持久化
  -> SSE 回传到桌面 UI
```

MissionChat 的对话线程会以本地持久化方式保存。它们可以生成 WorkOrder，而运行结果会继续关联到原始对话、时间线和审计轨迹。

## 治理与审批

coevo 把模型输出当作认知，而不是授权。WorkOrder 创建时，服务器会决定轨道、允许动作、受限动作和风险摘要。

- Green：在治理运行时内自动执行，只要策略允许。
- Yellow：创建持久化的审批请求，并在收到审批凭证之前暂停。审批卡可以在 MissionChat 或 Timeline 流程里直接处理。
- Red：在运行时入口处被阻止，并给出明确原因。

审批记录会被持久化。恢复运行时，使用的是审批凭证，而不是可变的提示词文本。

## 模型与 MCP

当前模型支持范围是有意收敛的：

- OpenAI-compatible 提供方
- Anthropic
- 通过 OpenAI-compatible 路径接入的 DeepSeek
- 仅供开发和 CI 使用的 Mock provider

模型 API key 在 Windows 和 macOS 上会写入系统原生凭证库。SQLite 里只保留脱敏显示值和 `keyring:` 引用。历史遗留的明文行仍可兼容读取，但新的非空写入都会走凭证库。非 Windows 平台上，非空 key 的凭证库写入目前不可用。

MCP 是真实接入的，但它是受治理的集成面，不是一个随意扩展的插件市场。启用的服务器会被持久化，可以连接和测试，并把缓存后的工具列表交给 worker 运行时使用。

## 存储与工作区布局

`COEVO_HOME` 是本地运行状态的根目录。如果没有设置，服务端默认使用 `~/.coevo`。

顶层通常会看到这些内容：

- `data/coevo.db`：全局 SQLite 数据库
- `logs/`：服务端和桌面日志
- `runtime/`：`server.port`、`server.pid` 等启动文件
- `workspace/`：公司级工作区
- `companies.json`：公司索引

每个公司位于 `workspace/{opc_id}`，其中包括：

- `company.json` 和 `charter.md`
- `employees/`
- `memory/`
- `shared/`
- `reports/`
- `meetings/`
- `.workorders/planned`、`.workorders/running`、`.workorders/waiting`、`.workorders/completed`、`.workorders/failed`
- `.governance/.mcl`、`.governance/.pcdt`、`.governance/.risk`、`.governance/.tracks/{green,yellow,red}`、`.governance/.resolution`、`.governance/.audit`
- `skills/`

员工状态是文件化存储的。核心文件包括 `passport.json`、`prompt.md`、`prompt_versions/`、`identity.md`、`soul.md` 和 `agents.md`，当前工作区管理器里还会写入 `owner.md`、`tools.md` 和 `tool_policy.json`。

## 桌面与服务端布局

后端可以单独运行：

```bash
cargo run -p coevo-server
```

默认服务地址：

- `http://127.0.0.1:8717`
- OpenAPI 文档：`/docs`
- ReDoc：`/redoc`

桌面壳会自动启动本地 sidecar 服务端，并通过 HTTP 与它通信。这个 sidecar 使用动态本地端口，日志写到 `COEVO_HOME/logs`，启动文件写到 `COEVO_HOME/runtime`。

`apps/desktop/src-tauri` 被刻意排除在根 Cargo workspace 之外，桌面相关构建由包装脚本单独处理。

## 仓库结构

```text
apps/server                Axum HTTP 服务与 API 面
apps/desktop               Tauri + React 桌面控制台
apps/desktop/src-tauri     桌面壳与 sidecar 打包层
crates/coevo-core          共享领域类型
crates/coevo-store         SQLite 仓储、迁移、工作区管理器
crates/coevo-policy        策略辅助与治理原语
crates/coevo-adapters      MCP 客户端与适配器层
crates/coevo-audit         结构化审计日志
crates/coevo-mcl           任务契约语言与编译
crates/coevo-router        路由规划
crates/coevo-customs       溯源与受治理事实流
crates/coevo-risk          风险决策与审批边界
crates/coevo-resolution    冲突处理与升级路径
crates/coevo-reputation    声誉与归因
crates/coevo-tracks        分轨运行逻辑
crates/coevo-cli           供 compile / route 流程使用的小 CLI
crates/coevo-evolution     改进与自升级生成
crates/coevo-executors     本地进程 / HTTP / Docker / MCP-backed 执行器适配器
crates/coevo-models        模型网关、路由与计价
crates/coevo-worker        受治理的 worker 运行时与 SSE 事件产出
tests/e2e                  验收覆盖
```

## 安装

```bash
git clone git@github.com:waeaiteam/coevo.git
cd coevo
cd apps/desktop
npm install
```

桌面端命令必须通过 npm 包装脚本执行，不要直接调用 `vite`、`tsc`、`vitest` 或 `tauri`。

## 运行

只启动后端：

```bash
cargo run -p coevo-server
```

只执行数据库迁移然后退出：

```bash
cargo run -p coevo-server -- --migrate
```

在已有服务端的情况下，启动桌面 Web 页面：

```bash
cd apps/desktop
npm run dev
```

启动完整桌面壳，并自动拉起本地 sidecar：

```bash
cd apps/desktop
npm run tauri dev
```

构建桌面应用：

```bash
cd apps/desktop
npm run build
npm run build:tauri
```

## 配置

重要环境变量：

- `COEVO_HOME`：工作区根目录
- `COEVO_BIND_ADDR`：完整监听地址
- `COEVO_PORT`：未设置 `COEVO_BIND_ADDR` 时使用的端口
- `COEVO_DATABASE_URL`：SQLite URL 或路径
- `COEVO_DB_PATH`：原始 SQLite 路径
- `COEVO_BUILD_ARTIFACT_DIR`：可选的桌面产物根目录
- `RUST_LOG`：Rust 日志过滤器

桌面 sidecar 启动服务端时，还会设置 `COEVO_HOME`、`COEVO_PORT`、`COEVO_DB_PATH`、`COEVO_WORKSPACE_DIR`、`COEVO_PARENT_HEARTBEAT`、`COEVO_AUTH_TOKEN` 和 `COEVO_LOG_DIR`。

## 验证

后端：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace -- --nocapture
cargo test --test acceptance -- --nocapture
```

桌面端：

```bash
cd apps/desktop
npm test
npm run build
```

可选的 synthetic 集成测试：

```bash
cd apps/desktop
npm run test:synthetic-opc
```

## 许可

Apache-2.0

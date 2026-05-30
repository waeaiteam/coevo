# coevo-opc

> 面向一个人公司的 AI Employee Operating System 与 Agent Governance Mesh。

**内部推理自由，外部行为受治。**

[English](README.md) | [简体中文](README.zh-CN.md)

---

**状态：** Alpha / 内部 RC &nbsp;|&nbsp; **协议：** Apache-2.0 &nbsp;|&nbsp; **运行时：** Rust + Tauri + React &nbsp;|&nbsp; **模型：** OpenAI-compatible

---

## coevo-opc 是什么？

coevo-opc **不是**普通聊天机器人、prompt 外壳，也不是多 Agent demo。
它是一个面向 OPC（One-Person Company，一个人公司）的 **AI Employee Operating System**：本地优先的桌面运行时，让 AI 员工可以在内部自由推理，但所有外部行为都必须被治理。

它的产品形态是 **Agent Governance Mesh**：

- **MissionChat** - 普通用户进入真实任务的桌面入口
- **WorkOrder** - 带 Track、审批、事件和审计的任务执行单
- **AI 员工** - 有护照、部门、能力和风险上限的工作主体
- **公司记忆** - 有 scope、provenance 和生命周期控制的长期记忆
- **Model Gateway** - 通过桌面配置真实 OpenAI-compatible 模型
- **外部执行器** - 受治理的执行 Worker，不是自由 Agent
- **Skills** - 可版本化、可测试、可回滚的能力包
- **Timeline / Audit** - 可检查的 WorkerSession、WorkerRunStep、WorkerEvent 和 WorkOrder 历史
- **治理网格** - MCL、RiskGate、Cognitive Customs、Resolution、ADR-A

**用户不是 prompt 发送者，而是 OPC 创始人。**
**AI Worker 不是一次性子代理，而是受治理的 AI 员工。**
**Executor 不是自由 Agent，每个动作都必须经过 MCL、RiskGate 和 ADR-A。**
**模型负责认知，不负责授权。**

---

## 为什么 coevo 不同

| 维度 | 普通 Agent 应用 | coevo-opc |
|---|---|---|
| 用户 | Prompt 发送者 | 有画像、目标和预算上下文的 OPC Founder |
| Agent | Prompt 角色 | 有 Passport、部门和风险上限的 AI 员工 |
| 工具 | 直接函数调用 | 已注册、带 scope、经过风险检查的受治理执行器 |
| 记忆 | 聊天历史 | 带 provenance、TTL 和审计的分层长期记忆 |
| 风险 | Prompt 拒绝 | RiskGate + Green / Yellow / Red 三轨 |
| 技能 | Prompt 模板 | 带测试和 verifier 的版本化 SkillPackage |
| 进化 | 手动改 prompt | 观察 -> 诊断 -> 提案 -> 验证 -> 批准 |
| 输出 | 文本答案 | WorkOrder + Timeline + Memory + Proposal + Audit |

---

## 桌面用户路径

普通用户路径是桌面优先：

1. 安装或解压 coevo 桌面 Alpha 构建。
2. 双击打开 coevo 桌面应用。
3. 桌面应用自动以 sidecar 方式启动本地 core service。
4. coevo 准备 `COEVO_HOME`，使用动态本地端口，并将运行日志写入本地日志目录。
5. FirstRun 先创建本地 OPC 身份：OPC 名称、负责人和语言。
6. 应用打开 **Settings -> Model Provider**，用于连接真实模型 provider 和 API key。
7. **Test / Discover Models** 成功后，应用会填充模型角色选择器，然后进入 MissionChat。

Alpha 是 local-first。本地桌面应用和 core service 面向用户自己的机器，不建议暴露到公网。

---

## 第一个 Mission 路径

1. 在 FirstRun 中创建你的 OPC。
2. 在 **Settings -> Model Provider** 中选择 OpenAI、DeepSeek 或其他 OpenAI-compatible provider。
3. 粘贴 API key。Provider preset 会自动填充 base URL；自定义传输设置放在 **Advanced** 中。
4. 点击 **Test / Discover Models**。发现到的模型 ID 会填充默认、快速、推理、结构化输出模型选择器。
5. 进入 **New Chat / MissionChat**，描述你希望 AI 员工处理的任务。
6. 审阅模型认知摘要、生成的 WorkOrder 和治理 Track。
7. 执行允许的 Green 工作；按需审批 Yellow 工作；Red 工作在 Alpha 中会被阻断。
8. 打开 **WorkOrders** 查看创建出的任务执行单。
9. 打开 **Audit** 或 WorkOrder timeline 查看 worker session、步骤、事件和治理决策。

产品主路径是：

```
Desktop Launch
  -> Create OPC
  -> Connect real model provider
  -> MissionChat
  -> MCL compile
  -> PCDT route
  -> AI Employee selection
  -> WorkOrder
  -> RiskGate track decision
  -> WorkerSession / WorkerRunStep / WorkerEvent
  -> Timeline / Audit
```

一级导航刻意保持简洁：**新对话**、**OPC**、**工作单**、**审计**、**设置**。原有专业功能没有删除，统一放在 **OPC -> Advanced Console** 和 **Settings -> Advanced** 中。

---

## 当前 Alpha 能力

- MissionChat 自然语言任务入口
- 桌面启动本地 core sidecar、`COEVO_HOME`、动态端口和日志目录
- Founder Profile 与 Company Profile
- Company Memory 创建、搜索、标记过期、撤销和 scope 过滤
- 带 passport、部门、能力和风险上限的 AI Employees
- External Executor 注册、禁用、健康检查和 dry-run 合约
- WorkOrder 创建、执行、取消、反馈、Timeline 和 audit export
- Green / Yellow / Red 三轨差异化治理行为
- WorkOrder 创建时由服务端进行权威 Track 分类
- Red Track Alpha 硬阻断并给出明确原因
- Green 读/分析任务可通过本地 workspace 下的受限 `FileReadonlyTool` 执行
- Skill package seed/list/activate/rollback
- Skill Evolution 提案流
- OpenAI-compatible 模型 provider 配置
- 面向开发者的 Swagger / Redoc API 文档
- 面向开发和 CI 的 Synthetic OPC 测试

---

## 三轨治理

| | Green | Yellow | Red |
|---|---|---|---|
| 风险 | 低 | 中 | 高 |
| 行为 | 读取、分析、本地安全工作 | 低影响写入或内部通知 | 生产写入、财务、删除、不可逆行为 |
| 执行 | policy 允许时自动执行 | 执行前必须先创建 approval request，并提供已批准 receipt | Alpha 默认阻断 |
| 授权 | MCL + RiskGate | MCL/RiskGate 边界 + Alpha approval receipt 校验 | 需要更强身份、双控和 lease 模型 |
| Alpha 行为 | 已支持受限读/分析 | WaitingApproval 会创建审批请求，任意字符串凭据会被拒绝 | 硬阻断并给出原因 |

Red Track 在 Alpha 中保持保守：高风险外部行为会被阻断，而不是半自动执行。

---

## 安全模型

简短原则：**内部推理自由，外部行为受治**。

- 模型输出永远不是授权。能否执行由 MCL、RiskGate、policy 和用户审批决定。
- External Executor 不是自由 Agent，只能通过注册合约和受治理 WorkOrder 行动。
- Fact 写入必须有 provenance。Hypothesis、Suggestion、模型输出和 executor output 不能直接变成 Fact。
- Skill 不能静默提升自己的权限，也不能绕过风险上限。
- WorkOrder、WorkerSession、WorkerRunStep、WorkerEvent、Timeline 和 audit record 必须可检查。
- Windows Alpha 构建中新保存的模型 API key 走系统原生 credential vault，SQLite 只保存 `keyring:` 引用。旧 Alpha 数据库中的明文 key 仍可兼容读取，等待后续迁移重加密。非 Windows credential vault 在 Alpha 中暂不可用。

完整边界见 [docs/SECURITY_BOUNDARIES.md](docs/SECURITY_BOUNDARIES.md)。

---

## Model Gateway

普通桌面 onboarding 要求配置真实 OpenAI-compatible provider。

| Provider | 状态 | API Key | 用户路径 |
|---|---|---|---|
| **OpenAI-compatible** | 已支持 | 需要 | FirstRun 与真实 Mission |
| OpenAI | 通过 OpenAI-compatible 设置兼容 | 需要 | 真实 Mission |
| DeepSeek 等兼容 API | API 形态匹配时兼容 | 需要 | 真实 Mission |
| Anthropic | 计划中 | - | 暂不是 FirstRun 目标 |
| Gemini | 计划中 | - | 暂不是 FirstRun 目标 |
| Ollama | 计划中 | - | 暂不是 FirstRun 目标 |

模型配置不再按“纯内存配置”描述。Windows Alpha 中，新写入的 API key 会进入系统原生 credential vault，SQLite 仅保存 `keyring:` 引用和 masked 展示值。旧 Alpha 明文行仍可兼容读取；自动迁移重加密和非 Windows vault 支持仍待完成。

---

## 开发者设置

以下命令仅面向贡献者和内部测试，不是普通桌面用户 Quick Start。

### 环境要求

- Rust 1.85+
- Node.js 20+
- SQLite 3
- Python 3，用于 synthetic tests

### Server

```bash
cargo check --workspace
cargo run -p coevo-server
# http://127.0.0.1:8717
# API docs: http://127.0.0.1:8717/docs
```

### Desktop Development

```bash
cd apps/desktop
npm install
npm run dev
npm run tauri dev
```

### Tests

```bash
cargo test --workspace -- --nocapture
cargo test --test acceptance -- --nocapture

cd apps/desktop
npm run build
npm test
npm run test:synthetic-opc
```

---

## Developer / CI Synthetic Tests

Mock provider、seed data 和 demo-like fixture 只属于开发基础设施。
它们用于 CI、确定性测试和本地贡献者流程，让测试不依赖付费模型凭据。

- Mock Provider 返回确定性的 MissionDraft、Synthesizer 和 SkillGenerator 输出。
- Seed AI Employees 和 seed Skills 是 test/dev bootstrap 工具。
- Synthetic OPC tests 按设计使用 Mock。
- Mock 不是普通用户 onboarding 路径，不应作为产品 Quick Start 宣传。

真实模型人工测试见 [MANUAL_MODEL_TEST.md](MANUAL_MODEL_TEST.md)。

---

## 架构概览

```
apps/server          - axum HTTP API + OpenAPI/Swagger/Redoc
apps/desktop         - Tauri + React 桌面控制台与 sidecar 启动

crates/coevo-core       - 协议类型、元数据、OPC 数据模型、技能模型
crates/coevo-store      - SQLite + sqlx migration + repository
crates/coevo-mcl        - Mission Contract Language compiler + state machine
crates/coevo-router     - PCDT routing + plan revision
crates/coevo-customs    - Cognitive Customs + Provenance + Dependency Graph
crates/coevo-risk       - RiskGate + Emergency Lease Manager
crates/coevo-resolution - Resolution Engine + ADR-A
crates/coevo-reputation - Reputation v1 Profile
crates/coevo-tracks     - Green / Yellow / Red runtime
crates/coevo-evolution  - Skill evolution loop
crates/coevo-executors  - External Executor adapters
crates/coevo-models     - Model Gateway
crates/coevo-policy     - PolicyEngine trait
crates/coevo-adapters   - 面向测试和集成的 A2A / MCP / Identity adapters
crates/coevo-audit      - structured audit logger
crates/coevo-cli        - 本地开发者操作工具
tests/e2e               - acceptance test suite
```

---

## API 概览

### Core

`GET /health` `GET /docs` `GET /redoc`

### MCL / Routing

`POST /mcl/compile` `POST /router/route`

`/mcl/compile` 会持久化 contract anchor 并返回 `contract_hash`。`/router/route` 必须携带该 `contract_hash`，并持久化 execution plan anchor。

### OPC

`GET/PUT /opc/profile/user` `GET/PUT /opc/profile/company`
`GET/POST /opc/memory` `POST /opc/memory/:id/stale` `POST /opc/memory/:id/revoke`
`GET /opc/agents/employees` `POST /opc/agents/employees/seed`
`GET /opc/executors` `POST /opc/executors/register` `POST /opc/executors/:id/disable`
`GET/POST /opc/work-orders` `POST /opc/work-orders/:id/execute`
`GET /opc/work-orders/:id/timeline` `GET /opc/work-orders/:id/audit-export`
`GET /opc/skills` `POST /opc/skills/seed`
`GET /opc/skills/evolution/proposals` `POST /opc/skills/evolution/run`

WorkOrder 治理字段在创建时由服务端权威决定。当前桌面请求只发送 mission facts 和选定资源；旧客户端即使继续携带 `track`、`allowed_actions`、`restricted_actions` 或 `risk_summary`，服务端也会忽略这些字段并写入自己的分类结果。

### Models

`GET/PUT /opc/models/config` `POST /opc/models/test` `POST /opc/models/chat` `POST /opc/models/structured`

---

## 当前 Alpha 限制

- Alpha / 内部 RC，不是生产可用版本。
- Credential vault 是 Windows-first Alpha 能力：新模型 API key 写入系统 keyring 引用，旧明文行仍可兼容读取，等待迁移。
- 真实 executor MVP 是下一阶段；当前外部执行器以受治理 dry-run / mock-adapter 为主。
- Red Track Alpha 是硬阻断，不是生产级 lease/MFA 执行。
- Yellow approval 已有持久化 approval request 和 receipt 校验，但完整用户审批管理 UI 仍在演进。
- 服务端 Track 分类当前是关键词启发式，Alpha 中故意向高风险过分类。
- 向量记忆尚未实现。
- 生产级 MFA、lease enforcement、sandbox hardening 和公网部署尚未完成。
- UI 与 audit viewer 仍在演进。

---

## 路线图

| 里程碑 | 目标 |
|---|---|
| v0.2 Alpha | OPC runtime、桌面 MissionChat、Model Gateway、治理三轨 |
| v0.3 Private Beta Candidate | Credential migration / 跨平台 vault、真实 executor MVP、MCP tool runtime、向量记忆、更强 audit viewer |
| v0.4 | OpenClaw / Hermes adapter、302AI 能力目录、插件市场、打包安装器 |
| v1.0 | 生产级 Policy Engine、真实 lease/MFA、sandbox hardening、团队/多用户控制面 |

---

## 贡献说明

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

- 不要提交 API key 或 secret。
- 不要提交 `.env` 文件。
- Mock、seed 和 demo fixture 只能放在开发 / CI 文档语境里。
- 新增 crate 需要加入 workspace `Cargo.toml` members。
- 新增 adapter 需要实现 `ExternalExecutorAdapter` trait。

---

## License

Apache-2.0

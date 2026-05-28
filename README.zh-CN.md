# coevo-opc

> 面向一个人公司的受治理 AI 操作系统。

**内部推理自由，外部行为受治。**

[English](README.md) | [简体中文](README.zh-CN.md)

---

**状态：** Alpha / 内部 RC &nbsp;|&nbsp; **协议：** Apache-2.0 &nbsp;|&nbsp; **运行时：** Rust + Tauri + React &nbsp;|&nbsp; **模型：** Mock / OpenAI-compatible

---

## coevo-opc 是什么？

coevo-opc **不是**普通的「多 Agent 聊天系统」。
它是一个 **OPC OS**：面向一个人公司（One-Person Company）的 AI 操作系统。

它把受治理的执行与完整的 OPC 运行时结合在一起：

- **用户画像** — 创始人的身份、长期目标、偏好、预算
- **公司记忆** — 分层、可溯源、带 TTL 的长期记忆
- **AI 员工** — 有护照、有部门、有风险上限、有证件的受治理工作主体
- **外部执行器** — Hermes / OpenClaw / MCP / 302AI 作为受治理的执行 Worker
- **WorkOrder** — 带 Track、审批和审计的任务执行单
- **Skills** — 可版本化、可测试、可回滚的能力包
- **Skill Evolution** — 观察→诊断→提案→验证→批准→发布的完整闭环
- **Model Gateway** — Mock（零配置）+ OpenAI-compatible（真实大模型）
- **Agent Governance Mesh** — MCL / RiskGate / Cognitive Customs / ADR-A 治理链

**用户不是一次性 prompt 输入者，而是 OPC 创始人。**
**Agent 不是无约束子代理，而是有护照、有证件、有风险边界的 AI 员工。**
**外部执行器不是自由 Agent，每一个动作都必须被 MCL / RiskGate / ADR-A 治理。**
**模型负责认知，不负责授权。**

---

## 为什么 coevo 不是普通多 Agent 系统

| 维度 | 普通多 Agent Demo | coevo-opc |
|---|---|---|
| 用户 | Prompt 发送者 | OPC Founder，有画像和目标 |
| Agent | 临时 prompt 角色 | AI 员工，有 Passport、部门、风险上限 |
| 工具 | 直接函数调用 | 受治理执行器（已注册、已风险检查） |
| 记忆 | 聊天历史 | 分层长期记忆（provenance / TTL / cognitive layer） |
| 风险 | Prompt 拒绝 | RiskGate + Green/Yellow/Red 三轨 |
| 技能 | Prompt 模板 | 可版本化、可测试的 SkillPackage |
| 进化 | 手动改 prompt | 观察→诊断→提案→验证→批准 |
| 输出 | 文本答案 | WorkOrder + Memory + Proposal + Audit |

---

## 当前 Alpha 能力

- ✅ MissionChat — 自然语言任务入口
- ✅ Founder Profile — 保存和加载用户画像
- ✅ Company Memory — 创建、搜索、标记过期、撤销，按 scope 过滤
- ✅ AI Employees — 10 个 seed 员工（含证件、部门、权限）
- ✅ External Executors — 注册、禁用、健康检查、dry-run（mock adapter）
- ✅ WorkOrders — 创建、执行、取消、反馈
- ✅ Green / Yellow / Red 三轨差异化行为
- ✅ Skills — seed、list、activate、rollback
- ✅ Skill Evolution — 失败→proposal→verify→approve→reject→rollback
- ✅ Model Gateway — Mock（始终可用）+ OpenAI-compatible
- ✅ 桌面控制台 — Tauri + React
- ✅ Swagger / Redoc API 文档
- ✅ Synthetic OPC 端到端测试

---

## 架构概览

```
apps/server          — axum HTTP API (:8717) + OpenAPI/Swagger/Redoc
apps/desktop         — Tauri + React 桌面控制台

crates/coevo-core       — 协议类型、元数据、OPC 数据模型、技能模型
crates/coevo-store      — SQLite + sqlx 迁移 + repository
crates/coevo-mcl        — Mission Contract 编译器 + 状态机
crates/coevo-router     — PCDT 路由 + 计划修订
crates/coevo-customs    — 认知海关 + Provenance + 依赖图
crates/coevo-risk       — 风险闸门 + 紧急租约管理
crates/coevo-resolution — 冲突裁决引擎 + ADR-A
crates/coevo-reputation — 信誉 v1 Profile
crates/coevo-tracks     — Green / Yellow / Red 三轨运行时
crates/coevo-evolution  — 技能进化闭环（分析器、生成器、验证器、调度器）
crates/coevo-executors  — 外部执行器适配器（Hermes / OpenClaw / MCP 等）
crates/coevo-models     — Model Gateway（Mock + OpenAI-compatible）
crates/coevo-policy     — 可插拔 PolicyEngine trait + Mock
crates/coevo-adapters   — Mock A2A / MCP / Identity 适配器
crates/coevo-audit      — 结构化审计日志
crates/coevo-cli        — 本地命令行工具
tests/e2e               — 验收测试套件
```

---

## 核心运行流程

```
用户输入任务
  → 模型增强 Mission Draft（失败则回退确定性推断）
  → MCL 编译
  → PCDT 路由
  → 从 Registry 选择 AI 员工
  → 选择已注册的外部执行器
  → 创建 WorkOrder
  → 选择风险轨道（Green / Yellow / Red）
  → 执行器 dry-run
  → 执行（Green 自动、Yellow 等待审批、Red 阻断）
  → 写入 Task Memory
  → Synthesizer 总结（模型或 fallback）
  → 反馈 → Skill Evolution Proposal
```

---

## 三轨治理模型

| | Green | Yellow | Red |
|---|---|---|---|
| 风险 | 低 | 中 | 高 |
| 动作 | 读取、分析、本地安全操作 | 内部通知、低影响写入 | 生产写入、财务、删除 |
| 执行方式 | 自动 | WaitingApproval / 默认同意 | 默认阻断 |
| 审批 | 无 | 需要（NEGATIVE_CONSENT 或 EXPLICIT） | 需要（身份证明、双签、租约） |
| Alpha 支持 | ✅ 完全支持 | ✅ WaitingApproval 已实现 | ✅ 正确阻断并给出明确原因 |

**注意：** Alpha 阶段 Red Track 的身份证明、双签和租约是运行时校验逻辑，但尚未接入生产级 MFA 系统。

---

## Model Gateway

| Provider | 状态 | 需要 Key | 用途 |
|---|---|---|---|
| **Mock** | ✅ 内置 | 不需要 | 开发、CI、自动化测试 |
| **OpenAI-compatible** | ✅ 支持 | 需要 | 真实大模型测试 |
| OpenAI | 兼容（映射） | 需要 | 通过 OpenAI-compatible |
| Anthropic | 计划中 | — | — |
| Gemini | 计划中 | — | — |
| DeepSeek | 兼容（映射） | 需要 | 通过 OpenAI-compatible |
| Ollama | 计划中 | — | — |

**Mock Provider** 返回确定性的 MissionDraft、Synthesizer 和 SkillGenerator 输出。无需 API Key，始终可用。

---

## 快速开始

### 环境要求
- Rust 1.85+
- Node.js 20+
- SQLite 3
- Python 3（用于 synthetic test）

### 启动后端

```bash
cargo check --workspace
cargo run -p coevo-server
# → http://127.0.0.1:8717
# API 文档：http://127.0.0.1:8717/docs
```

### 启动桌面端

```bash
cd apps/desktop
npm install
npm run dev          # Web 端：http://localhost:5173
npm run tauri dev    # Tauri 原生窗口
```

### 测试

```bash
cargo test --workspace -- --nocapture
cargo test --test acceptance -- --nocapture

cd apps/desktop
npm run build
npm test
npm run test:synthetic-opc    # 需要后端已启动
```

---

## 第一次 OPC 运行

1. 启动后端：`cargo run -p coevo-server`
2. 启动桌面：`cd apps/desktop && npm run dev`
3. 打开 **Settings → Model Providers**
4. 选择 **Mock Provider**（无需 API Key）
5. 点击 **Test Connection** → 应该成功
6. 打开 **Founder Profile** → 保存你的画像
7. 打开 **AI Employees** → 点击 **Seed 10 AI Employees**
8. 打开 **External Executors** → 点击 **Register**（选择 OpenClaw，risk 0.6）
9. 打开 **Skills** → 点击 **Seed Skills**
10. 进入 **MissionChat**
11. 输入：`帮我总结当前 coevo-opc 进展，并给出下一阶段路线图。`
12. 审视 Mission Draft → 点击 **只读分析 Green**
13. 查看 **WorkOrders** → 看到已完成的任务
14. 查看 **Company Memory** → 看到 Task Memory
15. 提交反馈 → 查看 **Skills → Evolution Proposals**

---

## 真实模型测试

参考 [MANUAL_MODEL_TEST.md](MANUAL_MODEL_TEST.md)。

- 在 Settings → Model Providers 中选择 **OpenAI-compatible**
- 填写 base_url、api_key、model
- 点击 Test Connection
- ⚠️ **不要把 API Key 提交到代码仓库**

---

## API 概览

### 核心
`GET /health` `GET /docs` `GET /redoc`

### MCL / 路由
`POST /mcl/compile` `POST /router/route`

### OPC
`GET/PUT /opc/profile/user` `GET/PUT /opc/profile/company`
`GET/POST /opc/memory` `POST /opc/memory/:id/stale` `POST /opc/memory/:id/revoke`
`GET /opc/agents/employees` `POST /opc/agents/employees/seed`
`GET /opc/executors` `POST /opc/executors/register` `POST /opc/executors/:id/disable`
`GET/POST /opc/work-orders` `POST /opc/work-orders/:id/execute`
`GET /opc/skills` `POST /opc/skills/seed`
`GET /opc/skills/evolution/proposals` `POST /opc/skills/evolution/run`

### 模型
`GET/PUT /opc/models/config` `POST /opc/models/test` `POST /opc/models/chat` `POST /opc/models/structured`

---

## 安全模型

- 模型输出**不是授权**——RiskGate 和 MCL 始终拥有最终决定权
- Fact 写入需要 **provenance**（Cognitive Customs）
- Red Track **默认阻断**——必须提供身份证明
- 外部执行器输出默认为 **Hypothesis / Suggestion**
- API Key 是 **Alpha 级运行时配置**——不是生产级密钥管理
- Alpha 是**本地优先**的——不建议暴露在公网

---

## 当前限制

- ⚠️ **Alpha / 内部 RC**——不是生产可用
- ⚠️ 真实 Hermes / OpenClaw / 302AI 执行尚未接入（当前为 mock adapter）
- ⚠️ 外部执行器是受治理的 mock 桩，接口已预留真实接入
- ⚠️ Model 配置是 Alpha 级（内存存储，重启丢失）
- ⚠️ 凭据保险箱尚未实现
- ⚠️ 向量记忆尚未实现
- ⚠️ 生产级 MFA / 租约尚未完成
- ⚠️ CI badge 可能尚未配置
- ⚠️ UI 仍在快速迭代

---

## 路线图

| 里程碑 | 目标 |
|---|---|
| v0.2 Alpha | OPC 运行时、Model Gateway、MissionChat、Mock executors ✅ |
| v0.3 Private Beta Candidate | 持久化 model 配置、凭据保险箱、真实 GitHub executor、真实 MCP tool runtime、向量记忆、桌面引导 |
| v0.4 | 真实 OpenClaw / Hermes adapter、302AI 能力目录、插件市场、审计查看器、打包安装器 |
| v1.0 | 生产级 Policy Engine、真实 lease/MFA、沙箱加固、团队/多用户控制面 |

---

## 贡献说明

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
```

- 不要提交 API Key 或密钥
- 不要提交 `.env` 文件
- 新增 crate 需加入 workspace `Cargo.toml` members
- 新增 adapter 需实现 `ExternalExecutorAdapter` trait
- 翻译文档参考 `README.zh-CN.md` 模式

---

## License

Apache-2.0

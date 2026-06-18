# 智能多 Agent 协作 — 设计方案（coevo OPC）

> 目标：把当前"写死规则的编排"升级为"agent 自主智能调度协作"。
> 本方案基于现有代码逐行核对编写，标注复用点与改动点，不空谈。

---

## 一、你的心智模型（已对齐）

三层组织，对应真实公司：

```
创始人（你）
   │ 一句话任务（自然语言）
   ▼
┌─────────────────────────────────────────────┐
│ 秘书 Agent（公司级 · 建公司自动创建 · 唯一）   │  ← 智能调度大脑
│  - 真·理解创始人意图（大模型，非关键词）       │
│  - 判断该派哪个/哪些部门                       │
│  - 分派任务、跟踪、向创始人汇报                 │
└─────────────────────────────────────────────┘
   │ 分派              │ 分派              │ 分派
   ▼                   ▼                   ▼
┌─────────┐      ┌─────────┐      ┌─────────┐
│产品主管  │◄────►│工程主管  │◄────►│风控主管  │   ← 每部门一位主管
│(1人)    │ A2A  │(1人)    │ A2A  │(1人)    │     主管之间 = A2A 通信
└────┬────┘      └────┬────┘      └─────────┘     （会议室/辩论 = 主管间协作）
     │ 按 skill 动态创建            │
     ▼                            ▼
  subagent 团队               subagent 团队        ← 主管临时召集的子团队
  (临时 · 完成即回收)          (临时)
```

三种通信，各有其位：
| 通信 | 谁↔谁 | 机制 |
|---|---|---|
| **调度** | 秘书 → 主管 | 秘书智能分派任务 |
| **A2A** | 主管 ↔ 主管 | 部门间协商（会议室/辩论的真实底座） |
| **subagent** | 主管 → 子 agent | 主管按 skill 动态建临时团队 |

---

## 二、现状 vs 目标（诚实对照）

| 能力 | 现在 | 目标 |
|---|---|---|
| 理解意图 | ❌ 关键词匹配（`compiler.rs` CONCEPTS 表，`contains`） | ✅ 秘书用大模型真理解 |
| 选谁干 | ❌ 路由按规则选 `selected_agents.first()`，永远 1 人 | ✅ 秘书智能判断派哪些部门 |
| 建公司 | ❌ 只种 skill，不种员工（`opc.rs:1451`） | ✅ 自动创建秘书 + 默认部门主管 |
| 部门主管 | ⚠️ 有 `department`/`supervisor_agent_id` 字段，但无"一部门一主管"约束 | ✅ 每部门一主管，强约束 |
| subagent | ❌ 只有设置开关 `allow_ephemeral_sub_agent`，无实现 | ✅ 主管按 skill spawn 临时子 agent |
| 主管间通信 | ❌ 会议室是后端 `for` 循环轮流调 LLM（`organization.rs:703`） | ✅ A2A 真通信 + 秘书/主管驱动 |

**可复用的好骨架**（不用重写）：
- `AgentEmployee` 已有 `department` / `role` / `supervisor_agent_id` / `system_prompt` / `reputation_vector`（`opc.rs:169`）
- ReAct 治理循环 `AgentSubHarness` 真实可用——subagent 复用它即可
- 三轨治理 `GovernGate`——subagent 同样受治理，安全不破
- 真大模型网关、SSE 流式、持久化——全部复用

---

## 三、分阶段设计

### 阶段 A：秘书 Agent（智能调度入口）— 最高价值，先做

**A1. 建公司自动创建秘书**
- 改 `create_company`（`opc.rs:1434`）：种完 skill 后，自动创建一个 `agent-secretary-01`，department=`FounderOffice`，role=`Secretary`，`supervisor_agent_id=None`（直属创始人）。
- 给它一份"调度官"系统提示词：理解意图、判断部门、产出分派计划。

**A2. 用 LLM 替换 MCL 关键词分类**
- 现在：`compiler.rs` 关键词 → 轨道。
- 改成：秘书 agent 跑一轮"规划"——大模型读创始人原话 + 公司现有员工/部门/skill 清单 → 输出结构化「分派计划」：
  ```json
  {
    "understanding": "创始人想要...",
    "subtasks": [
      {"department": "Product", "goal": "...", "rationale": "..."},
      {"department": "Engineering", "goal": "...", "rationale": "..."}
    ]
  }
  ```
- **治理不破**：轨道仍由服务端 `GovernGate` 权威判定（秘书只"提议"派给谁，不能自己定风险等级）。关键词表降级为 fail-safe 兜底（LLM 不可用时回退）。

**A3. 秘书把子任务变成工单**
- 每个 subtask → 一个 WorkOrder，`selected_agents = [对应部门主管]`。
- 复用现有 execute 链路，无需改治理。

> 阶段 A 单独就能让"理解+分派"变智能，且改动集中在 2 个文件（compiler 调用层 + create_company）。

---

### 阶段 B：部门主管 + subagent 动态团队

**B1. 一部门一主管约束**
- 建公司时按公司类型种默认主管（产品/工程/风控…各 1）。
- `create_company_employee` 加约束：非 Custom 部门已有主管时，新员工必须挂 `supervisor_agent_id`=该主管（即成为 subagent），不能并列第二个主管。

**B2. 主管按 skill 创建 subagent**
- 在 ReAct 循环里加一个新提议类型 `spawn_subagent`（与现有 `call_tool`/`call_executor` 平级）。
- 主管在循环中可提议："我需要一个会 X skill 的子 agent" → 治理门校验（skill 是否存在、风险是否在主管 ceiling 内）→ 创建临时 agent（`lifecycle_status=Ephemeral`，TTL 用现成的 `ephemeral_agent_ttl_minutes` 设置）→ 子 agent 跑自己的 ReAct 循环 → 结果回交主管 → 回收。
- **复用** `AgentSubHarness`——subagent 就是一次受治理的子运行。

**B3. 子 agent 受治理 + 防递归爆炸**
- subagent 的 `risk_ceiling ≤` 主管的；不可再 spawn（或限定深度，用现成的 `max_hops`）。
- 全程落 `worker_events`，可回放。

---

### 阶段 C：主管间 A2A 通信（会议室真实底座）

**C1. A2A 从空壳变真**
- 定义内部 A2A 消息契约（不用 Google 全协议，先做内部版）：主管 A 向主管 B 发"协商请求"，B 回"立场/建议"。
- 复用现有 `A2aProvider` trait，把 `MockA2aAdapter` 换成 `InProcessA2aRouter`——同进程内按 agent_id 路由到对方主管的一次 LLM 调用。这是"真通信"但不跨网络（单机平台合理）。

**C2. 会议室/辩论改由 A2A 驱动**
- 现在：后端 `for` 循环硬编码顺序 + 写死"谁反对"。
- 改成：秘书或发起主管通过 A2A 召集相关部门主管 → 主管们基于各自 skill/职责**自主**发言、质疑、回应（不是写死 critic 必反对，而是风控主管因其职责自然提出风险）。
- 收敛仍用现有 `ResolutionEngine`（真实存在，`coevo-resolution`）做裁决。

> 到阶段 C，"会议室=主管间通信"才真正成立——正是你说的那句。

---

## 四、关键取舍 & 风险

1. **治理红线不能破**：秘书/主管/subagent 再智能，也只能"提议"，轨道与授权永远由服务端 `GovernGate` 判。智能化只发生在"理解/分派/协作"层，不发生在"授权"层。
2. **成本可控**：智能调度=更多 LLM 调用。需复用现有 `max_cost_per_task` / `max_agents_per_task` 上限，防止主管无限 spawn。
3. **LLM 不可用时降级**：秘书规划失败 → 回退到现有关键词路由，不让平台瘫痪。
4. **A2A 先做内部版**：不一上来就实现 Google A2A 全协议（跨网络/鉴权）。先做同进程主管间通信，跑通"智能协作"闭环；将来要对接外部 agent 再升级到标准 A2A。
5. **改动量诚实评估**：A 中等（2 文件 + 秘书提示词 + 测试）；B 大（新提议类型 + subagent 生命周期 + 治理校验）；C 大（A2A 真实现 + 会议室重构）。建议 **A → B → C 分批落地**，每批保持测试全绿。

---

## 五、建议的第一步

**先做阶段 A**：建公司自动有秘书 + 秘书用大模型理解并分派。这一步：
- 立刻让"不智能的 MCL 关键词"变成"真理解"；
- 改动集中、风险低、可独立验证；
- 是 B/C 的地基（秘书是调度中枢）。

确认后我从阶段 A 动手，每步 tsc/cargo 测试全绿再继续。

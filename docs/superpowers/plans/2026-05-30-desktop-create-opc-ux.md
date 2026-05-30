# Desktop Create OPC UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the raw model-key first-run experience with Create OPC onboarding, provider presets, model discovery, stable UUID metadata, and a simplified desktop information architecture.

**Architecture:** Local identity lives in `apps/desktop/src/settings/identity.ts`; provider presets and model role selection live in `apps/desktop/src/settings/modelPresets.ts`; the backend exposes `POST /opc/models/discover` using a new gateway discovery method; UI panels consume these helpers instead of scattering defaults.

**Tech Stack:** React, Vitest, TypeScript, Rust/Axum, reqwest, Tauri desktop.

---

### Task 1: Desktop Identity And Metadata Headers

**Files:**
- Create: `apps/desktop/src/settings/identity.ts`
- Modify: `apps/desktop/src/api/client.ts`
- Test: `apps/desktop/src/__tests__/client.test.ts`

- [ ] **Step 1: Write failing tests**

Add assertions that `headers()` emits a UUIDv4 tenant id, persists it, and exposes helper identity ids for WorkOrders.

- [ ] **Step 2: Verify tests fail**

Run: `npm test -- src/__tests__/client.test.ts`

Expected failure: tenant id is `desktop-tenant`, not UUIDv4.

- [ ] **Step 3: Implement identity helpers**

Create localStorage-backed identity helpers that generate UUIDv4 values once and reuse them.

- [ ] **Step 4: Use identity in headers**

Replace the fixed tenant header with `getTenantId()`.

- [ ] **Step 5: Verify**

Run: `npm test -- src/__tests__/client.test.ts`.

### Task 2: Backend Model Discovery

**Files:**
- Modify: `crates/coevo-models/src/types.rs`
- Modify: `crates/coevo-models/src/gateway.rs`
- Modify: `crates/coevo-models/src/openai.rs`
- Modify: `crates/coevo-models/src/mock.rs`
- Modify: `apps/server/src/handlers/models.rs`
- Modify: `apps/server/src/router.rs`
- Test: `apps/server/src/handlers/models.rs`

- [ ] **Step 1: Write failing Rust tests**

Add a unit test for a candidate discovery request that validates config but does not persist rows. Use Mock provider for a deterministic success path and OpenAI-compatible invalid URL for a deterministic failure path.

- [ ] **Step 2: Verify tests fail**

Run: `cargo test -p coevo-server handlers::models::`

Expected failure: discover handler is missing.

- [ ] **Step 3: Add discovery types and trait method**

Add `DiscoveredModel` and `ModelDiscoveryResponse`, plus `discover_models(&self, config)` to `ModelGateway`.

- [ ] **Step 4: Implement OpenAI-compatible discovery**

Call `{base_url}/models`, parse `data[].id`, and enrich known IDs with curated metadata.

- [ ] **Step 5: Add server route**

Add `POST /opc/models/discover`, accepting the same candidate shape as `/test`.

- [ ] **Step 6: Verify**

Run Rust model handler tests and then `cargo test --workspace`.

### Task 3: Create OPC Onboarding

**Files:**
- Modify: `apps/desktop/src/components/FirstRun.tsx`
- Modify: `apps/desktop/src/settings/i18n.ts`
- Modify: `apps/desktop/src/__tests__/onboarding.test.tsx`

- [ ] **Step 1: Write failing tests**

Assert FirstRun shows "Create your OPC", accepts OPC name and owner, persists identity, then reveals Connect Model.

- [ ] **Step 2: Verify tests fail**

Run: `npm test -- src/__tests__/onboarding.test.tsx`

- [ ] **Step 3: Implement two-step onboarding**

Replace the single configure button with Create OPC fields and a Connect Model step.

- [ ] **Step 4: Verify**

Run onboarding tests.

### Task 4: Provider Presets And Model Settings UX

**Files:**
- Create: `apps/desktop/src/settings/modelPresets.ts`
- Modify: `apps/desktop/src/api/client.ts`
- Modify: `apps/desktop/src/pages/Settings.tsx`
- Test: `apps/desktop/src/__tests__/onboarding.test.tsx`

- [ ] **Step 1: Write failing tests**

Assert base URL and max tokens are hidden by default, provider selection fills the base URL, Save & Test calls discovery, and role selectors use discovered model IDs.

- [ ] **Step 2: Verify tests fail**

Run onboarding tests.

- [ ] **Step 3: Implement presets and discovery client**

Add `discoverModels(config)` API and model selection helpers.

- [ ] **Step 4: Rework ModelProviderPanel**

Show provider, API key, connect button, role dropdowns, and Advanced disclosure.

- [ ] **Step 5: Verify**

Run onboarding and client tests.

### Task 5: Sidebar And Mission Identity

**Files:**
- Modify: `apps/desktop/src/components/Sidebar.tsx`
- Modify: `apps/desktop/src/pages/MissionChat.tsx`
- Test: `apps/desktop/src/__tests__/productSurface.test.tsx`
- Test: `apps/desktop/src/__tests__/missionChat.test.tsx`

- [ ] **Step 1: Write failing tests**

Assert the primary sidebar only shows New Chat, OPC, WorkOrders, Audit, and Settings. Assert MissionChat sends persisted OPC/user ids.

- [ ] **Step 2: Verify tests fail**

Run product surface and MissionChat tests.

- [ ] **Step 3: Implement sidebar and identity usage**

Slim the link list and replace hard-coded WorkOrder identity values.

- [ ] **Step 4: Verify**

Run the updated frontend tests.

### Task 6: Full Verification

**Files:**
- No production files expected unless failures reveal an integration bug.

- [ ] **Step 1: Run frontend tests**

Run with portable Node: `npm test`.

- [ ] **Step 2: Run frontend build**

Run: `npm run build`.

- [ ] **Step 3: Run Rust checks**

Run: `cargo check --workspace` and `cargo test --workspace`.

- [ ] **Step 4: Run package build**

Run: `npm run build:tauri`.

- [ ] **Step 5: Summarize installer artifacts and manual test steps**

Report exact installer paths and the expected first-run click path.

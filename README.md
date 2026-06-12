# coevo

coevo is a local-first desktop app built with Tauri and a Rust server. It organizes work around companies, employees, and governed work orders so you can create a company, add employees yourself, connect an LLM provider, and run tasks with an audit trail on your machine.

**Status:** Alpha / internal RC  
**Stack:** Rust + Tauri + React  
**Backend:** Rust HTTP server with company-scoped `/companies` routes  
**License:** Apache-2.0

---

## Clean Start

This is the recommended manual setup flow for a fresh GitHub clone or download.

1. Clone or download the repository from GitHub.
2. Install the required runtime tools:
   - Rust 1.85+
   - Node.js 20+
   - SQLite 3
   - Python 3 for the synthetic test script
3. Install the desktop dependencies:
   ```bash
   cd apps/desktop
   npm install
   ```
4. Start the Rust server from the repository root:
   ```bash
   cargo run -p coevo-server
   ```
5. Start the desktop development app in another terminal:
   ```bash
   cd apps/desktop
   npm run dev
   ```
6. Open the desktop app and create your company.
7. Add employees manually inside that company.
8. Go to Settings and enter your LLM API key manually.
9. Run a mission and review the resulting work order before execution.

There is no onboarding step that auto-creates employees. A clean start means you make the company, create the employees you want, and supply the model key yourself.

---

## What It Does

coevo is centered on a few real product surfaces:

- Company setup and company-scoped operations
- Manually created employees with company-owned configuration
- Mission intake and governed work orders
- Model provider configuration through the desktop UI
- Local history, timeline, and audit data stored on the user machine

The app is not positioned as a general chatbot, a prompt toy, or an autonomous multi-agent demo. The key idea is that reasoning can happen locally, but actions are governed.

---

## Product Shape

The current shape of the app is:

- Tauri desktop client
- Rust server
- Company-scoped routes under `/companies`
- Work orders as the governed unit of execution
- Employee management inside a company
- Manual LLM provider setup with a pasted API key

Typical flow:

1. Create a company.
2. Create employees manually.
3. Configure the model provider and API key.
4. Draft a mission.
5. Review the resulting work order.
6. Execute only what the system allows.
7. Inspect the timeline and audit records afterward.

---

## Developer Commands

From the repository root:

```bash
cargo check --workspace
cargo test --workspace
cargo run -p coevo-server
```

Desktop app:

```bash
cd apps/desktop
npm install
npm run dev
npm run build
npm run tauri dev
```

Project helpers:

```bash
make check
make test
make run-server
make dev-desktop
```

Synthetic and acceptance checks:

```bash
cargo test -p coevo-server --test acceptance -- --nocapture
cd apps/desktop
npm run test
npm run test:synthetic-opc
```

---

## API Surface

Useful server endpoints include:

- `GET /health`
- `GET /docs`
- `GET /redoc`
- `GET /companies`
- `POST /companies`
- `GET /companies/{id}/profile/company`
- `GET /companies/{id}/employees`
- `POST /companies/{id}/employees`
- `GET /companies/{id}/skills`
- `GET /companies/{id}/work-orders`
- `POST /companies/{id}/work-orders`
- `POST /companies/{id}/work-orders/{work_order_id}/execute`
- `GET /opc/models/config`
- `POST /opc/models/test`

The `/companies` routes are the main way to work with company-scoped data. Work orders are server-governed and should be treated as the source of truth for what ran, when it ran, and what was allowed.

---

## Notes

- Data is intended to stay local to the user's machine.
- The repository includes developer and test fixtures, but those are not the onboarding path.
- The desktop app and the Rust server are meant to be run together during local use and development.

---

## License

Apache-2.0

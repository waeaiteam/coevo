import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import Settings from "../pages/Settings";
import { setLanguage } from "../settings/i18n";

const api = vi.hoisted(() => ({
  discoverModels: vi.fn(),
  testModelConnection: vi.fn(),
  updateModelConfig: vi.fn(),
  listMcpServers: vi.fn(),
  createMcpServer: vi.fn(),
  updateMcpServer: vi.fn(),
  deleteMcpServer: vi.fn(),
  testMcpServer: vi.fn(),
  connectMcpServer: vi.fn(),
  disconnectMcpServer: vi.fn(),
  listMcpServerTools: vi.fn(),
}));

vi.mock("../api/client", () => ({
  getApiBase: () => "http://127.0.0.1:8717",
  discoverModels: api.discoverModels,
  testModelConnection: api.testModelConnection,
  updateModelConfig: api.updateModelConfig,
  listMcpServers: api.listMcpServers,
  createMcpServer: api.createMcpServer,
  updateMcpServer: api.updateMcpServer,
  deleteMcpServer: api.deleteMcpServer,
  testMcpServer: api.testMcpServer,
  connectMcpServer: api.connectMcpServer,
  disconnectMcpServer: api.disconnectMcpServer,
  listMcpServerTools: api.listMcpServerTools,
}));

vi.mock("../api/tauri", () => ({
  getTauriInvoke: () => null,
}));

describe("MCP settings", () => {
  beforeEach(() => {
    setLanguage("en");
    localStorage.clear();
    api.discoverModels.mockResolvedValue({ models: [] });
    api.testModelConnection.mockResolvedValue({
      latency_ms: 12,
      model: "gpt-4o",
      provider_kind: "OpenAICompatible",
    });
    api.updateModelConfig.mockResolvedValue({ ok: true });
    api.listMcpServers.mockResolvedValue([
      {
        id: "srv-files",
        name: "files",
        transport: "stdio",
        command: "node",
        args_json: "[\"server.js\"]",
        env_json: "{}",
        url: null,
        headers_json: "{}",
        enabled: true,
        status: "unknown",
        last_error: null,
        tools_json: "[]",
        created_at: "2026-06-12T00:00:00Z",
        updated_at: "2026-06-12T00:00:00Z",
      },
    ]);
    api.connectMcpServer.mockResolvedValue({
      ok: true,
      server: { id: "srv-files", name: "files" },
    });
    api.disconnectMcpServer.mockResolvedValue({
      ok: true,
    });
    api.listMcpServerTools.mockResolvedValue({
      server_id: "srv-files",
      tools: [
        {
          name: "read_file",
          description: "Read a file from the workspace",
          urn: "urn:mcp:files:read_file",
        },
      ],
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it("shows an MCP Servers section in settings navigation", () => {
    render(
      <MemoryRouter initialEntries={["/settings/general"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>,
    );

    expect(screen.getByRole("link", { name: /MCP Servers/i })).toHaveAttribute(
      "href",
      "/settings/mcp_servers",
    );
  });

  it("loads configured MCP servers and shows tools after connect", async () => {
    render(
      <MemoryRouter initialEntries={["/settings/mcp_servers"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>,
    );

    await waitFor(() => expect(api.listMcpServers).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("files")).toBeInTheDocument();
    expect(screen.getByText("stdio")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Connect/i }));

    await waitFor(() =>
      expect(api.connectMcpServer).toHaveBeenCalledWith("srv-files"),
    );
    await waitFor(() =>
      expect(api.listMcpServerTools).toHaveBeenCalledWith("srv-files"),
    );
    expect(await screen.findByText("read_file")).toBeInTheDocument();
    expect(screen.getByText(/urn:mcp:files:read_file/)).toBeInTheDocument();
  });

  it("shows cached discovered tools for connected servers and allows disconnect", async () => {
    api.listMcpServers.mockReset();
    api.listMcpServers
      .mockResolvedValueOnce([
        {
          id: "srv-files",
          name: "files",
          transport: "stdio",
          command: "node",
          args_json: "[\"server.js\"]",
          env_json: "{}",
          url: null,
          headers_json: "{}",
          enabled: true,
          status: "connected",
          last_error: null,
          tools_json: JSON.stringify([
            {
              name: "read_file",
              description: "Read a file from the workspace",
              urn: "urn:mcp:files:read_file",
            },
          ]),
          created_at: "2026-06-12T00:00:00Z",
          updated_at: "2026-06-12T00:00:00Z",
        },
      ])
      .mockResolvedValueOnce([
        {
          id: "srv-files",
          name: "files",
          transport: "stdio",
          command: "node",
          args_json: "[\"server.js\"]",
          env_json: "{}",
          url: null,
          headers_json: "{}",
          enabled: true,
          status: "unknown",
          last_error: null,
          tools_json: "[]",
          created_at: "2026-06-12T00:00:00Z",
          updated_at: "2026-06-12T00:00:00Z",
        },
      ]);

    render(
      <MemoryRouter initialEntries={["/settings/mcp_servers"]}>
        <Routes>
          <Route path="/settings/*" element={<Settings />} />
        </Routes>
      </MemoryRouter>,
    );

    await waitFor(() => expect(api.listMcpServers).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("read_file")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Disconnect/i })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Disconnect/i }));

    await waitFor(() =>
      expect(api.disconnectMcpServer).toHaveBeenCalledWith("srv-files"),
    );
    await waitFor(() => expect(api.listMcpServers).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole("button", { name: /Connect/i })).toBeInTheDocument();
  });
});

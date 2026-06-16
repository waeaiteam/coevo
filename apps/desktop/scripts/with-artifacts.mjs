import { spawn } from "child_process";
import { mkdirSync, existsSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, "..", "..", "..");
const defaultArtifactRoot = join(repoRoot, ".artifacts");
const artifactRoot = process.env.COEVO_BUILD_ARTIFACT_DIR || defaultArtifactRoot;
const args = process.argv.slice(2);
const localBinDir = join(process.cwd(), "node_modules", ".bin");
const localNodeEntrypoints = {
  tauri: join(process.cwd(), "node_modules", "@tauri-apps", "cli", "tauri.js"),
  tsc: join(process.cwd(), "node_modules", "typescript", "bin", "tsc"),
  vite: join(process.cwd(), "node_modules", "vite", "bin", "vite.js"),
  vitest: join(process.cwd(), "node_modules", "vitest", "vitest.mjs"),
};

if (args.length === 0) {
  console.error("Usage: node scripts/with-artifacts.mjs <command> [...args]");
  process.exit(1);
}

const cargoTargetDir = process.env.CARGO_TARGET_DIR || join(artifactRoot, "cargo-target");
const npmCacheDir = process.env.npm_config_cache || join(artifactRoot, "npm-cache");
const nodeBinDir = dirname(process.execPath);
// Default toolchain is platform-aware so non-Windows hosts (macOS / Linux) do
// not try to invoke a Windows MSVC toolchain that does not exist. Override with
// COEVO_RUST_TOOLCHAIN / RUSTUP_TOOLCHAIN when you need a specific channel.
const defaultToolchainForHost = () => {
  const v = "1.96.0";
  if (process.platform === "win32") return `${v}-x86_64-pc-windows-msvc`;
  if (process.platform === "darwin")
    return process.arch === "arm64"
      ? `${v}-aarch64-apple-darwin`
      : `${v}-x86_64-apple-darwin`;
  return process.arch === "arm64"
    ? `${v}-aarch64-unknown-linux-gnu`
    : `${v}-x86_64-unknown-linux-gnu`;
};
const rustToolchain =
  process.env.COEVO_RUST_TOOLCHAIN ||
  process.env.RUSTUP_TOOLCHAIN ||
  defaultToolchainForHost();

for (const dir of [
  artifactRoot,
  cargoTargetDir,
  npmCacheDir,
  join(artifactRoot, "vite-cache"),
  join(artifactRoot, "desktop-dist"),
  join(artifactRoot, "sidecars"),
]) {
  mkdirSync(dir, { recursive: true });
}

const env = {
  ...process.env,
  COEVO_BUILD_ARTIFACT_DIR: artifactRoot,
  CARGO_TARGET_DIR: cargoTargetDir,
  RUSTUP_TOOLCHAIN: rustToolchain,
  npm_config_cache: npmCacheDir,
  PATH: [nodeBinDir, localBinDir, process.env.PATH || ""].join(process.platform === "win32" ? ";" : ":"),
};

function resolveCommand(command) {
  const lower = command.toLowerCase();
  if (lower === "node" || lower === "node.exe") return process.execPath;
  if (localNodeEntrypoints[lower] && existsSync(localNodeEntrypoints[lower])) {
    args.splice(1, 0, localNodeEntrypoints[lower]);
    return process.execPath;
  }
  if (process.platform === "win32" && !command.includes("\\") && !command.includes("/")) {
    const localCmd = join(localBinDir, `${command}.cmd`);
    if (existsSync(localCmd)) return localCmd;
  }
  return command;
}

const command = resolveCommand(args[0]);
const useShell = process.platform === "win32" && command.toLowerCase().endsWith(".cmd");

const child = spawn(command, args.slice(1), {
  env,
  stdio: "inherit",
  shell: useShell,
});

child.on("exit", (code, signal) => {
  if (signal) {
    console.error(`Command terminated by signal ${signal}`);
    process.exit(1);
  }
  process.exit(code ?? 0);
});

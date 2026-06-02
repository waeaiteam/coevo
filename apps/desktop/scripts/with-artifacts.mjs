import { spawn } from "child_process";
import { mkdirSync, existsSync } from "fs";
import { dirname, join } from "path";

const defaultArtifactRoot = `D:\\${"\u7f16\u8bd1\u4ea7\u7269"}\\coevo`;
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
const rustToolchain = process.env.COEVO_RUST_TOOLCHAIN || process.env.RUSTUP_TOOLCHAIN || "1.96.0-x86_64-pc-windows-msvc";

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

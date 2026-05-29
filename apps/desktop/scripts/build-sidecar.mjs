// build-sidecar.mjs — Build coevo-server and copy to Tauri sidecar directory
import { execSync } from "child_process";
import { existsSync, mkdirSync, copyFileSync, readdirSync, rmSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, "..", "..", "..");
const sidecarDir = join(__dirname, "..", "src-tauri", "binaries");
const serverCrateDir = join(repoRoot, "apps", "server");

console.log("Building coevo-server sidecar...");
console.log("  Repo root:", repoRoot);

// Get target triple
const cargo = process.env.CARGO || "cargo";
const triple = execSync(`${cargo} -vV`, { cwd: repoRoot })
  .toString().split("\n").find(l => l.startsWith("host:"))
  ?.replace("host:", "").trim() || "unknown-target";

console.log("  Target triple:", triple);

// Build release
console.log("  Running: cargo build --release -p coevo-server");
execSync(`${cargo} build --release -p coevo-server`, { 
  cwd: repoRoot, 
  stdio: "inherit" 
});

// Clean old sidecar binaries
mkdirSync(sidecarDir, { recursive: true });
for (const f of readdirSync(sidecarDir)) {
  if (f.startsWith("coevo-server-")) rmSync(join(sidecarDir, f));
}
// Copy to sidecar directory
const src = join(repoRoot, "target", "release", 
  process.platform === "win32" ? "coevo-server.exe" : "coevo-server");
const dest = join(sidecarDir, 
  `coevo-server-${triple}${process.platform === "win32" ? ".exe" : ""}`);

if (existsSync(src)) {
  copyFileSync(src, dest);
  console.log("  Sidecar copied:", dest);
} else {
  console.error("  ERROR: coevo-server binary not found at", src);
  process.exit(1);
}
console.log("Sidecar build complete.");

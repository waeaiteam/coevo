// Build coevo-server and copy it to the external Tauri sidecar directory.
import { execFileSync } from "child_process";
import { existsSync, mkdirSync, copyFileSync, readdirSync, rmSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(__dirname, "..", "..", "..");
const defaultArtifactRoot = `D:\\${"\u7f16\u8bd1\u4ea7\u7269"}\\coevo`;
const artifactRoot = process.env.COEVO_BUILD_ARTIFACT_DIR || defaultArtifactRoot;
const cargoTargetDir = process.env.CARGO_TARGET_DIR || join(artifactRoot, "cargo-target");
const sidecarDir = join(artifactRoot, "sidecars");
process.env.CARGO_TARGET_DIR = cargoTargetDir;
console.log("Building coevo-server sidecar...");
console.log("  Repo root:", repoRoot);
console.log("  Artifact root:", artifactRoot);

const cargo = process.env.CARGO || "cargo";
const defaultToolchain = process.env.COEVO_RUST_TOOLCHAIN || "1.96.0-x86_64-pc-windows-msvc";
const cargoArgsPrefix = cargo === "cargo" && defaultToolchain ? [`+${defaultToolchain}`] : [];
const runCargo = (args, options = {}) => execFileSync(cargo, [...cargoArgsPrefix, ...args], options);
const cargoLabel = [cargo, ...cargoArgsPrefix].join(" ");

const triple = runCargo(["-vV"], { cwd: repoRoot })
  .toString()
  .split("\n")
  .find((line) => line.startsWith("host:"))
  ?.replace("host:", "")
  .trim() || "unknown-target";

console.log("  Target triple:", triple);

const metadata = JSON.parse(runCargo(["metadata", "--format-version", "1", "--no-deps"], { cwd: repoRoot }).toString());
const targetDir = metadata.target_directory || join(repoRoot, "target");
console.log("  Cargo target dir:", targetDir);

// Enable vendored Swagger UI so sidecar builds do not download assets at build time.
const buildArgs = ["build", "--release", "-p", "coevo-server", "--features", "utoipa-swagger-ui/vendored"];
console.log(`  Running: ${cargoLabel} ${buildArgs.join(" ")}`);
runCargo(buildArgs, {
  cwd: repoRoot,
  stdio: "inherit",
});

mkdirSync(sidecarDir, { recursive: true });
for (const fileName of readdirSync(sidecarDir)) {
  if (fileName.startsWith("coevo-server-")) rmSync(join(sidecarDir, fileName));
}

const src = join(targetDir, "release", process.platform === "win32" ? "coevo-server.exe" : "coevo-server");
const dest = join(sidecarDir, `coevo-server-${triple}${process.platform === "win32" ? ".exe" : ""}`);

if (existsSync(src)) {
  copyFileSync(src, dest);
  console.log("  Sidecar copied:", dest);
} else {
  console.error("  ERROR: coevo-server binary not found at", src);
  process.exit(1);
}
console.log("Sidecar build complete.");

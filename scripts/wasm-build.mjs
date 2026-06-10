// Cross-platform spouštěč wasm-pack: npm na Windows spouští skripty přes
// cmd.exe, kde se `$HOME` neexpanduje — proto cestu k ~/.cargo/bin řešíme
// tady v Node. Preferujeme wasm-pack z PATH, fallback na ~/.cargo/bin.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const exe = process.platform === "win32" ? "wasm-pack.exe" : "wasm-pack";
const onPath = spawnSync(exe, ["--version"], { stdio: "ignore" }).status === 0;
const cargoBin = join(homedir(), ".cargo", "bin", exe);
const cmd = onPath ? exe : cargoBin;

if (!onPath && !existsSync(cargoBin)) {
  console.error("wasm-pack nenalezen v PATH ani v ~/.cargo/bin.");
  console.error("Nainstalujte ho: cargo install wasm-pack");
  process.exit(1);
}

const result = spawnSync(
  cmd,
  ["build", "dpi-core", "--target", "bundler", "--out-dir", "../src/wasm"],
  { stdio: "inherit" },
);
process.exit(result.status ?? 1);

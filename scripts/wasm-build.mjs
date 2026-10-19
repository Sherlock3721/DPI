// Cross-platform spouštěč wasm-pack: npm na Windows spouští skripty přes
// cmd.exe, kde se `$HOME` neexpanduje — proto cestu k ~/.cargo/bin řešíme
// tady v Node. Preferujeme wasm-pack z PATH, fallback na ~/.cargo/bin a ~/.local/bin.
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const exe = process.platform === "win32" ? "wasm-pack.exe" : "wasm-pack";
const onPath = spawnSync(exe, ["--version"], { stdio: "ignore" }).status === 0;
const cargoBin = join(homedir(), ".cargo", "bin", exe);
const localBin = join(homedir(), ".local", "bin", exe);

let cmd = onPath ? exe : null;
if (!cmd && existsSync(cargoBin)) cmd = cargoBin;
if (!cmd && existsSync(localBin)) cmd = localBin;

if (!cmd) {
  console.error("wasm-pack nebyl nalezen v PATH, ~/.cargo/bin ani ~/.local/bin.");
  console.error("Nainstalujte ho prosím:");
  console.error("  - Arch Linux: sudo pacman -S wasm-pack");
  console.error("  - Ostatní OS / Cargo: cargo install wasm-pack");
  process.exit(1);
}

const result = spawnSync(
  cmd,
  ["build", "dpi-core", "--target", "bundler", "--out-dir", "../src/wasm"],
  { stdio: "inherit" },
);
process.exit(result.status ?? 1);

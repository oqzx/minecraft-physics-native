// What a validation run actually executed (B017, docs/validation-records.md): the code and whether it
// was clean, the addon by content, the generated data, the toolchain and the environment overrides
// that change what runs. A result quoted without one of these describes some other build.
//   node scripts/validation-record.cjs              prints the record
//   node scripts/validation-record.cjs -- <cmd...>  runs the command and adds its exit code and output hash
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { engineDigest } = require("./parity-contract.cjs");

const root = path.resolve(__dirname, "..");
const OVERRIDES = ["CARGO_TARGET_DIR", "NODE_PATH", "REPLAY_RECORDINGS", "NAPI_RS_NATIVE_LIBRARY_PATH", "MC_SOURCE_ROOT"];
const sha256 = (data) => crypto.createHash("sha256").update(data).digest("hex");
const run = (command, ...args) => spawnSync(command, args, { cwd: root, encoding: "utf8", shell: process.platform === "win32" }).stdout?.trim() ?? "";

/** The record of this checkout and addon. */
function validationRecord(env = process.env) {
  const artifacts = path.join(root, "native", "artifacts");
  const generated = path.join(root, "generated");
  const changed = run("git", "status", "--porcelain", "--", "native/src", "native/data", "src", "scripts", "generated");
  return {
    commit: run("git", "rev-parse", "HEAD"),
    dirty: changed === "" ? "clean" : sha256(changed),
    engine: engineDigest(root),
    addon: Object.fromEntries(fs.readdirSync(artifacts).filter((name) => name.endsWith(".node")).map((name) => [name, sha256(fs.readFileSync(path.join(artifacts, name)))])),
    generated: Object.fromEntries(fs.readdirSync(generated, { recursive: true }).map((name) => name.replaceAll("\\", "/")).sort()
      .filter((name) => fs.statSync(path.join(generated, name)).isFile())
      .map((name) => [name, sha256(fs.readFileSync(path.join(generated, name), "utf8").replace(/\r\n/g, "\n"))])),
    toolchain: { node: process.version, rustc: run("rustc", "--version"), cargo: run("cargo", "--version") },
    overrides: Object.fromEntries(OVERRIDES.filter((name) => env[name] !== undefined).map((name) => [name, env[name]])),
  };
}

if (require.main === module) {
  const separator = process.argv.indexOf("--");
  const record = validationRecord();
  if (separator > 0) {
    const [command, ...args] = process.argv.slice(separator + 1);
    const result = spawnSync(command, args, { cwd: root, encoding: "utf8", shell: process.platform === "win32", maxBuffer: 1 << 28 });
    record.command = [command, ...args].join(" ");
    record.exit = result.status;
    record.output = sha256(`${result.stdout}${result.stderr}`);
  }
  console.log(JSON.stringify(record, null, 2));
  process.exit(record.exit ?? 0);
}

module.exports = { validationRecord };

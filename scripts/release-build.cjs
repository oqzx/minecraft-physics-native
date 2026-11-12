// Builds one target's package from the checkout, in the order docs/release-build.md gives:
//   node scripts/release-build.cjs --out <dir> [--target <triple>...] [--data full|cached|tracked] [--keep-binaries]
//
// --data full regenerates everything from the exact sources (scripts/generate-all.cjs) and fails on a
// missing or wrong-release input; cached keeps a release whose sources are absent, and says so;
// tracked regenerates nothing. The record says which (`dataGeneration`).
//
// The build runs with no path of this host in the binaries (--remap-path-prefix), and the package is
// assembled in <out>/stage/<name> from the files of the build alone, so a Windows addon a checkout
// tracks cannot reach a Linux tarball. <out> holds the tarball and its report; nothing is published.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { TARGETS, hostTarget, provenanceFile } = require("./native-artifacts.cjs");
const { fileHash, packageProblems, recordProvenance, shippedOutputs } = require("./build-provenance.cjs");
const { releaseDataProblems } = require("./verify-release-data.cjs");
const { inputProblems } = require("./bootstrap-inputs.cjs");
const { patchLoader, patchNoThreadsLoader } = require("./patch-wasi-loader.cjs");

const root = path.resolve(__dirname, "..");
const windows = process.platform === "win32";

function option(name, fallback) {
  const index = process.argv.indexOf(`--${name}`);
  return index < 0 ? fallback : process.argv[index + 1];
}
const options = (name) => process.argv.flatMap((value, index) => (value === `--${name}` ? [process.argv[index + 1]] : []));

function run(command, args, environment = {}) {
  console.log(`> ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit", shell: windows && command !== process.execPath, env: { ...process.env, ...environment } });
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed with ${result.status}`);
}

function capture(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: "utf8", shell: windows });
  if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr}`);
  return result.stdout.trim();
}

/** The pinned toolchain, or a refusal naming what is installed instead. */
function checkToolchain() {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
  const channel = /channel = "([^"]+)"/.exec(fs.readFileSync(path.join(root, "rust-toolchain.toml"), "utf8"))?.[1];
  const rustc = capture("rustc", ["--version"]);
  assert.ok(rustc.startsWith(`rustc ${channel} `), `rust-toolchain.toml pins ${channel}, this host has ${rustc}`);
  const major = /^v(\d+)\./.exec(process.version)?.[1];
  assert.equal(`>=${major}`, manifest.engines.node, `package.json engines pins node ${manifest.engines.node}, this host has ${process.version}`);
}

function regenerate(mode) {
  if (mode === "tracked") return;
  if (mode === "full") {
    const problems = inputProblems();
    if (problems.length > 0) throw new Error(`The exact sources are not all at hand:\n  ${problems.join("\n  ")}`);
  }
  const args = ["scripts/generate-all.cjs", ...(mode === "cached" ? ["--cached"] : [])];
  run(process.execPath, args);
}

/**
 * What keeps a build the same wherever and whenever it runs: no host path in the binary, and on MSVC
 * no link time. link.exe otherwise stamps the PE header and debug directory and a random PDB id,
 * which was the whole difference between two clean builds (24 bytes).
 */
function rustFlags(target) {
  const cargoHome = process.env.CARGO_HOME ?? path.join(require("node:os").homedir(), ".cargo");
  return [`--remap-path-prefix=${root}=/mcpn`, `--remap-path-prefix=${cargoHome}=/cargo`,
    ...(target.endsWith("-msvc") ? ["-Clink-arg=/Brepro"] : [])].join(" ");
}

function build(target) {
  const hostBuild = target === hostTarget();
  run("npx", ["napi", "build", "--manifest-path", "native/Cargo.toml", "--release", "--platform",
    ...(hostBuild ? [] : ["--target", target]), "--output-dir", "native/artifacts", "--js", "native.js", "--dts", "native.d.ts"],
  { RUSTFLAGS: rustFlags(target) });
  if (target === "wasm32-wasip1-threads") patchLoader();
  if (target === "wasm32-wasip1") patchNoThreadsLoader();
}

/** The package of `targets` alone, in `stage`: nothing else of the checkout's artifacts comes along. */
function stagePackage(stage, targets) {
  fs.rmSync(stage, { recursive: true, force: true });
  fs.mkdirSync(path.join(stage, "native", "artifacts"), { recursive: true });
  fs.cpSync(path.join(root, "dist"), path.join(stage, "dist"), { recursive: true });
  for (const name of ["package.json", "README.md", "THIRD_PARTY_NOTICES"]) fs.copyFileSync(path.join(root, name), path.join(stage, name));
  const artifacts = (name) => path.join(root, "native", "artifacts", name);
  const copy = (name) => fs.copyFileSync(artifacts(name), path.join(stage, "native", "artifacts", name));
  for (const name of ["native.js", "native.d.ts"]) copy(name);
  for (const target of targets) {
    copy(provenanceFile(target));
    for (const name of shippedOutputs(target)) copy(name);
  }
}

function main() {
  const out = path.resolve(option("out", path.join(root, "release")));
  const data = option("data", "full");
  assert.ok(["full", "cached", "tracked"].includes(data), `--data ${data}`);
  const targets = options("target");
  if (targets.length === 0) targets.push(hostTarget());
  for (const target of targets) assert.ok(TARGETS[target], `${target} is no declared target`);

  checkToolchain();
  regenerate(data);
  const problems = releaseDataProblems(root);
  if (problems.length > 0) throw new Error(`The release data is not sound:\n  ${problems.join("\n  ")}`);
  // --keep-binaries packages what is built already: a binary of other sources is still refused below
  if (!process.argv.includes("--keep-binaries")) for (const target of targets) build(target);
  run("npx", ["tsc"]);
  run(process.execPath, ["scripts/verify-native-package.cjs"]);
  const records = targets.map((target) => recordProvenance(root, target, { dataGeneration: data }));
  if (records.some((record) => record.dirty)) console.warn("the checkout has uncommitted changes: the record says dirty");

  fs.mkdirSync(out, { recursive: true });
  const name = targets.map((target) => TARGETS[target].artifact).join("+");
  const stage = path.join(out, "stage", name);
  stagePackage(stage, targets);
  const staged = packageProblems(stage, { targets });
  if (staged.length > 0) throw new Error(`The staged package is not one build:\n  ${staged.join("\n  ")}`);
  const packed = JSON.parse(spawnSync(windows ? "npm.cmd" : "npm", ["pack", "--json", "--ignore-scripts", "--pack-destination", out],
    { cwd: stage, encoding: "utf8", shell: windows, maxBuffer: 2 ** 28 }).stdout)[0];
  const tarball = path.join(out, `${path.basename(packed.filename, ".tgz")}-${name}.tgz`);
  fs.renameSync(path.join(out, packed.filename), tarball);

  const unpacked = path.join(out, "unpacked", name);
  fs.rmSync(unpacked, { recursive: true, force: true });
  fs.mkdirSync(unpacked, { recursive: true });
  // relative names: GNU tar reads a drive letter as a remote host
  const extract = spawnSync("tar", ["-xzf", path.basename(tarball), "-C", path.relative(out, unpacked).replaceAll("\\", "/")], { cwd: out, stdio: "inherit" });
  assert.equal(extract.status, 0, "the tarball does not unpack");
  const inside = packageProblems(path.join(unpacked, "package"), { targets });
  if (inside.length > 0) throw new Error(`The tarball is not the build:\n  ${inside.join("\n  ")}`);

  const report = { tarball: path.basename(tarball), sha256: fileHash(tarball), bytes: fs.statSync(tarball).size, entries: packed.entryCount, targets, records };
  fs.writeFileSync(path.join(out, `${name}.report.json`), `${JSON.stringify(report, null, 2)}\n`);
  console.log(`${tarball}\n${report.sha256}`);
}

if (require.main === module) main();

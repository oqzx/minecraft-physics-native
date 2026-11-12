// The WebAssembly builds as a consumer gets them: a tarball with both builds is installed from the
// registry into an empty project, then run in Node (forced selection and the direct loaders) and in a
// real browser (a vite bundle of the installed package, served with and without cross-origin
// isolation). Every tick is compared, to the bit, with the native addon of the same tarball.
//   node scripts/wasm-check.cjs <tarball> --work <empty dir> --out <dir> [--chrome <path>]
// Without a browser on the host only the Node half runs, and the report says so.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const { spawn, spawnSync } = require("node:child_process");
const { fileHash, packageProblems } = require("./build-provenance.cjs");

const windows = process.platform === "win32";
const PACKAGE = "minecraft-physics-native";
const VITE = "8.3.6";
const THREADS = "wasm32-wasip1-threads";
const NO_THREADS = "wasm32-wasip1";
const CHROME = ["C:/Program Files/Google/Chrome/Application/chrome.exe", "/usr/bin/google-chrome", "/usr/bin/chromium"];

// One scene for every backend: a run across the releases, the data tables and a batch call. Written
// to run unchanged as CommonJS text and in a page.
const SCENARIO = `
function scenario(binding) {
    const world = new binding.NativeWorld();
    for (let x = -8; x < 8; x += 1) for (let z = -8; z < 8; z += 1) world.setFullBlock(x, 63, z, 1);
    const out = {};
    for (const version of ['1.8.8', '1.12.2', '1.13.2', '1.16.5', '1.20.4', '1.21.11', '26.1', '26.3']) {
        let state = {
            position: { x: 0.5, y: 66.25, z: 0.5 }, velocity: { x: 0.02, y: -0.1, z: 0.05 }, yaw: 0.7, pitch: 0.1,
            halfWidth: 0.3, height: 1.8, onGround: false, collidedHorizontally: false, collidedVertically: false,
        };
        const trace = [];
        for (let tick = 0; tick < 60; tick += 1) {
            state = binding.simulateVersionedTick(version, world, state, { input: { x: 0, y: 0, z: 1 }, sprinting: tick > 20, isPlayer: true });
            trace.push([state.position.x, state.position.y, state.position.z, state.velocity.x, state.velocity.y, state.velocity.z, state.onGround]);
        }
        out[version] = {
            trace,
            supported: binding.isSupportedVersion(version),
            stone: binding.generatedBlockStateCount(version, 'stone'),
            water: binding.generatedFluidStateCount(version, 'water'),
        };
    }
    return out;
}
`;

const NODE_RUN = (entry) => `
${SCENARIO}
const binding = require(${JSON.stringify(entry)});
process.stdout.write(JSON.stringify({ identity: binding.buildIdentity(), result: scenario(binding) }));
`;

const LOADED_RUN = `
${SCENARIO}
const physics = require('${PACKAGE}');
const binding = physics.LoadNativePhysics();
process.stdout.write(JSON.stringify({ identity: physics.nativeBackend(binding), result: scenario(binding) }));
`;

const PAGE = `<!doctype html><meta charset="utf-8"><title>wasm check</title><script type="module" src="/main.js"></script>`;

const PAGE_MAIN = `${SCENARIO}
const mode = new URLSearchParams(location.search).get('mode');
const artifacts = '${PACKAGE}/native/artifacts';
async function load() {
    if (mode === 'wasip1') return import('${PACKAGE}/native/artifacts/index.wasip1-browser.js');
    if (mode === 'threads') return import('${PACKAGE}/native/artifacts/index.wasi-browser.js');
    const { instantiate } = await import('${PACKAGE}/native/artifacts/index.wasip1-deferred.js');
    return instantiate(await WebAssembly.compileStreaming(fetch('/deferred.wasm')));
}
const report = body => fetch('/result', { method: 'POST', body: JSON.stringify(body) });
window.addEventListener('unhandledrejection', event => report({ ok: false, error: String(event.reason) }));
window.addEventListener('error', event => report({ ok: false, error: String(event.message) }));
try {
    const binding = await load();
    await report({ ok: true, isolated: self.crossOriginIsolated, identity: binding.buildIdentity(), result: scenario(binding) });
} catch (error) {
    await report({ ok: false, isolated: self.crossOriginIsolated, error: String(error && error.stack || error) });
}
`;

function option(name, required = true) {
  const index = process.argv.indexOf(`--${name}`);
  if (index < 0) {
    assert.ok(!required, `--${name} is required`);
    return null;
  }
  return path.resolve(process.argv[index + 1]);
}

function cleanEnvironment(cache) {
  const environment = { ...process.env, npm_config_cache: cache, npm_config_audit: "false", npm_config_fund: "false", npm_config_update_notifier: "false" };
  for (const name of Object.keys(environment)) {
    if (name === "NODE_PATH" || name.startsWith("NAPI_RS_") || /^(?:npm_(?:package|lifecycle|execpath|node_execpath|command)|PNPM_)/i.test(name)) delete environment[name];
  }
  return environment;
}

const MIME = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".map": "application/json" };

/** Serves `root`; `block` turns files into 404s, `isolate` sends the headers threads need. */
function serve(root, { isolate, block }, onResult) {
  const server = http.createServer((request, response) => {
    const url = new URL(request.url, "http://localhost");
    if (url.pathname === "/result") {
      let body = "";
      request.on("data", (chunk) => { body += chunk; });
      request.on("end", () => {
        response.end("ok");
        onResult(JSON.parse(body));
      });
      return;
    }
    const name = url.pathname === "/" ? "index.html" : url.pathname.slice(1);
    const file = path.join(root, name);
    if ((block && block.test(name)) || !file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      response.statusCode = 404;
      response.end("not found");
      return;
    }
    if (isolate) {
      response.setHeader("Cross-Origin-Opener-Policy", "same-origin");
      response.setHeader("Cross-Origin-Embedder-Policy", "require-corp");
    }
    response.setHeader("Content-Type", MIME[path.extname(name)] ?? "application/octet-stream");
    fs.createReadStream(file).pipe(response);
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

/** One page load in its own headless Chrome, stopped by its own PID; times out to `{ timedOut: true }`. */
async function browserCase(chrome, site, work, { name, mode, isolate = false, block = null, seconds = 90 }) {
  const profile = path.join(work, "chrome", name);
  fs.mkdirSync(profile, { recursive: true });
  let finish;
  const finished = new Promise((resolve) => { finish = resolve; });
  const server = await serve(site, { isolate, block }, finish);
  const url = `http://127.0.0.1:${server.address().port}/?mode=${mode}`;
  const browser = spawn(chrome, ["--headless=new", "--disable-gpu", "--no-first-run", "--no-default-browser-check", "--disable-extensions",
    `--user-data-dir=${profile}`, "--remote-debugging-port=0", url], { stdio: "ignore" });
  const timer = setTimeout(() => finish({ timedOut: true }), seconds * 1000);
  const outcome = await finished;
  clearTimeout(timer);
  if (windows) spawnSync("taskkill", ["/PID", String(browser.pid), "/T", "/F"], { stdio: "ignore" });
  else browser.kill("SIGKILL");
  server.closeAllConnections();
  server.close();
  return outcome;
}

async function main() {
  const tarball = path.resolve(process.argv[2]);
  const work = option("work");
  const out = option("out");
  const chrome = process.argv.includes("--chrome") ? process.argv[process.argv.indexOf("--chrome") + 1] : CHROME.find((file) => fs.existsSync(file));
  fs.mkdirSync(work, { recursive: true });
  fs.mkdirSync(out, { recursive: true });
  assert.deepEqual(fs.readdirSync(work), [], `${work} is not empty`);
  const environment = cleanEnvironment(path.join(work, "npm-cache"));
  const run = (name, command, args, overrides = {}) => {
    const result = spawnSync(command, args, { cwd: work, encoding: "utf8", shell: windows && !command.endsWith("node.exe"), env: { ...environment, ...overrides }, maxBuffer: 2 ** 30 });
    fs.writeFileSync(path.join(out, `${name}.log`), `exit ${result.status}\n--- stdout\n${(result.stdout ?? "").slice(0, 4000)}\n--- stderr\n${result.stderr ?? ""}\n`);
    return result;
  };
  const npm = windows ? "npm.cmd" : "npm";
  fs.writeFileSync(path.join(work, "package.json"), JSON.stringify({
    name: "consumer-wasm", private: true, type: "commonjs",
    dependencies: { [PACKAGE]: `file:${tarball.replaceAll("\\", "/")}`, vite: VITE },
  }, null, 2));
  const install = run("wasm-install", npm, ["install", "--no-package-lock"]);
  assert.equal(install.status, 0, install.stderr);
  const installed = path.join(work, "node_modules", PACKAGE);
  assert.deepEqual(packageProblems(installed), [], "the installed package is not the recorded build");

  const report = { tarball: path.basename(tarball), sha256: fileHash(tarball), host: { platform: process.platform, node: process.version }, node: {}, browser: {}, diagnostics: {} };
  const artifacts = path.join(installed, "native", "artifacts");
  const script = (name, text) => {
    const file = path.join(work, name);
    fs.writeFileSync(file, text);
    return file;
  };
  const exec = (name, text, overrides = {}) => {
    const result = run(name, process.execPath, [script(`${name}.cjs`, text)], overrides);
    assert.equal(result.status, 0, `${name}: ${result.stderr}`);
    return JSON.parse(result.stdout);
  };

  // the native addon of the same tarball is the reference
  const reference = exec("native", LOADED_RUN);
  assert.equal(reference.identity.backend, "native");
  report.reference = reference.identity;
  const compare = (name, outcome, target) => {
    assert.equal(outcome.identity.target, target, `${name} answered as ${outcome.identity.target}`);
    assert.equal(outcome.identity.sourceHash, reference.identity.sourceHash, `${name} is another build`);
    assert.equal(JSON.stringify(outcome.result), JSON.stringify(reference.result), `${name} differs from the native addon`);
    return { target: outcome.identity.target, ticks: Object.values(outcome.result).reduce((sum, entry) => sum + entry.trace.length, 0), equalToNative: true };
  };

  for (const [name, flavor, target] of [["flavor-threads", "wasm32-wasi", THREADS], ["flavor-no-threads", "wasm32-wasip1", NO_THREADS]]) {
    report.node[name] = compare(name, exec(name, LOADED_RUN, { NAPI_RS_WASI_FLAVOR: flavor }), target);
  }
  // NAPI_RS_FORCE_WASI takes the first flavor the loader lists
  report.node["force-error"] = compare("force-error", exec("force-error", LOADED_RUN, { NAPI_RS_FORCE_WASI: "error" }), THREADS);
  report.node["direct-threads"] = compare("direct-threads", exec("direct-threads", NODE_RUN(`${PACKAGE}/native/artifacts/index.wasi.cjs`)), THREADS);
  report.node["direct-no-threads"] = compare("direct-no-threads", exec("direct-no-threads", NODE_RUN(`${PACKAGE}/native/artifacts/index.wasip1.cjs`)), NO_THREADS);

  const without = (file, name, action) => {
    const location = path.join(artifacts, file);
    const backup = `${location}.held`;
    fs.renameSync(location, backup);
    const result = action();
    fs.renameSync(backup, location);
    assert.notEqual(result.status, 0, `${name} succeeded without ${file}`);
    report.diagnostics[name] = `${result.stderr}`.split("\n").filter((line) => /Error|missing|not found|Cannot/i.test(line)).slice(0, 3).join(" | ").slice(0, 300);
    assert.notEqual(report.diagnostics[name], "", `${name} gave no diagnostic`);
  };
  const loadedScript = script("loaded.cjs", LOADED_RUN);
  without("index.wasm32-wasip1.wasm", "node-wasm-missing", () => run("node-wasm-missing", process.execPath, [loadedScript], { NAPI_RS_WASI_FLAVOR: "wasm32-wasip1" }));
  without("index.wasip1.cjs", "node-loader-missing", () => run("node-loader-missing", process.execPath, [loadedScript], { NAPI_RS_WASI_FLAVOR: "wasm32-wasip1" }));
  // the engine starts no thread, so ticks do not need the worker: what it does without one is measured,
  // and the record is what notices the file is gone
  const worker = path.join(artifacts, "wasi-worker.mjs");
  fs.renameSync(worker, `${worker}.held`);
  const workerless = run("node-worker-missing", process.execPath, [loadedScript], { NAPI_RS_WASI_FLAVOR: "wasm32-wasi" });
  assert.match(packageProblems(installed).join("\n"), /wasi-worker\.mjs of .* is missing/);
  fs.renameSync(`${worker}.held`, worker);
  report.diagnostics["node-worker-missing"] = `exit ${workerless.status}; the provenance check reports the file missing`;
  assert.deepEqual(packageProblems(installed), []);

  if (!chrome) {
    report.browser = { skipped: "no browser on this host" };
  } else {
    report.browser.chrome = chrome;
    fs.writeFileSync(path.join(work, "index.html"), PAGE);
    fs.writeFileSync(path.join(work, "main.js"), PAGE_MAIN);
    fs.writeFileSync(path.join(work, "vite.config.mjs"), `export default { build: { target: 'esnext', outDir: 'site', assetsInlineLimit: 0 }, worker: { format: 'es' }, optimizeDeps: { exclude: ['${PACKAGE}'] } };\n`);
    const bundle = run("vite-build", process.execPath, [path.join(work, "node_modules", "vite", "bin", "vite.js"), "build"]);
    assert.equal(bundle.status, 0, `${bundle.stdout}\n${bundle.stderr}`);
    const site = path.join(work, "site");
    fs.copyFileSync(path.join(artifacts, "index.wasm32-wasip1.wasm"), path.join(site, "deferred.wasm"));
    const cases = [
      { name: "no-threads", mode: "wasip1", target: NO_THREADS },
      { name: "deferred", mode: "deferred", target: NO_THREADS },
      { name: "threads-isolated", mode: "threads", isolate: true, target: THREADS },
    ];
    for (const entry of cases) {
      const outcome = await browserCase(chrome, site, work, entry);
      assert.equal(outcome.ok, true, `${entry.name}: ${JSON.stringify(outcome).slice(0, 600)}`);
      report.browser[entry.name] = { ...compare(entry.name, outcome, entry.target), crossOriginIsolated: outcome.isolated };
    }
    const failing = [
      { name: "threads-without-isolation", mode: "threads" },
      { name: "wasm-missing", mode: "wasip1", block: /\.wasm$/ },
      { name: "threads-wasm-missing", mode: "threads", isolate: true, block: /\.wasm$/ },
      { name: "threads-worker-missing", mode: "threads", isolate: true, block: /worker/i, seconds: 30 },
    ];
    for (const entry of failing) {
      const outcome = await browserCase(chrome, site, work, entry);
      assert.notEqual(outcome.ok, true, `${entry.name} loaded`);
      assert.notEqual(outcome.timedOut, true, `${entry.name} gave no diagnostic`);
      report.diagnostics[`browser-${entry.name}`] = String(outcome.error).slice(0, 300);
    }
  }
  fs.writeFileSync(path.join(out, "wasm.report.json"), `${JSON.stringify(report, null, 2)}\n`);
  console.log(JSON.stringify({ node: report.node, browser: report.browser, diagnostics: report.diagnostics }, null, 2));
}

if (require.main === module) main().catch((error) => {
  console.error(error);
  process.exit(1);
});

// Two corrections to the browser loaders napi-rs writes. A loader napi-rs has changed is an error
// here, not a silent skip.
//
// The threaded loader (after the wasm32-wasip1-threads build) waits for its workers without listening
// to them, so a worker file that fails to load leaves the page hanging with nothing said. This makes
// the failure say what happened (it reaches window.onerror and the console; the load itself still
// does not settle, the wait being emnapi's).
//
// The no-threads loader (after the wasm32-wasip1 build) instantiates synchronously, and browsers
// refuse to compile or instantiate a module above 8 MB that way on the main thread. The module passed
// that size with the block shape tables. It now instantiates asynchronously, at the top-level await
// the threaded and deferred loaders already use.
const fs = require("node:fs");
const path = require("node:path");

const ANCHOR = "      __wasiWorkers.add(worker)\n";
const MESSAGE = "The threaded WebAssembly worker wasi-worker-browser.mjs failed: check that it is served beside the module, or use index.wasip1-browser.js";
const LISTENER = `      worker.addEventListener('error', () => {
        throw new Error(${JSON.stringify(MESSAGE)})
      })
`;

function patchedLoader(text) {
  const normal = text.replaceAll("\r\n", "\n");
  if (normal.includes(LISTENER)) return normal;
  if (!normal.includes(ANCHOR)) throw new Error("index.wasi-browser.js no longer starts its workers the way scripts/patch-wasi-loader.cjs knows");
  return normal.replace(ANCHOR, () => `${ANCHOR}${LISTENER}`);
}

const SYNC_IMPORT = "  instantiateNapiModuleSync as __emnapiInstantiateNapiModuleSync,\n";
const ASYNC_IMPORT = "  instantiateNapiModule as __emnapiInstantiateNapiModule,\n";
const SYNC_CALL = "} = __emnapiInstantiateNapiModuleSync(__wasmFile, {";
const ASYNC_CALL = "} = await __emnapiInstantiateNapiModule(__wasmFile, {";

function patchedNoThreadsLoader(text) {
  const normal = text.replaceAll("\r\n", "\n");
  if (normal.includes(ASYNC_CALL) && normal.includes(ASYNC_IMPORT)) return normal;
  if (!normal.includes(SYNC_CALL) || !normal.includes(SYNC_IMPORT)) throw new Error("index.wasip1-browser.js no longer instantiates its module the way scripts/patch-wasi-loader.cjs knows");
  return normal.replace(SYNC_IMPORT, () => ASYNC_IMPORT).replace(SYNC_CALL, () => ASYNC_CALL);
}

const artifactsOf = (artifacts) => artifacts ?? path.resolve(__dirname, "..", "native", "artifacts");

function patchFile(file, patched) {
  fs.writeFileSync(file, patched(fs.readFileSync(file, "utf8")));
}

function patchLoader(artifacts) {
  patchFile(path.join(artifactsOf(artifacts), "index.wasi-browser.js"), patchedLoader);
}

function patchNoThreadsLoader(artifacts) {
  patchFile(path.join(artifactsOf(artifacts), "index.wasip1-browser.js"), patchedNoThreadsLoader);
}

if (require.main === module) {
  if (process.argv.includes("--no-threads")) patchNoThreadsLoader();
  else patchLoader();
}

module.exports = { patchedLoader, patchedNoThreadsLoader, patchLoader, patchNoThreadsLoader };

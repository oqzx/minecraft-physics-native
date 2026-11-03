"use strict";

// What napi-rs writes into native/artifacts (package.json "napi", the native:build* scripts): the
// loader and its declarations, a .node addon per platform, and the WebAssembly builds with their
// loaders, workers and declarations (wasm32-wasip1-threads: index.wasi.*, wasm32-wasip1:
// index.wasip1.*; the debug .wasm beside the release one, browser.js re-exporting the wasm
// package). verify-native-package.cjs refuses anything else, and package.json "files" ships
// exactly these.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const loaderOutputs = ["native.js", "native.d.ts"];
const wasmOutputs = [
    "browser.js",
    "index.wasm32-wasi.wasm", "index.wasm32-wasi.debug.wasm", "index.wasi.cjs", "index.wasi.d.cts",
    "index.wasi-browser.js", "wasi-worker.mjs", "wasi-worker-browser.mjs",
    "index.wasm32-wasip1.wasm", "index.wasm32-wasip1.debug.wasm", "index.wasip1.cjs", "index.wasip1.d.cts",
    "index.wasip1-browser.js", "index.wasip1-deferred.js", "index.wasip1-deferred.d.ts",
];
const addon = /^index\.[a-z0-9]+(?:-[a-z0-9]+)+\.node$/;

const isNapiOutput = (name) => loaderOutputs.includes(name) || wasmOutputs.includes(name) || addon.test(name);

/// Every file of the directory is a napi-rs output, and every addon and CommonJS wasm loader is
/// one native.js loads (a retired or misnamed build left there is refused).
function verifyArtifactDirectory(artifacts) {
    const loader = path.join(artifacts, "native.js");
    assert.equal(fs.existsSync(loader), true, "native.js was not generated");
    const loaderText = fs.readFileSync(loader, "utf8");
    for (const entry of fs.readdirSync(artifacts)) {
        assert.ok(isNapiOutput(entry), `native/artifacts/${entry} is no napi-rs output`);
        if (entry.endsWith(".node") || entry.endsWith(".cjs")) {
            assert.ok(loaderText.includes(`'./${entry}'`), `native/artifacts/${entry} is not loaded by native.js`);
        }
    }
}

module.exports = { loaderOutputs, wasmOutputs, isNapiOutput, verifyArtifactDirectory };

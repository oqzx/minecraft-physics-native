"use strict";

// What napi-rs writes into native/artifacts (package.json "napi", the native:build* scripts): the
// loader and its declarations, a .node addon per platform, and the WebAssembly builds with their
// loaders, workers and declarations (wasm32-wasip1-threads: index.wasi.*, wasm32-wasip1:
// index.wasip1.*; the debug .wasm beside the release one, browser.js re-exporting the wasm
// package). verify-native-package.cjs refuses anything else, and package.json "files" ships
// exactly these, with the provenance of each build (scripts/build-provenance.cjs).
//
// TARGETS are the declared targets, the same as package.json napi.targets. The loader has branches
// for many more platforms: a branch is not a supported product target.

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const wasiOutputs = ["index.wasm32-wasi.wasm", "index.wasm32-wasi.debug.wasm", "index.wasi.cjs", "index.wasi.d.cts",
    "index.wasi-browser.js", "wasi-worker.mjs", "wasi-worker-browser.mjs"];
const wasip1Outputs = ["index.wasm32-wasip1.wasm", "index.wasm32-wasip1.debug.wasm", "index.wasip1.cjs", "index.wasip1.d.cts",
    "index.wasip1-browser.js", "index.wasip1-deferred.js", "index.wasip1-deferred.d.ts"];

const TARGETS = {
    "x86_64-pc-windows-msvc": { artifact: "win32-x64-msvc", outputs: ["index.win32-x64-msvc.node"], platform: "win32", arch: "x64", format: "pe", machine: 0x8664 },
    "x86_64-unknown-linux-gnu": { artifact: "linux-x64-gnu", outputs: ["index.linux-x64-gnu.node"], platform: "linux", arch: "x64", format: "elf", machine: 62 },
    "wasm32-wasip1-threads": { artifact: "wasm32-wasi", outputs: wasiOutputs, wasm: true },
    "wasm32-wasip1": { artifact: "wasm32-wasip1", outputs: wasip1Outputs, wasm: true },
};

const loaderOutputs = ["native.js", "native.d.ts"];
const wasmOutputs = ["browser.js", ...wasiOutputs, ...wasip1Outputs];
const addon = /^index\.[a-z0-9]+(?:-[a-z0-9]+)+\.node$/;
const provenance = /^provenance\.[a-z0-9-]+\.json$/;

const isNapiOutput = (name) => loaderOutputs.includes(name) || wasmOutputs.includes(name) || addon.test(name) || provenance.test(name);
const provenanceFile = (target) => `provenance.${TARGETS[target].artifact}.json`;

/** The declared native target of this host, null when it has none. */
function hostTarget(platform = process.platform, arch = process.arch) {
    return Object.keys(TARGETS).find((target) => TARGETS[target].platform === platform && TARGETS[target].arch === arch) ?? null;
}

/** What an addon's first bytes say it was built for: `{ format, machine }`, null when they are no binary. */
function binaryFormat(bytes) {
    if (bytes.length >= 20 && bytes.readUInt32BE(0) === 0x7f454c46) return { format: "elf", machine: bytes.readUInt16LE(18) };
    if (bytes.length >= 64 && bytes.toString("latin1", 0, 2) === "MZ") {
        const header = bytes.readUInt32LE(60);
        if (header + 6 <= bytes.length && bytes.toString("latin1", header, header + 4) === "PE\0\0") {
            return { format: "pe", machine: bytes.readUInt16LE(header + 4) };
        }
    }
    if (bytes.length >= 4 && [0xfeedfacf, 0xcffaedfe, 0xcafebabe].includes(bytes.readUInt32BE(0))) return { format: "macho", machine: 0 };
    return null;
}

/** Why an addon file is not what its name says, null when it is. The name must be a declared target's. */
function addonProblem(name, bytes) {
    const target = Object.values(TARGETS).find((entry) => !entry.wasm && entry.outputs.includes(name));
    if (!target) return `${name} is the addon of no declared target (${Object.keys(TARGETS).join(", ")})`;
    const found = binaryFormat(bytes);
    if (!found) return `${name} is no native binary`;
    if (found.format !== target.format || found.machine !== target.machine) {
        return `${name} holds a ${found.format} binary for machine ${found.machine}, not the ${target.format} of ${target.artifact}`;
    }
    return null;
}

/// Every file of the directory is a napi-rs output, every addon is what its name says, and every
/// addon and CommonJS wasm loader is one native.js loads (a retired or misnamed build left there is
/// refused).
function verifyArtifactDirectory(artifacts) {
    const loader = path.join(artifacts, "native.js");
    assert.equal(fs.existsSync(loader), true, "native.js was not generated");
    const loaderText = fs.readFileSync(loader, "utf8");
    for (const entry of fs.readdirSync(artifacts)) {
        assert.ok(isNapiOutput(entry), `native/artifacts/${entry} is no napi-rs output`);
        if (entry.endsWith(".node") || entry.endsWith(".cjs")) {
            assert.ok(loaderText.includes(`'./${entry}'`), `native/artifacts/${entry} is not loaded by native.js`);
        }
        if (entry.endsWith(".node")) {
            const problem = addonProblem(entry, fs.readFileSync(path.join(artifacts, entry)));
            assert.equal(problem, null, problem ?? undefined);
        }
    }
}

module.exports = { TARGETS, loaderOutputs, wasmOutputs, isNapiOutput, provenanceFile, hostTarget, binaryFormat, addonProblem, verifyArtifactDirectory };

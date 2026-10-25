


## Status - WIP

This repository is a development project for a native, version-aware Minecraft physics engine for Mineflayer.

 The active task list is in [todo.md](todo.md).

## WebAssembly

The engine also builds to WebAssembly through napi-rs's WASI targets and runs on the emnapi runtime
(`@napi-rs/wasm-runtime`), with the same JS API as the native addon and the same results, bit for bit.

Build (needs the Rust targets: `rustup target add wasm32-wasip1-threads wasm32-wasip1`):

| script | target | output in `native/artifacts` |
| --- | --- | --- |
| `pnpm native:build:wasm` | `wasm32-wasip1-threads` | `index.wasm32-wasi.wasm`, `index.wasi.cjs` (Node), `index.wasi-browser.js` (browser), `wasi-worker*.mjs` |
| `pnpm native:build:wasip1` | `wasm32-wasip1` (no threads) | `index.wasm32-wasip1.wasm`, `index.wasip1.cjs` (Node), `index.wasip1-browser.js`, `index.wasip1-deferred.js` (browser) |

The builds call `cargo` and `rustc` from `PATH`: they must be rustup's, with the targets installed (a standalone Rust
install has no WASI standard library; put the rustup toolchain's `bin` first on `PATH`, or set `CARGO` and `RUSTC`).

### Node

`native/artifacts/native.js` (what `LoadNativePhysics` and the package load) prefers the native addon and falls back
to a WebAssembly build when no addon loads:

- `NAPI_RS_FORCE_WASI=true` prefers WebAssembly (native stays the fallback), `NAPI_RS_FORCE_WASI=error` requires it;
- `NAPI_RS_WASI_FLAVOR=wasm32-wasi` (threads) or `wasm32-wasip1` (no threads) loads exactly that build;
- or require a loader directly: `require("minecraft-physics-native/native/artifacts/index.wasip1.cjs")`.

### Browser

Import the ES module loader through a bundler (it resolves `@napi-rs/wasm-runtime` and the `.wasm` file through
`new URL(..., import.meta.url)`, and uses top-level await):

```js
import { NativeWorld, simulateVersionedTick } from "minecraft-physics-native/native/artifacts/index.wasip1-browser.js";
```

- The no-threads build (`index.wasip1-browser.js`) needs nothing more: a plain `WebAssembly.Memory`, no workers.
  `index.wasip1-deferred.js` exports `instantiate(module)` for pages that compile the `.wasm` themselves.
- The threads build (`index.wasi-browser.js`) uses shared memory, so the page must be cross-origin isolated
  (`Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` or `credentialless`,
  for `SharedArrayBuffer`), and it starts a pool of module Web Workers (`wasi-worker-browser.mjs`, 4 plus
  `navigator.hardwareConcurrency`), each instantiating the module. Without isolation it fails at load.

The engine uses no threads, async tasks or thread-safe functions, so the no-threads build loses nothing: prefer it in
browsers. The threads build is napi-rs's default WASI target and only pays off for crates that use threads.

Both builds compute in WebAssembly about as fast as natively (a 400-tick batch takes 6.4 to 6.7 us per tick, natively
6.0), but every call goes through emnapi's JS implementation of N-API, so one `simulateVersionedTick` call costs about
145 us (no threads) or 185 us (threads) against 31 us natively: batch ticks (`simulateVersionedTicks`) where you can.
Each build reserves 128 MiB of memory up front (`napi.wasm.initialMemory`; the module itself needs 65 MiB, 61 of them
the 64 MB stack napi-rs links).

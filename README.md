


## Status - WIP

This repository is a development project for a native, version-aware Minecraft physics engine for Mineflayer.
No release is certified to match vanilla: a registered release runs, which says nothing of parity.

 The active task list is in [todo.md](todo.md).

## Documents

| Question | Page |
| --- | --- |
| What has each release shown? | [docs/release-status.md](docs/release-status.md) |
| What does it not do, or not show? | [docs/limitations.md](docs/limitations.md) |
| How does a tick get to a release's rules? | [docs/engine-architecture.md](docs/engine-architecture.md) |
| How must a world be kept in step? | [docs/synchronization.md](docs/synchronization.md) |
| What does each field mean? | [docs/state-contract.md](docs/state-contract.md), [docs/field-maps.md](docs/field-maps.md), [docs/field-catalogue.md](docs/field-catalogue.md) |
| What would a release hold? | [docs/release-notes.md](docs/release-notes.md), unreleased |

## Releases

The engine has one module per release it supports (`native/src/versioned/v-*.rs`, listed in `SUPPORTED_VERSIONS`):
1.8.8, 1.9.4, 1.10.2, 1.11.2, 1.12.2, 1.13.2, 1.14.4 and every release from 1.15 through 26.3. The TypeScript side
mirrors it: `SUPPORTED_VERSIONS` and `isSupportedVersion` (`src/versions.ts`), one engine class per release
(`NativePhysics_1_20_2`, ...) and `NATIVE_PHYSICS_ENGINES`, whose type refuses a release without its class, and
`createNativePhysics(registry)`, which picks the class of the registry's release (`src/mineflayer/versions.ts`).
`test/version-registry.test.cjs` keeps the lists equal and checks them against Mojang's release list
(`generated/mojang-releases.json`, `pnpm generate:mojang-releases`).

Adapter engines require the supplied registry to declare the same exact release
as the selected engine. Unverified overrides and inconsistent dependency aliases
are rejected before native construction. See [the registry contract and migration
notes](docs/registry-release-contract.md), including the six releases whose tables are their
same-protocol release's.

`generated/physics-breakpoints.json` (`pnpm report:physics-breakpoints`) lists what physics reads that differs
between adjacent releases: the tracked vanilla methods, the client mappings of the classes physics reads, the
block registry and states, the block and fluid tags, minecraft-data's entities, effects and attributes, and the
movement packets.

## Installing

```
npm install minecraft-physics-native
```

Node 24 or later. The package holds its engine compiled; nothing is built at install and no Minecraft source is needed.

| what you use | what you need | |
| --- | --- | --- |
| `minecraft-physics-native/dist/native`, the low-level binding | nothing more | |
| `minecraft-physics-native`, the adapter | `mineflayer@^4.37` and `@nxg-org/mineflayer-physics-util@^1.9.10` | optional peers |

The peers are optional because nothing is loaded from them at run time: the adapter takes your bot and your
`EPhysicsCtx`. Their types are public, though. TypeScript reports `Cannot find module 'mineflayer'` on the main entry
until you install them; install both, or import from `dist/native`.

The adapter takes the data of the registry you give it, so a release needs a `minecraft-data` that has it: the published
package has no 26.2 or 26.3 yet, which the engines do. The local `minecraft-data` (`scripts/source-pins.cjs`
`MINECRAFT_DATA`) has an exact entry for each of the fifty releases.

### Targets

Packages are built per target (`docs/release-build.md`), and each says what it was built from in
`native/artifacts/provenance.<target>.json`.

| target | status |
| --- | --- |
| `x86_64-pc-windows-msvc` | built, installed and run from the tarball |
| `x86_64-unknown-linux-gnu` (glibc) | built, installed and run from the tarball |
| `wasm32-wasip1`, `wasm32-wasip1-threads` | built, run in Node and in Chrome from the installed tarball |
| macOS, Linux musl, Linux arm64 | not built, not supported. The generated loader has branches for them; a branch is no support |

On Windows x64 and Linux x64 glibc, `LoadNativePhysics` refuses to run when the addon did not load, rather than fall back
to WebAssembly unasked. `nativeBackend(binding)` says which build answered, from what the binary says of itself.

### Direct

```ts
import { LoadNativePhysics, nativeBackend } from 'minecraft-physics-native/dist/native';

const binding = LoadNativePhysics();
const world = new binding.NativeWorld();
world.setFullBlock(0, 0, 0, 1);
const state = binding.simulateVersionedTick('1.21.11', world, {
    position: { x: 0.5, y: 1.5, z: 0.5 }, velocity: { x: 0, y: -0.1, z: 0 },
    halfWidth: 0.3, height: 1.8, onGround: false, collidedHorizontally: false, collidedVertically: false,
}, {});
console.log(JSON.stringify({ backend: nativeBackend(binding), y: state.position.y }));
```

### With a bot, through `EPhysicsCtx.FROM_BOT`

```ts
import { createBotPhysics } from 'minecraft-physics-native';
import type { Bot } from 'mineflayer';
import { EPhysicsCtx } from '@nxg-org/mineflayer-physics-util/dist/physics/settings/entityPhysicsCtx';

export function predict(bot: Bot): void {
    const setup = createBotPhysics(bot);
    const ctx = setup.context(EPhysicsCtx);
    setup.physics.simulate(ctx, bot.world);
    ctx.state.apply(bot);
    setup.dispose();
}
```

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
to a WebAssembly build when no addon loads, and `LoadNativePhysics` takes that fallback only where this platform has no
declared addon or you asked for it:

- `NAPI_RS_FORCE_WASI=true` prefers WebAssembly (native stays the fallback, with a process warning), `NAPI_RS_FORCE_WASI=error` requires it;
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

Every call goes through emnapi's JS implementation of N-API, so batch ticks (`simulateVersionedTicks`) where you can.
These timings are dated: they predate the growth of the module, and a run on the shared machine here is too noisy to replace
them. A 400-tick batch took 6.4 to 6.7 us per tick, natively 6.0. One `simulateVersionedTick` call took about 145 us (no
threads) or 185 us (threads), against 31 us natively.

Each build reserves 128 MiB of memory up front (`napi.wasm.initialMemory`). A build of 2026-11-06, not of this commit, measured
8.4 MiB on disk (no threads) and 8.7 MiB (threads), and declares a minimum of 68 MiB of memory, most of it the 64 MB stack
napi-rs links.

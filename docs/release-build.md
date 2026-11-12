# Building a release

How a package is rebuilt from a clean checkout, what is pinned, and what reproducibility is claimed. Nothing here publishes
anything.

## Targets

`package.json` `napi.targets` and `TARGETS` in `scripts/native-artifacts.cjs` are the declared targets, and a test keeps
them equal:

| target | package | built on |
| --- | --- | --- |
| `x86_64-pc-windows-msvc` | `index.win32-x64-msvc.node` | Windows x64 |
| `x86_64-unknown-linux-gnu` | `index.linux-x64-gnu.node` | Linux x64, glibc |
| `wasm32-wasip1-threads` | `index.wasm32-wasi.wasm`, loaders, workers | any, with the wasm target added |
| `wasm32-wasip1` | `index.wasm32-wasip1.wasm`, loaders | any, with the wasm target added |

Not declared: macOS (no host has built or run it), Linux musl, Linux arm64. The generated loader has branches for them, and
an addon under one of those names is refused.

A tarball holds the targets named on its build, so it is one host's distribution; there is no combined tarball yet, since
no host builds both native targets.

## Pinned inputs

| input | pin |
| --- | --- |
| Rust | `rust-toolchain.toml`; `release-build.cjs` refuses another `rustc` |
| Rust dependencies | `native/Cargo.lock` |
| Node | major in `package.json` `engines`, checked by `release-build.cjs`; the exact version is recorded |
| pnpm and JavaScript dependencies | `package.json` `packageManager`, `pnpm-lock.yaml` |
| modern release sources | `PINNED_SOURCES`, `PINNED_ROOT_SOURCES` (26.2, 26.3) and `PINNED_CLIENTS` in `scripts/source-pins.cjs`, each held to the release it embeds |
| every source tree read | its hash in `scripts/source-tree-pins.json` (`scripts/tree-pins.cjs`): version.json, `net/` and `data/minecraft/tags/`, line endings aside |
| legacy release sources | each release's `NAMED_JARS` jar through Vineflower 1.12.0 (`MC_LEGACY_SOURCES`), held to the release it embeds and its tree hash |
| minecraft-data | `MINECRAFT_DATA` in `scripts/source-pins.cjs` (commit, and the commit of its data submodule) |
| version jsons | the sha1 of each in `generated/mojang-releases.json`, Mojang's list when it was made |
| client jars and libraries | the sha1 each pinned version json gives, and for the clients `scripts/source-pins.cjs` as well |
| Mojang-named jars before 26.1 | `NAMED_JARS` in `scripts/source-pins.cjs`, by sha256 |
| JDK | 25 (Oracle 25.0.3 on Windows); the oracles compile with `--release 21` |

The Rust pin is exact. With rustup, a checkout selects that toolchain by itself, and a host whose toolchain has another name
sets `RUSTUP_TOOLCHAIN`. A Rust install outside rustup ignores the file and is checked all the same; it has no wasm standard
library.

## Locations

Every external input is named by a variable. The defaults are a development layout; a clean host sets them all.
Manifests record a source as `<pin>:<path>` or `mc-source:<path>`, never by where these happen to be, so the layout does not reach
generated output. All of `MODERN_TREES` are required: generators prefer such a tree when it exists, so one that is merely absent
would change the output with no error.

| variable | holds |
| --- | --- |
| `MC_SOURCE_ROOT` | `26.2/client`, `26.3/client` |
| `MC_SOURCE_CACHE` | what `scripts/materialize-sources.cjs` unpacks and decompiles |
| `MC_LEGACY_SOURCES` | the six legacy releases' jars as `materialize-sources.cjs` decompiles them (`<release>/net`, and 1.13.2's `data/`) |
| `MC_EXTRACTED_DATA_REPO`, `MC_DATA_REPO` | the extracted data repository, with `origin/client<release>` |
| `MC_CLIENT_JAR_ROOT`, `MC_DECOMPILER_JAR` | client jars and Vineflower, to materialize a pinned client and to run `generate-block-shapes` and `generate-entity-data` |
| `MC_LIBRARY_ROOT`, `MC_GRADLE_MODULES` | the libraries those jars run with, found by coordinates and sha1 |
| `MC_MINECRAFT_DATA` | the minecraft-data checkout, when it is not `../minecraft-data` (a worktree) |
| `MC_MINECRAFT_DATA_SOURCE`, `MC_MINECRAFT_DATA_SUBMODULE_SOURCE` | where `bootstrap-inputs.cjs` clones minecraft-data and its data from, when not GitHub |
| `JAVA_HOME`, `JAVA_<major>_HOME` | the JDK the oracles run on; `javac` on the PATH compiles them |

## From a clean checkout

```sh
git clone <repository> minecraft-physics-native && cd minecraft-physics-native
corepack enable                               # or a pnpm of the pinned version
node scripts/fetch-jar-inputs.cjs --derived <dir|url>   # version jsons, clients, libraries from Mojang; named jars from <dir|url>
node scripts/bootstrap-inputs.cjs             # clones ../minecraft-data at its pin, checks every source and jar (--cached leaves out the jars)
pnpm install --frozen-lockfile
node scripts/materialize-sources.cjs          # MC_SOURCE_CACHE and MC_SOURCE_ROOT from pinned commits and jars, each tree to its pin
node scripts/release-build.cjs --out ../out                       # data, addon, types, provenance, tarball
node scripts/consumer-check.cjs ../out/<tarball> --work <empty> --out ../out/consumer
```

`release-build.cjs` takes `--target <triple>` (repeated, the host's native target by default) and `--data`:

- `full` regenerates everything. Before it starts, `bootstrap-inputs.cjs` must find every release's exact source; a missing or
  wrong-release input stops the run and names the release.
- `cached` keeps the tables of a release whose sources or jars are absent, and the record says `cached`. Everything whose input
  is at hand is still regenerated, so on a host with the sources but not the jars the block shapes and entity data stay as committed.
- `tracked` regenerates nothing and takes the committed tables, for a target that does not need them made twice. The record
  says `tracked`; `generatedHash` and `sourceHash` still say which tables they were.

The WebAssembly targets need `rustup target add wasm32-wasip1 wasm32-wasip1-threads` on the pinned toolchain.
`scripts/wasm-check.cjs <tarball>` needs a tarball holding the host's addon and both wasm builds.

## Source inputs

`scripts/materialize-sources.cjs` makes every Mojang-named source tree from a pin, and then holds each tree it made or
found to its hash in `scripts/source-tree-pins.json`; `bootstrap-inputs.cjs` checks the same hashes before a full build.

| trees | from | step |
| --- | --- | --- |
| `cache:` 15 releases of `PINNED_CLIENTS` | the client jar by sha1 (26.1.2), or unimined's Mojang-named jar by sha256 | Vineflower 1.12.0, by sha1, `--only=net/minecraft`; the client's data pack and version.json copied |
| `cache:` 27 releases of `PINNED_SOURCES` | a commit of the extracted data repository (`MC_EXTRACTED_DATA_REPO`, github.com/extremeheat/extracted_minecraft_data) | `git archive` of `client/net`; the data pack from the client jar by sha1 |
| `mc-source:26.2`, `mc-source:26.3` | the commits of `PINNED_ROOT_SOURCES` | `git archive` of `client/` into `MC_SOURCE_ROOT` |
| `legacy:` the six legacy releases | the Mojang-named jar by its `NAMED_JARS` sha256, remapped by `legacy-mappings/<release>.tiny` | Vineflower 1.12.0 into `MC_LEGACY_SOURCES`, sources only; `generate-movement-sites` and `generate-primitive-map` read them |

A materialized tree that is not its pin (a decompiler that no longer reproduces it, a tree left from another input) fails
the step and names the tree. In an empty directory the step made all 52 materialized trees equal to their pins, which were
taken from the trees the committed data was generated from; spot checks found 1.15, 1.19.2, 26.1.2 and 1.12.2 equal to those
trees file for file, and the 26.2 and 26.3 commits equal to `E:/pr/mc-source` line endings aside.

### The engine's citations

Every generated table, and every citation in the engine's comments, is read from the Mojang-named trees above. A comment
cites a registered release, its file and line ("1.8.8 Block.java:367-369", "1.20 BlockBehaviour.java:470-472"), and
`generate-movement-sites` looks each one up in that release's exact tree to prove the literal beside it (`treesOf`). Until
2026-11-09 the comments cited yarn-named trees, some of releases that are none of ours (1.8.9, 1.14); they were rewritten
then, each line moved to the Mojang-named file that holds the same code, and the 30 yarn trees left the inputs.

## Jar inputs

`generate-block-shapes`, `generate-entity-data` and `generate-legacy-shapes` run each release's own code, so a full
generation needs, for the 44 modern and 6 legacy releases, the version json, the client, every library and the
Mojang-named jar. `scripts/fetch-jar-inputs.cjs` puts them in `MC_CLIENT_JAR_ROOT` (unimined's layout) and
`MC_LIBRARY_ROOT` (the launcher's), each checked before it is kept:

- the version json from piston-meta (`/v1/packages/<sha1>/<release>.json`) at the sha1 of `generated/mojang-releases.json`;
- the client and the libraries at the sha1 that json gives, from a `--derived` copy, Gradle's cache or Mojang;
- the Mojang-named jar of a release before 26.1 only from `--derived` (a folder or URL in unimined's layout, such as
  `~/.gradle/caches/unimined/net/minecraft/minecraft` of a host that built the recorder), at its `NAMED_JARS` sha256.
  Mojang does not publish it, and unimined stamps the time of the remap into every zip entry, so it cannot be made
  again to the byte: it is a pinned input. A legacy release's is the one remapped by `legacy-mappings/<release>.tiny`,
  whose sha1 names its folder; no other remap of it is taken.

`--check` fetches nothing and fails on anything absent or off its pin. The library rules are read as on Windows on
every host, so all hosts run the same classpath, and a generator refuses a classpath missing a library
(`completeClasspath`): which jars a host happens to hold never reaches a table. `bootstrap-inputs.cjs` also refuses a
version json other than the pinned one.

## Reproducing a generation

Validated on one host, Windows; Linux and macOS are deferred (see below). `scripts/generated-hashes.cjs` is the
inventory: the sha256 of every file under `generated/` and `native/data/`, raw bytes, sorted by path.

```sh
node scripts/fetch-jar-inputs.cjs --derived <dir|url>   # once, into MC_CLIENT_JAR_ROOT and MC_LIBRARY_ROOT
node scripts/bootstrap-inputs.cjs --check
GENERATOR_WORKERS=8 node scripts/generate-all.cjs && node scripts/generated-hashes.cjs --out a.txt
# a second checkout at another path, every location set explicitly
GENERATOR_WORKERS=1 node scripts/generate-all.cjs && node scripts/generated-hashes.cjs --out b.txt
node scripts/generated-hashes.cjs --compare a.txt b.txt
```

- A run publishes all of its stages or none: a failed stage, an interrupt the process sees, and (through the journal
  it keeps in the temporary folder) a run killed outright are all undone, the last by the next run or `--recover`.
- A run ends with `generation: fresh`, or `generation: cached` followed by the inputs not at hand whose tables it kept.
  `release-build.cjs` records the mode as `dataGeneration`.

The results are in `docs/backlog-progress.md`.

## Linux and macOS (deferred)

Neither is validated, and neither is a declared generation host. What a Linux attempt in WSL Ubuntu 26.04 showed before
it was stopped: the jar inputs fetch and verify there (`fetch-jar-inputs.cjs`, every Mojang file by sha1, the named jars
by sha256 from `--derived`); `bootstrap-inputs.cjs` passes with every source read from other roots than Windows'; a
legacy named jar's folder had to be pinned to the mapping's CRLF bytes, since unimined names it by the hash of the file
as checked out; the default library folder is per OS; minecraft-data's pinned commits (d0658d9, data e0313e69) are not
on GitHub, so a host off this machine needs `MC_MINECRAFT_DATA_SOURCE` and `MC_MINECRAFT_DATA_SUBMODULE_SOURCE`; and
reading 5 GB of source trees through `/mnt/<drive>` is too slow (one stage ran an hour at 3% CPU), so the trees belong
on the Linux file system. No Linux generation finished, so no inventory was compared. macOS has had no host at all; its
default library folder is `~/Library/Application Support/minecraft/libraries` and everything else is a variable.

## What a package says of itself

The addon and the wasm modules carry the hash of the sources they were compiled from and their Rust target (`native/build.rs`,
`buildIdentity()`). `native/artifacts/provenance.<artifact>.json` records for one target the commit, toolchain, compiler
settings, input hashes and the sha256 of every file shipped with it. It holds no time and no path.

`scripts/build-provenance.cjs` `packageProblems` holds a package to its records, in the staged copy, in the unpacked tarball
and in the installed package: each file has its hash, each binary carries the recorded sources and target (read from its
bytes, so this works on a host that cannot load it), all records are of one build, and nothing is unrecorded.
`scripts/verify-native-package.cjs` does what a working tree allows: the host's addon was built from the sources now here, the
declarations and `NativePhysicsBinding` agree with its exports, and the release inventory holds.

## Generated files

`GENERATED` in `scripts/generated-inventory.cjs` lists every generated file with the step that makes it and what that step
reads. `pass: all` files are stages of `scripts/generate-all.cjs`, whose `--check` regenerates and compares them. The rest
cannot be made from the pinned sources alone: `network` reads Mojang's live list and `oracle` reads jars and dumps of running
clients. Those stay as committed in a clean build, and their pinned identity is the commit. Each such entry says what pins
its input (`pin`) and what refuses a changed or wrong-release input before its output is taken (`detect`), or "none";
`node scripts/generated-inventory.cjs` fails on one that does not say.

`generate-mojang-releases.cjs`, the network step, is the root of the jar pins: a registered release's version json sha1
is what `fetch-jar-inputs.cjs` takes its client and libraries by. A list that gives a registered release another sha1, or
drops it, fails before anything is written and names the release; `--accept-changed <release>` takes it after a review.
On 2026-11-09 Mojang's list gave new version json sha1s for the 26 registered releases from 1.19.3 to 26.3, so the step
now refuses to run: the pins stay those of the commit.

## Reproducibility policy

`scripts/compare-builds.cjs <a.report.json> <b.report.json>` applies this to two runs:

- **Inputs** must be equal on every host, line endings aside: `sourceHash`, `generatedHash`, `adapterHash`, the locks and pins.
  Two runs that disagree here are not the same release.
- **Two builds of one target** must give equal bytes for every shipped file, the binary included. The build strips the
  checkout and Cargo home from the binaries with `--remap-path-prefix`, so the directory does not count, and links with
  `/Brepro` on MSVC, whose linker would otherwise stamp the time and a random PDB id.
- **Different targets** share their inputs and differ in their binaries by construction. Each has its own record. Identical
  results between them are shown by running them (`wasm-check.cjs` compares ticks to the bit), never by their hashes.


// One record per built target, native/artifacts/provenance.<artifact>.json, tying the binary, its
// loader, declarations and the compiled adapter to one build: the sources and data it came from,
// the toolchain, and the hash of every file. It holds no time and no path, so two builds of the same
// inputs write the same record when their binaries are equal.
//
// A binary carries the hash of its sources (native/build.rs). That is read back from its bytes, so
// a binary from another build is refused even on a host that cannot load it.
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");
const { TARGETS, addonProblem, isNapiOutput, provenanceFile } = require("./native-artifacts.cjs");

const SCHEMA = 1;
const SOURCE_INPUTS = ["Cargo.toml", "Cargo.lock", "build.rs", "data", "src"];

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const fileHash = (file) => sha256(fs.readFileSync(file));
// a checkout's line endings are the host's, not part of what it holds
const withoutCarriageReturns = (bytes) => Buffer.from(bytes.toString("latin1").replaceAll("\r\n", "\n"), "latin1");
const textHash = (file) => sha256(withoutCarriageReturns(fs.readFileSync(file)));

function filesUnder(directory, name) {
    return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
        const child = `${name}/${entry.name}`;
        return entry.isDirectory() ? filesUnder(path.join(directory, entry.name), child) : [child];
    });
}

const compareBytes = (left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right));

/** The hash of named files of a root, line endings aside: the listing `<sha256>  <name>\n`, sorted by name. */
function listingHash(root, names) {
    const listing = [...names].sort(compareBytes).map((name) => `${textHash(path.join(root, name))}  ${name}\n`).join("");
    return sha256(Buffer.from(listing));
}

/** What native/build.rs compiles into the binary, for the sources of `nativeRoot`. */
function sourceHash(nativeRoot) {
    const names = SOURCE_INPUTS.flatMap((input) => {
        const file = path.join(nativeRoot, input);
        return fs.statSync(file).isDirectory() ? filesUnder(file, input) : [input];
    });
    return listingHash(nativeRoot, names);
}

function version(command, args = ["--version"]) {
    const result = spawnSync(command, args, { encoding: "utf8", shell: process.platform === "win32" });
    if (result.status !== 0) throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr}`);
    return result.stdout.trim().split("\n")[0].trim();
}

function toolchain(root) {
    const installed = (name) => JSON.parse(fs.readFileSync(path.join(root, "node_modules", ...name.split("/"), "package.json"), "utf8")).version;
    return {
        rustc: version("rustc"),
        cargo: version("cargo"),
        node: process.version,
        napiCli: installed("@napi-rs/cli"),
        typescript: installed("typescript"),
    };
}

/** The pinned inputs of a build, each by a hash a host's line endings do not change. */
function buildInputs(root) {
    return {
        sourceHash: sourceHash(path.join(root, "native")),
        generatedHash: listingHash(root, filesUnder(path.join(root, "generated"), "generated")),
        adapterHash: listingHash(root, filesUnder(path.join(root, "src"), "src")),
        cargoLock: textHash(path.join(root, "native", "Cargo.lock")),
        pnpmLock: textHash(path.join(root, "pnpm-lock.yaml")),
        rustToolchain: textHash(path.join(root, "rust-toolchain.toml")),
        sourcePins: textHash(path.join(root, "scripts", "source-pins.cjs")),
    };
}

function commitOf(root) {
    const head = spawnSync("git", ["-C", root, "rev-parse", "HEAD"], { encoding: "utf8" });
    if (head.status !== 0) throw new Error(`${root} is no git checkout: a build has to name its commit`);
    const status = spawnSync("git", ["-C", root, "status", "--porcelain", "--", ".", ":(exclude)native/artifacts", ":(exclude).gitignore"], { encoding: "utf8" });
    return { commit: head.stdout.trim(), dirty: status.stdout.trim().length > 0 };
}

/** What a package ships of a target. The loaders prefer a debug .wasm when one is there, so it stays out. */
const shippedOutputs = (target) => TARGETS[target].outputs.filter((name) => !name.endsWith(".debug.wasm"));

/** The files of a target in a package: what the addon or wasm build wrote, and what every target shares. */
function runtimeFiles(root, target) {
    return [
        ...filesUnder(path.join(root, "dist"), "dist"),
        "native/artifacts/native.js",
        "native/artifacts/native.d.ts",
        ...shippedOutputs(target).map((name) => `native/artifacts/${name}`),
    ].filter((name, index, names) => names.indexOf(name) === index).sort(compareBytes);
}

/** The binaries of a target that carry its identity. */
const identityBinaries = (target) => shippedOutputs(target).filter((name) => name.endsWith(".node") || name.endsWith(".wasm"));

/** Why a binary is not of this build, null when it is. */
function binaryProblem(root, target, name, identity) {
    const bytes = fs.readFileSync(path.join(root, "native", "artifacts", name));
    if (name.endsWith(".node")) {
        const problem = addonProblem(name, bytes);
        if (problem) return problem;
    }
    if (!bytes.includes(identity)) return `${name} was not built from sources ${identity.slice(0, 12)} (mixed build?)`;
    if (!bytes.includes(target)) return `${name} is not a build for ${target}`;
    return null;
}

function compilerSettings(root) {
    const manifest = fs.readFileSync(path.join(root, "native", "Cargo.toml"), "utf8").replaceAll("\r\n", "\n");
    const profile = /\[profile\.release\]\n([\s\S]*?)(?:\n\n|\n\[|$)/.exec(manifest)?.[1] ?? "";
    return { profile: "release", settings: profile.split("\n").filter(Boolean), remapped: true };
}

/** Writes the record of a target just built in `root`, and returns it. Refuses a mixed build. */
function recordProvenance(root, target, overrides = {}) {
    const inputs = buildInputs(root);
    for (const name of identityBinaries(target)) {
        const problem = binaryProblem(root, target, name, inputs.sourceHash);
        if (problem) throw new Error(problem);
    }
    const files = {};
    for (const name of runtimeFiles(root, target)) {
        const file = path.join(root, name);
        if (!fs.existsSync(file)) throw new Error(`${name} of ${target} was not built`);
        files[name] = fileHash(file);
    }
    const record = {
        schema: SCHEMA,
        target,
        artifact: TARGETS[target].artifact,
        ...commitOf(root),
        toolchain: toolchain(root),
        compiler: compilerSettings(root),
        inputs,
        files,
        ...overrides,
    };
    const output = path.join(root, "native", "artifacts", provenanceFile(target));
    fs.writeFileSync(output, `${JSON.stringify(record, null, 2)}\n`);
    return record;
}

function recordsOf(root) {
    const artifacts = path.join(root, "native", "artifacts");
    return fs.readdirSync(artifacts)
        .filter((name) => /^provenance\..+\.json$/.test(name))
        .map((name) => ({ name, record: JSON.parse(fs.readFileSync(path.join(artifacts, name), "utf8")) }));
}

/**
 * Whether a package tree (a staged copy, an unpacked tarball, an installed package) is exactly what
 * its records say: each named file with its hash, each binary of the one build, nothing unrecorded.
 */
function packageProblems(root, { targets = null } = {}) {
    const problems = [];
    const found = recordsOf(root);
    if (found.length === 0) return ["the package holds no provenance record"];
    const recorded = new Map();
    const first = found[0].record;
    for (const { name, record } of found) {
        if (record.schema !== SCHEMA) problems.push(`${name} has schema ${record.schema}`);
        if (!TARGETS[record.target]) {
            problems.push(`${name} names ${record.target}, no declared target`);
            continue;
        }
        if (name !== provenanceFile(record.target)) problems.push(`${name} records ${record.target}`);
        for (const key of ["commit", "dirty"]) {
            if (record[key] !== first[key]) problems.push(`${name} has ${key} ${record[key]}, ${found[0].name} has ${first[key]}`);
        }
        for (const key of Object.keys(record.inputs)) {
            if (record.inputs[key] !== first.inputs[key]) problems.push(`${name} was built from another ${key} than ${found[0].name}: a mixed build`);
        }
        for (const [file, hash] of Object.entries(record.files)) {
            recorded.set(file, hash);
            const location = path.join(root, file);
            if (!fs.existsSync(location)) problems.push(`${file} of ${name} is missing`);
            else if (fileHash(location) !== hash) problems.push(`${file} is not the file ${name} recorded`);
        }
        if (problems.length === 0) {
            for (const binary of identityBinaries(record.target)) {
                const problem = binaryProblem(root, record.target, binary, record.inputs.sourceHash);
                if (problem) problems.push(problem);
            }
        }
    }
    if (targets) {
        const names = found.map(({ record }) => record.target).sort();
        if (names.join() !== [...targets].sort().join()) problems.push(`the package holds ${names.join()}, intended ${[...targets].sort().join()}`);
    }
    const shipped = [...filesUnder(path.join(root, "dist"), "dist"), ...filesUnder(path.join(root, "native", "artifacts"), "native/artifacts")];
    for (const file of shipped) {
        const name = path.basename(file);
        if (file.startsWith("native/artifacts/") && !isNapiOutput(name)) problems.push(`${file} is no approved output`);
        if (/^provenance\./.test(name) || recorded.has(file)) continue;
        problems.push(`${file} is not in any provenance record`);
    }
    return problems;
}

module.exports = { shippedOutputs, sourceHash, buildInputs, recordProvenance, packageProblems, recordsOf, fileHash, filesUnder, listingHash, textHash };

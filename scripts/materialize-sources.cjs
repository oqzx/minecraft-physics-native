// Unpacks the exact sources of the releases that have no local Mojang-named tree into
// MC_SOURCE_CACHE/<release>/client, the layout of MC_SOURCE_ROOT, from the commits pinned in
// PINNED_SOURCES. A commit is unpacked only when the release embedded in it is the release it is
// pinned for, so a moving or mislabelled branch cannot stand in. Nothing here touches the
// repository's checkout: `git archive` reads objects.
//
// A release in PINNED_CLIENTS is decompiled instead, from Mojang's own client jar. The jar is taken
// only at the sha1 pinned for it and when the version.json inside it names the release; its data
// packs and version.json are copied beside the sources.
//
// A release in PINNED_ROOT_SOURCES (26.2, 26.3) is unpacked from its commit into MC_SOURCE_ROOT/<release>/client.
//
// The six legacy releases are decompiled into MC_LEGACY_SOURCES/<release> (default legacy-src beside the source cache)
// from their Mojang-named jar at its NAMED_JARS sha256 (scripts/java-oracle/classpath.cjs), the jar remapped by
// legacy-mappings/<release>.tiny. generate-movement-sites and generate-primitive-map read these.
//
// Every tree made or found is then held to its hash in scripts/source-tree-pins.json (scripts/tree-pins.cjs): a decompile
// that does not reproduce the pinned tree, or a tree left from another input, fails the run and names the tree.
//   node scripts/materialize-sources.cjs [release...]
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { execFileSync, spawnSync } = require("node:child_process");
const { refIdentity, directoryIdentity } = require("./source-identity.cjs");
const { PINNED_SOURCES, PINNED_ROOT_SOURCES, PINNED_CLIENTS, CLIENT_DATA, DECOMPILER, clientJarRoot, decompilerJar, extractedRepo, sourceCache, mcSourceRoot } = require("./source-pins.cjs");
const { treeProblem } = require("./tree-pins.cjs");
const { openZip } = require("./zip-read.cjs");

const digest = (file, algorithm) => crypto.createHash(algorithm).update(fs.readFileSync(file)).digest("hex");

/** The jars of a pinned client, each checked against its pin. */
function pinnedJars(version, pin, root = clientJarRoot()) {
  const directory = path.join(root, version);
  const client = path.join(directory, `minecraft-${version}-client.jar`);
  if (!fs.existsSync(client)) throw new Error(`No client jar of ${version}: ${client}`);
  const sha1 = digest(client, "sha1");
  if (sha1 !== pin.client) throw new Error(`The client jar of ${version} is ${sha1}, pinned ${pin.client}`);
  const name = JSON.parse(openZip(client).read("version.json")?.toString("utf8") ?? "{}").id;
  if (name !== version) throw new Error(`The client jar pinned for ${version} embeds ${name}`);
  if (!pin.remapped) return { client, named: client };
  const named = path.join(directory, `intermediary-${version}-mojmap-${version}`, `minecraft-${version}-fabric-merged+custom-no-environment+fixed-mojmap+intermediary.jar`);
  if (!fs.existsSync(named)) throw new Error(`No Mojang-named jar of ${version}: ${named}`);
  const sha256 = digest(named, "sha256");
  if (sha256 !== pin.remapped) throw new Error(`The Mojang-named jar of ${version} is ${sha256}, pinned ${pin.remapped}`);
  return { client, named };
}

/** Copies a jar's version.json and default data pack under `client`. */
function copyClientData(jar, client) {
  const zip = openZip(jar);
  for (const name of zip.names.filter((entry) => (entry === "version.json" || entry.startsWith("data/")) && !entry.endsWith("/"))) {
    const file = path.join(client, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, zip.read(name));
  }
}

/** The data pack of a release whose sources come from elsewhere, once. */
function clientData(version, sha1) {
  const client = path.join(sourceCache(), version, "client");
  if (fs.existsSync(path.join(client, "data"))) return;
  const { client: jar } = pinnedJars(version, { client: sha1 });
  copyClientData(jar, client);
}

/** Decompiles a pinned client into the cache, once; a partial run never looks finished. */
function decompileClient(version, pin) {
  const target = path.join(sourceCache(), version);
  if (fs.existsSync(path.join(target, "client", "net", "minecraft")) && directoryIdentity(target) === version) return target;
  const { client, named } = pinnedJars(version, pin);
  const decompiler = decompilerJar();
  if (!fs.existsSync(decompiler) || digest(decompiler, "sha1") !== DECOMPILER.sha1) {
    throw new Error(`${DECOMPILER.name} ${DECOMPILER.version} is not at ${decompiler}`);
  }
  const staging = `${target}.partial`;
  fs.rmSync(staging, { recursive: true, force: true });
  fs.rmSync(target, { recursive: true, force: true });
  fs.mkdirSync(path.join(staging, "client"), { recursive: true });
  const decompiled = path.join(staging, "out");
  execFileSync("java", ["-Xmx5g", "-jar", decompiler, "--silent", "--folder", "--only=net/minecraft", named, decompiled], { stdio: ["ignore", "ignore", "inherit"] });
  fs.renameSync(path.join(decompiled, "net"), path.join(staging, "client", "net"));
  fs.rmSync(decompiled, { recursive: true, force: true });
  copyClientData(client, path.join(staging, "client"));
  fs.renameSync(staging, target);
  return target;
}

const legacySources = () => process.env.MC_LEGACY_SOURCES ?? path.join(sourceCache(), "..", "legacy-src");

/** Decompiles a legacy release's pinned Mojang-named jar, once; a partial run never looks finished. */
function decompileLegacy(version) {
  const target = path.join(legacySources(), version);
  const { namedJar } = require("./java-oracle/classpath.cjs");
  if (fs.existsSync(path.join(target, "done"))) {
    // a tree from before the data pack was kept beside the sources (1.13.2 has one)
    if (!fs.existsSync(path.join(target, "data"))) copyData(namedJar(version).jar, target);
    return target;
  }
  const { jar, client } = namedJar(version);
  const decompiler = decompilerJar();
  if (!fs.existsSync(decompiler) || digest(decompiler, "sha1") !== DECOMPILER.sha1) {
    throw new Error(`${DECOMPILER.name} ${DECOMPILER.version} is not at ${decompiler}`);
  }
  const staging = `${target}.partial`;
  fs.rmSync(staging, { recursive: true, force: true });
  fs.rmSync(target, { recursive: true, force: true });
  fs.mkdirSync(staging, { recursive: true });
  execFileSync("java", ["-Xmx2g", "-jar", decompiler, "--thread-count=2", "--log-level=error", "--folder", jar, staging], { stdio: ["ignore", "ignore", "inherit"] });
  // the sources and the default data pack (its tags, from 1.13): the jar's other resources are not read here
  for (const name of fs.readdirSync(staging)) if (name !== "net") fs.rmSync(path.join(staging, name), { recursive: true, force: true });
  copyData(jar, staging);
  fs.writeFileSync(path.join(staging, "done"), `${client}\n`);
  fs.renameSync(staging, target);
  return target;
}

/** A jar's data/ entries under `directory`, as they are in the jar. */
function copyData(jar, directory) {
  const zip = openZip(jar);
  for (const name of zip.names.filter((entry) => entry.startsWith("data/") && !entry.endsWith("/"))) {
    const file = path.join(directory, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, zip.read(name));
  }
}

/** Unpacks a pinned commit's client under `root`/<release>, once; a partial run never looks finished. */
function materialize(version, commit, { root = sourceCache(), whole = false } = {}) {
  const target = path.join(root, version);
  if (fs.existsSync(path.join(target, "client", "net", "minecraft")) && directoryIdentity(target) === version) return target;
  if (!fs.existsSync(extractedRepo())) throw new Error(`No extracted data repository at ${extractedRepo()}: set MC_EXTRACTED_DATA_REPO`);
  const name = refIdentity(extractedRepo(), commit);
  if (name !== version) throw new Error(`Commit ${commit} pinned for ${version} embeds ${name}`);
  const staging = `${target}.partial`;
  fs.rmSync(staging, { recursive: true, force: true });
  fs.mkdirSync(staging, { recursive: true });
  const archive = path.join(staging, "source.tar");
  const files = whole ? ["client"] : spawnSync("git", ["-C", extractedRepo(), "cat-file", "-e", `${commit}:client/version.json`]).status === 0
    ? ["client/net", "client/version.json"]
    : ["client/net"];
  execFileSync("git", ["-C", extractedRepo(), "archive", "--format=tar", "-o", archive, commit, ...files], { stdio: "inherit" });
  const unpacked = spawnSync("tar", ["-xf", "source.tar"], { cwd: staging, stdio: "inherit" });
  if (unpacked.status !== 0) throw new Error(`Could not unpack ${archive}`);
  fs.rmSync(archive);
  // the data pack an earlier run put beside the sources is kept; nothing else of what was there
  const data = path.join(target, "client", "data");
  if (fs.existsSync(data) && !fs.existsSync(path.join(staging, "client", "data"))) fs.cpSync(data, path.join(staging, "client", "data"), { recursive: true });
  fs.rmSync(target, { recursive: true, force: true });
  fs.renameSync(staging, target);
  return target;
}

if (require.main === module) {
  const wanted = process.argv.slice(2);
  const made = [];
  for (const [version, commit] of Object.entries(PINNED_SOURCES)) {
    if (wanted.length > 0 && !wanted.includes(version)) continue;
    materialize(version, commit);
    made.push(`cache:${version}`);
    console.log(`${version} ${commit}`);
  }
  for (const [version, commit] of Object.entries(PINNED_ROOT_SOURCES)) {
    if (wanted.length > 0 && !wanted.includes(version)) continue;
    materialize(version, commit, { root: mcSourceRoot(), whole: true });
    made.push(`mc-source:${version}`);
    console.log(`${version} ${commit} (MC_SOURCE_ROOT)`);
  }
  for (const [version, pin] of Object.entries(PINNED_CLIENTS)) {
    if (wanted.length > 0 && !wanted.includes(version)) continue;
    decompileClient(version, pin);
    made.push(`cache:${version}`);
    console.log(`${version} ${pin.client}`);
  }
  for (const { version } of require("./legacy-release-data.cjs").LEGACY_RELEASES) {
    if (wanted.length > 0 && !wanted.includes(version)) continue;
    decompileLegacy(version);
    made.push(`legacy:${version}`);
    console.log(`${version} (MC_LEGACY_SOURCES)`);
  }
  for (const [version, sha1] of Object.entries(CLIENT_DATA)) {
    if (wanted.length > 0 && !wanted.includes(version)) continue;
    clientData(version, sha1);
    if (!made.includes(`cache:${version}`)) made.push(`cache:${version}`);
  }
  const problems = made.map((key) => treeProblem(key)).filter(Boolean);
  for (const problem of problems) console.error(problem);
  if (problems.length > 0) process.exit(1);
  console.log(`sources: ${made.length} trees, each equal to its pin`);
}

module.exports = { materialize, decompileClient, decompileLegacy, legacySources, pinnedJars };

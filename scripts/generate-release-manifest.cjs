// Writes generated/release-manifest.json: one joined record per registered release, each fact from
// the exact release or left null with its reason, never from a neighbour or from version order.
//   protocol, dataVersion  the running client's report (generated/runtime-registries.json) where a
//                          dump exists, minecraft-data's protocolVersions by exact name; the two
//                          and the source's version.json must agree
//   mappings               Mojang's client mappings sha1 (generated/mojang-releases.json), or
//                          `unobfuscated` for 26.x, `unpublished` before 1.14.4
//   source                 what is known of the sources: an embedded identity with its immutable
//                          commit or tree, a directory name only, a stand-in, a rejection, or none
//   tables                 the sha256 of the release's generated Rust table, line endings aside
// Run with --check to compare against the checked-in file instead of writing it.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { registeredReleases } = require("./registered-releases.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");
const { PINNED_SOURCES, PINNED_CLIENTS, TREE_CLIENTS, NAMED_JARS, DECOMPILER, extractedRepo } = require("./source-pins.cjs");
const { legacyTree } = require("./source-roots.cjs");
const { aliasDefect, evidenceHash } = require("./generate-release-aliases.cjs");
const { refIdentity, directoryIdentity } = require("./source-identity.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "release-manifest.json");
const mcSourceRoot = process.env.MC_SOURCE_ROOT ?? path.join(repositoryRoot, "..", "..", "pr", "mc-source");
const aliasesPath = path.join(repositoryRoot, "generated", "release-aliases.json");
const SCHEMA = 3;

const readJson = (file) => JSON.parse(fs.readFileSync(file, "utf8"));

function protocolTable() {
  const entry = require.resolve("minecraft-data/package.json", { paths: [repositoryRoot] });
  const table = readJson(path.join(path.dirname(entry), "minecraft-data", "data", "pc", "common", "protocolVersions.json"));
  return new Map(table.map((row) => [row.minecraftVersion, row]));
}

/** The commit a rejected branch holds: it names a release and embeds another; never read. */
const REJECTED_REFS = {
  "1.16.4": { ref: "origin/client1.16.4", commit: "08e782cbdb11cfaec9cdb27240b38b105e5054e6" },
};

/** The content of a source tree: its java files, by relative path, line endings aside. */
function treeSha256(tree) {
  const files = (directory) => fs.readdirSync(directory, { withFileTypes: true })
    .sort((left, right) => (left.name < right.name ? -1 : 1))
    .flatMap((entry) => (entry.isDirectory() ? files(path.join(directory, entry.name)) : [path.join(directory, entry.name)]));
  const hash = crypto.createHash("sha256");
  for (const file of files(tree).filter((name) => name.endsWith(".java"))) {
    hash.update(path.relative(tree, file).replaceAll("\\", "/")).update("\0").update(fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n")).update("\0");
  }
  return hash.digest("hex");
}

/** What is known of a release's sources: an exact, checked identity or the reason there is none. */
function sourceRecord(version) {
  const standIn = LEGACY_RELEASES.find((release) => release.version === version && release.source !== version);
  if (standIn) {
    const record = readJson(aliasesPath).aliases[version];
    const defect = aliasDefect(record);
    if (defect) return { status: "stand-in", embedded: null, sourceRelease: standIn.source, reason: defect };
    return { status: "alias", embedded: standIn.source, sourceRelease: standIn.source, scope: record.scope, evidence: evidenceHash(record), mapping: "yarn" };
  }
  if (PINNED_CLIENTS[version]) {
    const { client, remapped } = PINNED_CLIENTS[version];
    return {
      status: "embedded",
      embedded: version,
      clientJar: { sha1: client, ...(remapped ? { remappedSha256: remapped } : {}) },
      decompiler: `${DECOMPILER.name} ${DECOMPILER.version}`,
      mapping: "mojang",
      ...(REJECTED_REFS[version] ? { rejectedRef: REJECTED_REFS[version] } : {}),
    };
  }
  if (PINNED_SOURCES[version]) {
    const embedded = fs.existsSync(extractedRepo()) ? refIdentity(extractedRepo(), PINNED_SOURCES[version]) : version;
    if (embedded !== version) throw new Error(`The commit pinned for ${version} embeds ${embedded}`);
    return { status: "embedded", embedded, commit: PINNED_SOURCES[version], mapping: "mojang" };
  }
  if (LEGACY_RELEASES.some((release) => release.version === version)) {
    // the release's own client, remapped by the recorder's mapping and decompiled by the pinned Vineflower
    const tree = legacyTree(version);
    if (!tree) return { status: "unavailable", embedded: null };
    const embedded = directoryIdentity(tree);
    if (embedded !== version) return { status: embedded === null ? "directory-only" : "rejected", embedded, mapping: "recorder" };
    return {
      status: "embedded",
      embedded,
      clientJar: { sha1: TREE_CLIENTS[version], remappedSha256: NAMED_JARS[version] },
      decompiler: `${DECOMPILER.name} ${DECOMPILER.version}`,
      mapping: "recorder",
      tree: treeSha256(tree),
    };
  }
  const tree = path.join(mcSourceRoot, version);
  if (fs.existsSync(tree)) {
    const embedded = directoryIdentity(tree);
    if (embedded === null) return { status: "directory-only", embedded: null, mapping: "mojang" };
    if (embedded !== version) return { status: "rejected", embedded, mapping: "mojang" };
    const client = TREE_CLIENTS[version];
    return { status: "embedded", embedded, mapping: "mojang", ...(client ? { clientJar: { sha1: client } } : {}), tree: treeSha256(tree) };
  }
  return { status: "unavailable", embedded: null };
}

function sha256OfModule(version) {
  const file = path.join(repositoryRoot, "native", "data", "releases", `v${version.replaceAll(".", "_")}.rs`);
  return crypto.createHash("sha256").update(fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n")).digest("hex");
}

/** One joined record. Throws where two sources of a fact disagree. */
function joinRecord(version, { runtime, protocolRow, mojang, source, tables }) {
  if (runtime && protocolRow && runtime.runtimeProtocolVersion !== protocolRow.version) {
    throw new Error(`${version}: the client reports protocol ${runtime.runtimeProtocolVersion}, minecraft-data ${protocolRow.version}`);
  }
  const protocol = runtime?.runtimeProtocolVersion ?? protocolRow?.version ?? null;
  const mappings = mojang?.mappings?.sha1
    ? { kind: "mojang", sha1: mojang.mappings.sha1 }
    : { kind: /^\d\d\./.test(version) ? "unobfuscated" : "unpublished", sha1: null };
  return {
    release: version,
    kind: "release",
    versionJsonSha1: mojang?.sha1 ?? null,
    protocol,
    protocolSource: runtime ? "client-runtime" : protocolRow ? "minecraft-data" : null,
    dataVersion: protocolRow?.dataVersion ?? null,
    dataVersionSource: protocolRow?.dataVersion !== undefined ? "minecraft-data" : null,
    mappings,
    source,
    tables,
  };
}

function build() {
  const runtime = readJson(path.join(repositoryRoot, "generated", "runtime-registries.json")).releases;
  const mojang = new Map(readJson(path.join(repositoryRoot, "generated", "mojang-releases.json")).releases.map((release) => [release.id, release]));
  const protocols = protocolTable();
  const previous = fs.existsSync(outputPath) ? readJson(outputPath).releases : {};
  const sourcesAtHand = fs.existsSync(extractedRepo()) && LEGACY_RELEASES.every(({ version }) => legacyTree(version));
  const releases = {};
  for (const { version } of registeredReleases()) {
    releases[version] = joinRecord(version, {
      runtime: runtime[version],
      protocolRow: protocols.get(version),
      mojang: mojang.get(version),
      // a host without the source trees keeps what a host with them established
      source: sourcesAtHand ? sourceRecord(version) : previous[version]?.source ?? { status: "unavailable", embedded: null },
      tables: { sha256: sha256OfModule(version) },
    });
  }
  return `${JSON.stringify({ schema: SCHEMA, generatedBy: "scripts/generate-release-manifest.cjs", releases }, null, 2)}\n`;
}

if (require.main === module) {
  const text = build();
  if (process.argv.includes("--check")) {
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replace(/\r\n/g, "\n") : "";
    if (current !== text) {
      console.error("generated/release-manifest.json is stale: run scripts/generate-release-manifest.cjs");
      process.exit(1);
    }
    console.log("generated/release-manifest.json is current.");
  } else {
    fs.writeFileSync(outputPath, text);
    console.log(`Joined ${Object.keys(JSON.parse(text).releases).length} releases.`);
  }
}

module.exports = { build, joinRecord, sourceRecord, sha256OfModule, treeSha256 };

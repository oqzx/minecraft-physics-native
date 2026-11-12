/* eslint-disable no-console */
// Writes generated/mojang-releases.json: every release (type "release") from 1.8 on in Mojang's
// version metadata (version_manifest_v2.json, MOJANG_VERSION_MANIFEST: a URL or a file, default
// piston-meta), with its release time and the sha1 of its version JSON, and for each release the
// engine registers (scripts/registered-releases.cjs) the client mappings it ships (1.14.4 on) as
// fingerprints of the members of the classes physics reads: per class (by simple name, so a move
// to another package is the same class), the sha256 of its sorted member lines without line
// numbers or obfuscated names, so a fingerprint changes when a field or method is added, removed,
// renamed or retyped. scripts/report-physics-breakpoints.cjs compares them. Mojang published
// mappings from 1.14.4; from 26.1 the client ships unobfuscated, with none (`mappings: null`).
//
// Mappings are downloaded once into MC_MAPPINGS_CACHE (default <tmp>/mc-physics-mappings). A
// release whose metadata cannot be fetched keeps its previous entry.
//
// The sha1 of a registered release's version JSON is the root of its pins: scripts/fetch-jar-inputs.cjs takes that JSON,
// and through it the client and libraries, only at this sha1. So a list that gives a registered release another sha1, or
// no longer lists it, fails the run before anything is written, naming the release; after a review
// `--accept-changed <release>` takes the new one. A fetched version JSON is held to the sha1 the list gives it.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const crypto = require("node:crypto");
const { registeredReleases, compareReleases } = require("./registered-releases.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "mojang-releases.json");
const manifestSource = process.env.MOJANG_VERSION_MANIFEST
  ?? "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const cacheRoot = process.env.MC_MAPPINGS_CACHE ?? path.join(os.tmpdir(), "mc-physics-mappings");
const firstRelease = "1.8";

// The classes physics reads, by their Mojang simple names.
const mappedClasses = [
  "Entity",
  "LivingEntity",
  "Player",
  "LocalPlayer",
  "Abilities",
  "Boat",
  "AbstractBoat",
  "AbstractMinecart",
  "AbstractHorse",
  "FireworkRocketEntity",
  "PistonMovingBlockEntity",
  "AABB",
  "Vec3",
  "VoxelShape",
  "Shapes",
  "Mth",
  "Attributes",
  "MobEffects",
  "FlowingFluid",
  "WorldBorder",
];

async function text(source) {
  if (!/^https?:/.test(source)) return fs.readFileSync(source, "utf8");
  const response = await fetch(source);
  if (!response.ok) throw new Error(`${source}: HTTP ${response.status}`);
  return response.text();
}

/** The client mappings of a release, from the cache or downloaded into it (checked against sha1). */
async function clientMappings(release, download) {
  const file = path.join(cacheRoot, `${release}-client.txt`);
  const sha1 = (data) => crypto.createHash("sha1").update(data).digest("hex");
  if (fs.existsSync(file) && sha1(fs.readFileSync(file)) === download.sha1) return fs.readFileSync(file, "utf8");
  const data = await text(download.url);
  if (sha1(data) !== download.sha1) throw new Error(`${release} client mappings do not match their sha1`);
  fs.mkdirSync(cacheRoot, { recursive: true });
  fs.writeFileSync(file, data);
  return data;
}

/** The member fingerprints of the mapped classes in ProGuard-format mappings. */
function classFingerprints(mappings) {
  const classes = {};
  let current = null;
  for (const line of mappings.split("\n")) {
    if (line.startsWith("#") || line.trim() === "") continue;
    if (!line.startsWith(" ")) {
      const name = /^(\S+) -> \S+:$/.exec(line.trim())?.[1];
      const simple = name?.split(".").at(-1);
      current = simple && mappedClasses.includes(simple) && !classes[simple] ? { name, members: [] } : null;
      if (current) classes[simple] = current;
      continue;
    }
    if (current) {
      current.members.push(line.trim().replace(/^\d+:\d+:/, "").replace(/ -> \S+$/, "").replace(/:\d+:\d+$/, ""));
    }
  }
  return Object.fromEntries(Object.entries(classes).map(([simple, { name, members }]) => {
    const sorted = [...new Set(members)].sort();
    return [simple, {
      name,
      members: sorted.length,
      sha256: crypto.createHash("sha256").update(sorted.join("\n")).digest("hex"),
    }];
  }));
}

/** What a new list changes of the pins of registered releases: one line per release, empty when nothing. */
function pinChanges(previousReleases, listed, registered, accepted = new Set()) {
  const changes = [];
  const byId = new Map(listed.map((version) => [version.id, version]));
  for (const id of registered) {
    const kept = previousReleases.get(id);
    if (!kept || accepted.has(id)) continue;
    const now = byId.get(id);
    if (!now) changes.push(`${id}: Mojang's list no longer has it; pinned version json ${kept.sha1}`);
    else if (now.sha1 !== kept.sha1) changes.push(`${id}: Mojang's list gives version json ${now.sha1}, pinned ${kept.sha1}. Review it, then pass --accept-changed ${id}`);
  }
  return changes;
}

async function main() {
  const accepted = new Set(process.argv.flatMap((argument, index) => (process.argv[index - 1] === "--accept-changed" ? [argument] : [])));
  const previous = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")) : null;
  const previousReleases = new Map((previous?.releases ?? []).map((release) => [release.id, release]));
  const manifest = JSON.parse(await text(manifestSource));
  const registered = new Set(registeredReleases().map(({ version }) => version));
  const releases = manifest.versions
    .filter((version) => version.type === "release" && /^\d+(?:\.\d+)+$/.test(version.id))
    .filter((version) => compareReleases(version.id, firstRelease) >= 0)
    .sort((left, right) => compareReleases(left.id, right.id));
  const changes = pinChanges(previousReleases, releases, [...registered].filter((id) => compareReleases(id, firstRelease) >= 0), accepted);
  if (changes.length > 0) throw new Error(`the pins of registered releases would change:\n${changes.join("\n")}`);
  const entries = [];
  for (const version of releases) {
    const kept = previousReleases.get(version.id);
    const entry = { id: version.id, releaseTime: version.releaseTime, sha1: version.sha1 };
    if (registered.has(version.id)) {
      if (kept?.sha1 === version.sha1 && kept.mappings !== undefined) {
        entry.mappings = kept.mappings;
      } else {
        const body = await text(version.url);
        const bodySha1 = crypto.createHash("sha1").update(body).digest("hex");
        if (bodySha1 !== version.sha1) throw new Error(`${version.id}: the version json at ${version.url} is ${bodySha1}, the list gives ${version.sha1}`);
        const details = JSON.parse(body);
        const download = details.downloads?.client_mappings;
        entry.mappings = download
          ? { sha1: download.sha1, classes: classFingerprints(await clientMappings(version.id, download)) }
          : null;
      }
    }
    entries.push(entry);
  }
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  fs.writeFileSync(outputPath, `${JSON.stringify({
    schema: 1,
    generatedBy: "scripts/generate-mojang-releases.cjs",
    source: "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
    latestRelease: manifest.latest.release,
    firstRelease,
    mappedClasses,
    releases: entries,
  }, null, 2)}\n`);
  console.log(`Recorded ${entries.length} Mojang releases, ${entries.filter((entry) => entry.mappings).length} with mappings.`);
}

if (require.main === module) {
  main().catch((error) => {
    console.error(error.message ?? error);
    process.exitCode = 1;
  });
}

module.exports = { pinChanges };

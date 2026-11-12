// Fills the jar folders the oracle stages run on (generate-block-shapes, generate-entity-data, generate-legacy-shapes),
// on any host, every file checked against its pin before it is kept:
//   node scripts/fetch-jar-inputs.cjs [--derived <dir|url>]... [--release <v>[,<v>]] [--decompiler] [--offline] [--check]
//
// What comes from where, for the 44 modern and 6 legacy releases:
// - each release's version json, from piston-meta by the sha1 generated/mojang-releases.json pins, into
//   MC_CLIENT_JAR_ROOT/<release>/version.json;
// - the client jar, at the sha1 that json gives, which must also be the one scripts/source-pins.cjs pins where it names
//   one, into MC_CLIENT_JAR_ROOT/<release>/minecraft-<release>-client.jar;
// - every library that json puts on the classpath (classpath.cjs libraryArtifacts), at its sha1, into MC_LIBRARY_ROOT;
// - the Mojang-named jar of a release before 26.1 (NAMED_JARS), which Mojang does not publish and whose bytes cannot
//   be made again (scripts/source-pins.cjs): only from a --derived folder or URL in unimined's layout, at its sha256;
// - with --decompiler, Vineflower from Maven Central at DECOMPILER's sha1, into MC_DECOMPILER_JAR.
// A file already in place with its hash is kept, a --derived copy with the right hash is taken before the network, and
// anything else is refused by name: nothing unpinned is ever kept, and a release never borrows a neighbour's file.
// --check fetches nothing and fails on anything absent or wrong.
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const { NAMED_JARS, DECOMPILER, namedJarPath, clientJarRoot, decompilerJar } = require("./source-pins.cjs");
const { libraryArtifacts, libraryRoots, clientPin } = require("./java-oracle/classpath.cjs");
const { LEGACY_RELEASES } = require("./legacy-release-data.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const hash = (bytes, algorithm) => crypto.createHash(algorithm).update(bytes).digest("hex");
const versionJsonUrl = (id, sha1) => `https://piston-meta.mojang.com/v1/packages/${sha1}/${id}.json`;
const MAVEN_CENTRAL = "https://repo1.maven.org/maven2";

/** Every release the oracle stages run on: the six legacy ones, then the modern manifest's. */
function releases() {
  const modern = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", "minecraft-versions.json"), "utf8")).versions;
  return [...LEGACY_RELEASES.map(({ version }) => version), ...modern];
}

/** The sha1 of each release's version json, as Mojang's list gave it when generated/mojang-releases.json was made. */
function versionJsonPins() {
  const list = JSON.parse(fs.readFileSync(path.join(repositoryRoot, "generated", "mojang-releases.json"), "utf8")).releases;
  return new Map(list.map(({ id, sha1 }) => [id, sha1]));
}

class Fetcher {
  constructor({ derived = [], offline = false, check = false }) {
    Object.assign(this, { derived, offline, check, counts: { present: 0, copied: 0, downloaded: 0 }, problems: [] });
  }

  async bytesFrom(source) {
    if (/^https?:\/\//.test(source)) {
      // (three tries: one of 3000 requests failing for a moment is no reason to stop)
      for (let attempt = 1; ; attempt += 1) {
        try {
          const response = await fetch(source);
          if (!response.ok) throw new Error(`${source}: HTTP ${response.status}`);
          return Buffer.from(await response.arrayBuffer());
        } catch (error) {
          if (attempt === 3) throw error;
        }
      }
    }
    return fs.existsSync(source) ? fs.readFileSync(source) : null;
  }

  /**
   * Puts the file at `target` with `digest` under `algorithm`, from the first of `sources` that has it (a derived copy,
   * then the network): `true` when it is in place.
   */
  async ensure(target, algorithm, digest, sources, label) {
    if (fs.existsSync(target) && hash(fs.readFileSync(target), algorithm) === digest) {
      this.counts.present += 1;
      return true;
    }
    if (this.check) {
      this.problems.push(`${label}: ${fs.existsSync(target) ? "not at its pin" : "absent"} at ${target}`);
      return false;
    }
    const tried = [];
    for (const source of sources) {
      const remote = /^https?:\/\//.test(source);
      if (remote && this.offline) continue;
      let bytes;
      try {
        bytes = await this.bytesFrom(source);
      } catch (error) {
        tried.push(error.message);
        continue;
      }
      if (!bytes) continue;
      const found = hash(bytes, algorithm);
      if (found !== digest) {
        tried.push(`${source} is ${found}`);
        continue;
      }
      fs.mkdirSync(path.dirname(target), { recursive: true });
      const partial = `${target}.${process.pid}.${crypto.randomBytes(4).toString("hex")}.partial`;
      fs.writeFileSync(partial, bytes);
      fs.renameSync(partial, target);
      this.counts[remote ? "downloaded" : "copied"] += 1;
      return true;
    }
    this.problems.push(`${label}: no source has ${algorithm} ${digest}${tried.length ? ` (${tried.join("; ")})` : ""}`);
    return false;
  }

  derivedSources(relative) {
    return this.derived.map((base) => (/^https?:\/\//.test(base) ? `${base.replace(/\/$/, "")}/${relative}` : path.join(base, ...relative.split("/"))));
  }
}

/** One release's version json, client, libraries and Mojang-named jar. */
async function release(fetcher, version, pins, root, libraries) {
  const jsonPin = pins.get(version);
  if (!jsonPin) {
    fetcher.problems.push(`${version}: generated/mojang-releases.json pins no version json`);
    return;
  }
  const jsonFile = path.join(root, version, "version.json");
  if (!await fetcher.ensure(jsonFile, "sha1", jsonPin, [versionJsonUrl(version, jsonPin)], `${version} version json`)) return;
  const json = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
  if (json.id !== version) {
    fetcher.problems.push(`${version}: the version json pinned for it is ${json.id}`);
    return;
  }
  const client = json.downloads.client;
  const pinned = clientPin(version);
  if (pinned && pinned !== client.sha1) fetcher.problems.push(`${version}: its version json gives the client ${client.sha1}, scripts/source-pins.cjs pins ${pinned}`);
  const clientRelative = `${version}/minecraft-${version}-client.jar`;
  await fetcher.ensure(path.join(root, ...clientRelative.split("/")), "sha1", client.sha1, [...fetcher.derivedSources(clientRelative), client.url], `${version} client`);
  for (const artifact of libraryArtifacts(json)) {
    const target = path.join(libraries.mojang, ...artifact.path.split("/"));
    const parts = artifact.path.split("/");
    const [file, revision, name] = [parts.pop(), parts.pop(), parts.pop()];
    const gradle = path.join(libraries.gradle, parts.join("."), name, revision, artifact.sha1.replace(/^0+/, ""), file);
    await fetcher.ensure(target, "sha1", artifact.sha1, [gradle, artifact.url], `${version} library ${artifact.name}`);
  }
  if (NAMED_JARS[version]) {
    const relative = namedJarPath(version);
    await fetcher.ensure(path.join(root, ...relative.split("/")), "sha256", NAMED_JARS[version], fetcher.derivedSources(relative),
      `${version} Mojang-named jar (not published by Mojang: give --derived)`);
  }
}

async function main() {
  const values = (name) => process.argv.flatMap((value, index) => (value === `--${name}` ? [process.argv[index + 1]] : []));
  const only = values("release").flatMap((value) => value.split(","));
  const fetcher = new Fetcher({ derived: values("derived"), offline: process.argv.includes("--offline"), check: process.argv.includes("--check") });
  const root = clientJarRoot();
  const libraries = libraryRoots();
  const pins = versionJsonPins();
  const wanted = releases().filter((version) => only.length === 0 || only.includes(version));
  for (const version of only) if (!wanted.includes(version)) fetcher.problems.push(`${version} is no registered release`);
  // (a few releases at a time: the libraries they share are fetched once, by whichever comes first)
  for (let at = 0; at < wanted.length; at += 4) {
    await Promise.all(wanted.slice(at, at + 4).map((version) => release(fetcher, version, pins, root, libraries)));
  }
  if (process.argv.includes("--decompiler")) {
    const { name, version, sha1 } = DECOMPILER;
    await fetcher.ensure(decompilerJar(), "sha1", sha1, [`${MAVEN_CENTRAL}/org/${name}/${name}/${version}/${name}-${version}.jar`], `${name} ${version}`);
  }
  const { present, copied, downloaded } = fetcher.counts;
  console.log(`jar inputs of ${wanted.length} releases under ${root} and ${libraries.mojang}: ${present} in place, ${copied} copied, ${downloaded} downloaded`);
  for (const problem of fetcher.problems) console.error(problem);
  if (fetcher.problems.length > 0) process.exit(1);
}

if (require.main === module) {
  main().catch((error) => {
    console.error(error.stack ?? error);
    process.exit(1);
  });
}

module.exports = { releases, versionJsonPins, versionJsonUrl };

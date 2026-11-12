// The classpath of a release's jar: the Mojang-named jar at its pin (NAMED_JARS, or the client's sha1 from 26.1), and
// the libraries its version json lists, found by their maven coordinates in Mojang's or Gradle's library folders with
// the sha1 the json gives. A jar that is not the pinned one throws: the oracle's answer is only that release's answer if
// the code running is. scripts/fetch-jar-inputs.cjs fills these folders from Mojang on a clean host.
const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { PINNED_CLIENTS, CLIENT_DATA, TREE_CLIENTS, NAMED_JARS, namedJarPath, clientJarRoot } = require("../source-pins.cjs");

const digests = new Map();
/** A file's digest, once per file as it stands (a 30 MB jar is asked for several times a release). */
function digest(file, algorithm) {
  const { size, mtimeMs } = fs.statSync(file);
  const key = `${algorithm}:${file}:${size}:${mtimeMs}`;
  if (!digests.has(key)) digests.set(key, crypto.createHash(algorithm).update(fs.readFileSync(file)).digest("hex"));
  return digests.get(key);
}
const sha1 = (file) => digest(file, "sha1");

/** The sha1 a release's client jar is pinned at, where scripts/source-pins.cjs names one. */
const clientPin = (version) => PINNED_CLIENTS[version]?.client ?? CLIENT_DATA[version] ?? TREE_CLIENTS[version] ?? null;

/** The Mojang-named jar of a release: its client from 26.1, which ships unobfuscated, else the jar unimined remapped. */
function namedJar(version, root = clientJarRoot()) {
  const client = path.join(root, version, `minecraft-${version}-client.jar`);
  const pinned = clientPin(version);
  if (!fs.existsSync(client)) throw new Error(`No client jar of ${version}: ${client}`);
  const clientSha1 = sha1(client);
  if (pinned && clientSha1 !== pinned) throw new Error(`The client jar of ${version} is ${clientSha1}, pinned ${pinned}`);
  if (Number(version.split(".")[0]) >= 26) return { jar: client, client: clientSha1, remapped: false };
  if (!NAMED_JARS[version]) throw new Error(`No Mojang-named jar of ${version} is pinned in NAMED_JARS`);
  const mapped = path.join(root, ...namedJarPath(version).split("/"));
  if (!fs.existsSync(mapped)) throw new Error(`No Mojang-named jar of ${version}: ${mapped}`);
  const sha256 = digest(mapped, "sha256");
  if (sha256 !== NAMED_JARS[version]) throw new Error(`The Mojang-named jar of ${version} is ${sha256}, pinned ${NAMED_JARS[version]}`);
  return { jar: mapped, client: clientSha1, remapped: true };
}

/** Where the launcher keeps its libraries on this host, and Gradle's module cache. */
function libraryRoots() {
  const home = os.homedir();
  const launcher = { win32: path.join(home, "AppData", "Roaming", ".minecraft"), darwin: path.join(home, "Library", "Application Support", "minecraft") };
  return {
    mojang: process.env.MC_LIBRARY_ROOT ?? path.join(launcher[process.platform] ?? path.join(home, ".minecraft"), "libraries"),
    gradle: process.env.MC_GRADLE_MODULES ?? path.join(home, ".gradle", "caches", "modules-2", "files-2.1"),
  };
}

/**
 * The library artifacts a version json puts on the classpath, in its order. The rules are read as on Windows on every
 * host, so a Linux or macOS run puts the same jars on the path as the Windows one: only natives differ by OS, and they
 * are classifiers, not artifacts.
 */
function libraryArtifacts(json) {
  return json.libraries.filter((library) => library.downloads?.artifact && (library.rules ?? []).every((rule) => (rule.action === "allow"
    ? !rule.os || rule.os.name === "windows" : rule.os?.name !== "windows"))).map((library) => ({ name: library.name, ...library.downloads.artifact }));
}

function findLibrary(artifactPath, expected, roots) {
  const mojang = path.join(roots.mojang, ...artifactPath.split("/"));
  if (fs.existsSync(mojang) && sha1(mojang) === expected) return mojang;
  const parts = artifactPath.split("/");
  const file = parts.pop();
  const release = parts.pop();
  const name = parts.pop();
  // (Gradle names the folder after the sha1 as a number: leading zeros dropped)
  const gradle = path.join(roots.gradle, parts.join("."), name, release, expected.replace(/^0+/, ""), file);
  return fs.existsSync(gradle) && sha1(gradle) === expected ? gradle : null;
}

/**
 * The jars a release runs on; a library with no copy of the json's sha1 is named in `missing`, not swapped for a
 * neighbour. A generator refuses a classpath with anything missing (`completeClasspath`): which jars a host happens to
 * hold must not reach a table.
 */
function classpath(version, root = clientJarRoot()) {
  const { jar, client, remapped } = namedJar(version, root);
  const jsonFile = path.join(root, version, "version.json");
  const json = JSON.parse(fs.readFileSync(jsonFile, "utf8"));
  const roots = libraryRoots();
  const jars = [jar];
  const missing = [];
  for (const artifact of libraryArtifacts(json)) {
    const found = findLibrary(artifact.path, artifact.sha1, roots);
    if (found) jars.push(found);
    else missing.push(artifact.name);
  }
  return { jars, missing, client, remapped, versionJson: sha1(jsonFile), java: json.javaVersion?.majorVersion ?? 8 };
}

/** `classpath`, refused when a library is not at hand. */
function completeClasspath(version, root = clientJarRoot()) {
  const result = classpath(version, root);
  if (result.missing.length > 0) {
    throw new Error(`${version} lacks the libraries ${result.missing.join(", ")}: run scripts/fetch-jar-inputs.cjs, or set MC_LIBRARY_ROOT`);
  }
  return result;
}

module.exports = { classpath, completeClasspath, namedJar, libraryArtifacts, libraryRoots, clientPin };

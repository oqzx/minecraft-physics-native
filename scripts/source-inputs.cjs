// What a generated manifest recorded as a source, read back: `<pin>:<path>` (a pinned commit or
// client, scripts/source-pins.cjs), `mc-source:<path>` (under MC_SOURCE_ROOT) or a path relative to
// the manifest. A commit is read from
// MC_SOURCE_CACHE when unpacked there, else as a Git object; a client only exists decompiled, under
// MC_SOURCE_CACHE. Absent input is `null`, never an empty file: the caller has to say what it could
// not verify.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { spawnSync } = require("node:child_process");
const { PINNED_SOURCES, PINNED_CLIENTS, extractedRepo, mcSourceRoot, sourceCache } = require("./source-pins.cjs");
const { resolveRecordedPath } = require("./portable-paths.cjs");

const PIN = /^([0-9a-f]{40}):(.+)$/;
const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const pinnedRelease = (pin) => Object.keys(PINNED_SOURCES).find((version) => PINNED_SOURCES[version] === pin)
  ?? Object.keys(PINNED_CLIENTS).find((version) => PINNED_CLIENTS[version].client === pin)
  ?? null;

/** A Git object's bytes, by `git cat-file --batch`, null when the repository or the object is absent. */
function gitObjects(repository, names) {
  const objects = new Map(names.map((name) => [name, null]));
  if (names.length === 0 || !fs.existsSync(repository)) return objects;
  const result = spawnSync("git", ["-C", repository, "cat-file", "--batch"], { input: `${names.join("\n")}\n`, maxBuffer: 2 ** 31 - 1 });
  if (result.status !== 0) return objects;
  const output = result.stdout;
  let offset = 0;
  for (const name of names) {
    const lineEnd = output.indexOf(10, offset);
    const size = /^[0-9a-f]+ blob (\d+)$/.exec(output.toString("utf8", offset, lineEnd))?.[1];
    if (size === undefined) {
      offset = lineEnd + 1;
      continue;
    }
    objects.set(name, output.subarray(lineEnd + 1, lineEnd + 1 + Number(size)));
    offset = lineEnd + 1 + Number(size) + 1;
  }
  return objects;
}

/**
 * The bytes of each recorded input of a manifest in `manifestDirectory`, null for one not at hand.
 * A pin no release holds is an error: a manifest may not name an input nobody admitted.
 */
function readRecorded(manifestDirectory, recorded) {
  const found = new Map();
  const fromGit = [];
  for (const entry of recorded) {
    const pinned = PIN.exec(entry);
    if (!pinned) {
      const file = entry.startsWith("mc-source:") ? path.join(mcSourceRoot(), entry.slice("mc-source:".length)) : resolveRecordedPath(manifestDirectory, entry);
      found.set(entry, fs.existsSync(file) ? fs.readFileSync(file) : null);
      continue;
    }
    const release = pinnedRelease(pinned[1]);
    if (!release) throw new Error(`${entry} names no pinned commit or client`);
    const unpacked = path.join(sourceCache(), release, pinned[2]);
    if (fs.existsSync(unpacked)) found.set(entry, fs.readFileSync(unpacked));
    else if (PINNED_SOURCES[release]) fromGit.push(entry);
    else found.set(entry, null);
  }
  for (const [name, bytes] of gitObjects(extractedRepo(), fromGit)) found.set(name, bytes);
  return found;
}

module.exports = { readRecorded, gitObjects, sha256, pinnedRelease };

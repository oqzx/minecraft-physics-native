// The release a source really is: the name it carries, not the name of the branch or folder that
// holds it (origin/client1.16.4 is 20w45a). Read from DetectedVersion.name in the client's sources,
// SharedConstants.VERSION_STRING before DetectedVersion had a fixed name, version.json, or the
// VERSION_STRING of RealmsSharedConstants in the oldest trees.
const fs = require("node:fs");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const candidates = [
  "client/net/minecraft/DetectedVersion.java",
  "client/net/minecraft/SharedConstants.java",
  "client/version.json",
  // the Realms constants carry the release's name in the trees that predate version.json (1.8 - 1.13)
  "client/net/minecraft/realms/RealmsSharedConstants.java",
];

function embeddedName(file, text) {
  if (file.endsWith("version.json")) return JSON.parse(text).id ?? null;
  return /"name"\s*:\s*"([^"]+)"/.exec(text)?.[1]
    ?? /\bname\s*=\s*"([^"]+)"/.exec(text)?.[1]
    ?? /\bVERSION_STRING\s*=\s*"([^"]+)"/.exec(text)?.[1]
    ?? null;
}

/** The release name embedded in a git ref of the extracted data repository, null without one. */
function refIdentity(repository, ref) {
  for (const file of candidates) {
    let text;
    try {
      text = execFileSync("git", ["-C", repository, "show", `${ref}:${file}`], {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
        maxBuffer: 16 * 1024 * 1024,
      });
    } catch {
      continue;
    }
    const name = embeddedName(file, text);
    if (name) return name;
  }
  return null;
}

/** The release name embedded in a directory holding `client/...` or the sources directly. */
function directoryIdentity(root) {
  for (const file of candidates) {
    for (const candidate of [path.join(root, file), path.join(root, file.replace(/^client\//, ""))]) {
      if (!fs.existsSync(candidate)) continue;
      const name = embeddedName(file, fs.readFileSync(candidate, "utf8"));
      if (name) return name;
    }
  }
  return null;
}

const identities = new Map();

/**
 * Whether origin/client<release> is that release: its embedded name says so. A branch that names
 * another one is refused, and so is one that embeds no name, whose label proves nothing.
 */
function refIsRelease(repository, release) {
  const key = `${repository}\0${release}`;
  if (!identities.has(key)) identities.set(key, refIdentity(repository, `origin/client${release}`));
  const name = identities.get(key);
  return name === release;
}

module.exports = { refIdentity, directoryIdentity, refIsRelease };

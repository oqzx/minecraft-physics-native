/* eslint-disable no-console */
const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const audit = JSON.parse(fs.readFileSync(
  path.join(root, "generated", "physics-methods.json"),
  "utf8",
));
const versions = Object.keys(audit.releases);

// Every method the source audit tracks (scripts/extract-physics-methods.cjs), in the order it
// first lists them.
const methods = [];
for (const release of versions) {
  for (const [file, entry] of Object.entries(audit.releases[release].files ?? {})) {
    for (const method of Object.keys(entry.methods ?? {})) {
      const key = `${file}:${method}`;
      if (!methods.includes(key)) methods.push(key);
    }
  }
}

function hashes(release, key) {
  const [file, method] = key.split(":");
  return (audit.releases[release]?.files?.[file]?.methods?.[method] ?? [])
    .map((entry) => entry.sha256)
    .join(",");
}

function hasFile(release, key) {
  return Boolean(audit.releases[release]?.files?.[key.split(":")[0]]);
}

// Adjacent releases of the audit only; a method whose file one of the two releases has no source
// for is listed as unavailable, not as changed.
const breakpoints = [];
for (let index = 1; index < versions.length; index += 1) {
  const previous = versions[index - 1];
  const current = versions[index];
  const comparable = methods.filter((method) => hasFile(previous, method) && hasFile(current, method));
  const changed = comparable.filter((method) => hashes(previous, method) !== hashes(current, method));
  const unavailable = methods.filter((method) => !comparable.includes(method));
  if (changed.length > 0 || unavailable.length > 0) {
    breakpoints.push({
      previous,
      current,
      changed,
      ...(unavailable.length > 0 ? { unavailable } : {}),
    });
  }
}

const output = {
  schema: 2,
  sourceAudit: "generated/physics-methods.json",
  releases: versions,
  comparedMethods: methods,
  breakpoints,
};
const outputPath = path.join(root, "generated", "physics-breakpoints.json");
fs.writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`);
console.log(`Recorded ${breakpoints.length} source behavior breakpoints.`);

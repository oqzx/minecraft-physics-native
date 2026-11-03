// The releases the engine registers: native/src/versioned/mod.rs SUPPORTED_VERSIONS, each entry
// the `VERSION` of its v-<release>.rs module, in the registry's order. Shared by the generators and
// verifiers that have to cover every registered release.

const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");
const versionedRoot = path.join(repositoryRoot, "native", "src", "versioned");

/** `{ version, module, file }` of each registered release, in SUPPORTED_VERSIONS order. */
function registeredReleases(root = versionedRoot) {
  const registry = fs.readFileSync(path.join(root, "mod.rs"), "utf8");
  const list = /pub const SUPPORTED_VERSIONS: &\[&str\] = &\[([\s\S]*?)\];/.exec(registry);
  if (!list) throw new Error("native/src/versioned/mod.rs has no SUPPORTED_VERSIONS");
  const paths = new Map(
    [...registry.matchAll(/#\[path = "([^"]+)"\]\s*pub mod (\w+);/g)].map((match) => [match[2], match[1]]),
  );
  return [...list[1].matchAll(/(\w+)::Engine::VERSION/g)].map(([, module]) => {
    const file = paths.get(module);
    if (!file) throw new Error(`SUPPORTED_VERSIONS names ${module}, which mod.rs does not declare`);
    const source = fs.readFileSync(path.join(root, file), "utf8");
    const version = /const VERSION: &'static str = "([^"]+)";/.exec(source)?.[1];
    if (!version) throw new Error(`${file} declares no VERSION`);
    return { version, module, file };
  });
}

/** Release ids compared numerically part by part (1.21.10 after 1.21.9, 26.1 after 1.21.11). */
function compareReleases(left, right) {
  const leftParts = left.split(".").map(Number);
  const rightParts = right.split(".").map(Number);
  for (let index = 0; index < Math.max(leftParts.length, rightParts.length); index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

module.exports = { registeredReleases, compareReleases };

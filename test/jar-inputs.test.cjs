// The jar inputs of the oracle stages are pinned, and nothing about the host chooses among them.
const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const { NAMED_JARS, namedJarPath, PINNED_CLIENTS } = require("../scripts/source-pins.cjs");
const { libraryArtifacts, clientPin } = require("../scripts/java-oracle/classpath.cjs");
const { releases, versionJsonPins, versionJsonUrl } = require("../scripts/fetch-jar-inputs.cjs");
const { differences, format, parse } = require("../scripts/generated-hashes.cjs");

const root = path.resolve(__dirname, "..");
const generated = (name) => JSON.parse(fs.readFileSync(path.join(root, "generated", name), "utf8"));

test("every release before 26.1 has a pinned Mojang-named jar, and only those", () => {
  for (const version of releases()) assert.equal(Boolean(NAMED_JARS[version]), Number(version.split(".")[0]) < 26, version);
  assert.equal(Object.keys(NAMED_JARS).length, releases().filter((version) => !version.startsWith("26.")).length);
});

test("the shape tables record the named jar each pin names", () => {
  for (const [version, entry] of Object.entries(generated("block-shapes.json").releases)) {
    if (entry.status !== "exact") continue;
    assert.equal(entry.remappedSha256, NAMED_JARS[version], version);
    assert.equal(entry.librariesMissing, undefined, `${version}: a table must not depend on which libraries a host holds`);
  }
  for (const [version, pin] of Object.entries(PINNED_CLIENTS)) if (pin.remapped) assert.equal(pin.remapped, NAMED_JARS[version], version);
});

test("a legacy named jar is the one remapped by the mapping in the repository", () => {
  for (const version of ["1.8.8", "1.9.4", "1.10.2", "1.11.2", "1.12.2", "1.13.2"]) {
    // (as a Windows checkout holds it, CRLF, whatever this checkout's line endings: the jars were remapped there)
    const text = fs.readFileSync(path.join(root, "legacy-mappings", `${version}.tiny`), "latin1").replaceAll("\r\n", "\n").replaceAll("\n", "\r\n");
    const mapping = crypto.createHash("sha1").update(Buffer.from(text, "latin1")).digest("hex");
    assert.ok(namedJarPath(version).includes(`-mojang-${version}-${mapping.slice(0, 7)}/`), version);
  }
  assert.equal(namedJarPath("1.20.4"), "1.20.4/intermediary-1.20.4-mojmap-1.20.4/minecraft-1.20.4-fabric-merged+custom-no-environment+fixed-mojmap+intermediary.jar");
  assert.equal(namedJarPath("26.3"), "26.3/minecraft-26.3-client.jar");
  assert.ok(namedJarPath("1.12.2").includes("-mojang-1.12.2-6e0ec07/"));
});

test("every release's version json is pinned by Mojang's list, and the clients agree with the source pins", () => {
  const pins = versionJsonPins();
  for (const version of releases()) {
    assert.match(pins.get(version) ?? "", /^[0-9a-f]{40}$/, version);
    assert.equal(versionJsonUrl(version, pins.get(version)), `https://piston-meta.mojang.com/v1/packages/${pins.get(version)}/${version}.json`);
  }
  const entities = generated("entity-data.json").releases;
  for (const version of releases()) if (clientPin(version)) assert.equal(entities[version].client, clientPin(version), version);
});

test("library rules are read the same on every host", () => {
  const json = { libraries: [
    { name: "a", downloads: { artifact: { path: "a.jar", sha1: "1" } } },
    { name: "osx only", downloads: { artifact: { path: "b.jar", sha1: "2" } }, rules: [{ action: "allow", os: { name: "osx" } }] },
    { name: "not osx", downloads: { artifact: { path: "c.jar", sha1: "3" } }, rules: [{ action: "allow" }, { action: "disallow", os: { name: "osx" } }] },
    { name: "natives only", downloads: { classifiers: {} } },
  ] };
  assert.deepEqual(libraryArtifacts(json).map(({ name }) => name), ["a", "not osx"]);
});

test("the hash inventory round-trips and names each difference", () => {
  const a = new Map([["generated/x.json", "a".repeat(64)], ["native/data/y.rs", "b".repeat(64)]]);
  assert.deepEqual(parse(format(a)), a);
  const b = new Map([["generated/x.json", "c".repeat(64)], ["native/data/z.rs", "b".repeat(64)]]);
  assert.equal(differences(a, b).length, 3);
  assert.deepEqual(differences(a, a), []);
});

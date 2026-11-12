// Writes generated/release-aliases.json: the evidence behind every release served from another's
// sources. Both releases' clients, remapped to one namespace (intermediary), are compared class by
// class: first by bytes, then by disassembly with constant-pool numbers and code offsets removed, so
// a renumbered pool is not a change. A class that still differs must have a reviewed reason here, or
// the run fails. An alias holds only for the jars it names; --check (and test/release-aliases.test.cjs)
// repeats the comparison where the jars and a JDK are at hand, and the test checks the record everywhere.
// Pass --check to compare instead of writing.
const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { execFileSync } = require("node:child_process");
const { clientJarRoot } = require("./source-pins.cjs");
const { openZip } = require("./zip-read.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const outputPath = path.join(repositoryRoot, "generated", "release-aliases.json");

// what no alias may leave out: the record says each of these was compared
const REQUIRED_SCOPE = ["movement", "collision", "block-registry", "block-states", "fluids", "protocol-version"];

const ALIASES = {
  "1.8.8": {
    source: "1.8.9",
    clients: { requested: "0983f08be6a4e624f5d85689d1aca869ed99c738", source: "3870888a6c3d349d3771a3e9d16c9bf5e076b908" },
    protocol: 47,
    jars: {
      requested: "intermediary-1.8.8-yarn-1.8.8+build.604/minecraft-1.8.8-fabric-merged+custom-no-environment+fixed-intermediary.jar",
      source: "intermediary-1.8.9-yarn-1.8.9+build.541/minecraft-1.8.9-fabric-merged+custom-no-environment+fixed-intermediary.jar",
    },
    reviewed: {
      "net.minecraft.class_1$1": "version string in the crash report",
      "net.minecraft.class_1600": "MinecraftClient: window title and the version header of its HTTP requests",
      "net.minecraft.class_1809": "Realms screen proxy: the notification overlay",
      "net.minecraft.class_2110": "server handshake: the version named in its refusal text; the compared protocol stays 47",
      "net.minecraft.class_2294": "version string in a client overlay",
      "net.minecraft.class_347": "GameOptions: the realmsNotifications option",
      "net.minecraft.class_347$2": "GameOptions: the switch over the options",
      "net.minecraft.class_347$class_350": "GameOptions: the realmsNotifications constant",
      "net.minecraft.class_383": "options screen: the realmsNotifications button",
      "net.minecraft.class_388": "Screen: a resize helper for the notification overlay",
      "net.minecraft.class_485": "ServerInfo: the default version string",
      "net.minecraft.class_591": "an entity renderer: the order of two texture bindings",
      "net.minecraft.class_601": "integrated server: version string in a log line",
      "net.minecraft.class_624": "title screen: the Realms notification overlay",
      "net.minecraft.class_770": "dedicated server: version string in a log line",
      "net.minecraft.class_853": "snooper: the reported version string",
      "net.minecraft.realms.Realms": "Realms: the notification helpers",
      "net.minecraft.realms.RealmsBridge": "Realms: the notification screen",
      "net.minecraft.realms.RealmsScreen": "Realms: drawing helpers",
      "net.minecraft.realms.RealmsSharedConstants": "the version string",
      "net.minecraft.server.MinecraftServer": "the reported version string",
    },
  },
};

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const sha1 = (bytes) => crypto.createHash("sha1").update(bytes).digest("hex");

function javap() {
  const candidates = [process.env.JAVA_HOME && path.join(process.env.JAVA_HOME, "bin", "javap"), "javap",
    "C:/Program Files/Java/jdk-25.0.3/bin/javap.exe"].filter(Boolean);
  for (const candidate of candidates) {
    try {
      execFileSync(candidate, ["-version"], { stdio: "ignore" });
      return candidate;
    } catch {
      // the next one
    }
  }
  return null;
}

/** A class-keyed disassembly with what a recompile renumbers removed. */
function normalizedCode(tool, jar, classes) {
  const text = execFileSync(tool, ["-cp", jar, "-p", "-c", ...classes], { encoding: "latin1", maxBuffer: 2 ** 30, stdio: ["ignore", "pipe", "ignore"] });
  const code = new Map();
  let current = null;
  for (const line of text.split(/\r?\n/)) {
    const header = /^(?:public |final |abstract |synchronized )*(?:class|enum|interface) (\S+)/.exec(line);
    if (header) {
      current = header[1];
      code.set(current, []);
    }
    if (current) {
      code.get(current).push(line.replace(/^\s*\d+: /, "").replace(/#\d+(?:, *\d+)?/g, "#").replace(/\/\/ */, "")
        .replace(/\b(goto|if\w*)\s+\d+/, "$1").trim());
    }
  }
  return new Map([...code].map(([name, lines]) => [name, lines.join("\n")]));
}

/** The comparison of two jars: counts, and the classes whose code differs. */
function compareJars(requestedJar, sourceJar, tool) {
  const requested = openZip(requestedJar);
  const source = openZip(sourceJar);
  const classes = requested.names.filter((name) => name.endsWith(".class")).sort();
  const sourceClasses = source.names.filter((name) => name.endsWith(".class")).sort();
  if (JSON.stringify(classes) !== JSON.stringify(sourceClasses)) throw new Error("The class sets of the aliased jars differ");
  const differing = classes.filter((name) => sha256(requested.read(name)) !== sha256(source.read(name)));
  const names = differing.map((name) => name.replace(/\.class$/, "").replaceAll("/", "."));
  const left = normalizedCode(tool, requestedJar, names);
  const right = normalizedCode(tool, sourceJar, names);
  return {
    classes: classes.length,
    byteIdentical: classes.length - differing.length,
    codeIdentical: classes.length - names.filter((name) => left.get(name) !== right.get(name)).length,
    differing: names.filter((name) => left.get(name) !== right.get(name)).sort(),
  };
}

function aliasRecord(requested, alias, root = clientJarRoot(), tool = javap()) {
  const file = (release, name) => path.join(root, release, name);
  for (const [side, release] of [["requested", requested], ["source", alias.source]]) {
    const client = file(release, `minecraft-${release}-client.jar`);
    if (sha1(fs.readFileSync(client)) !== alias.clients[side]) throw new Error(`The client jar of ${release} is not the pinned one`);
  }
  const requestedJar = file(requested, alias.jars.requested);
  const sourceJar = file(alias.source, alias.jars.source);
  const comparison = compareJars(requestedJar, sourceJar, tool);
  const unreviewed = comparison.differing.filter((name) => !alias.reviewed[name]);
  if (unreviewed.length > 0) throw new Error(`${requested} -> ${alias.source}: no review of ${unreviewed.join(", ")}`);
  const record = {
    requested,
    source: alias.source,
    scope: REQUIRED_SCOPE,
    protocol: alias.protocol,
    clients: alias.clients,
    remapped: {
      namespace: "intermediary",
      requested: sha256(fs.readFileSync(requestedJar)),
      source: sha256(fs.readFileSync(sourceJar)),
    },
    classes: comparison.classes,
    byteIdentical: comparison.byteIdentical,
    codeIdentical: comparison.codeIdentical,
    differing: Object.fromEntries(comparison.differing.map((name) => [name, alias.reviewed[name]])),
  };
  return { ...record, evidence: evidenceHash(record) };
}

/** The hash an alias is cited by: the record without itself. */
function evidenceHash(record) {
  const { evidence: _evidence, ...rest } = record;
  return sha256(JSON.stringify(rest));
}

/** Why a recorded alias cannot be used, null when it can. */
function aliasDefect(record) {
  if (!record) return "no alias evidence";
  const left = REQUIRED_SCOPE.filter((entry) => !record.scope?.includes(entry));
  if (left.length > 0) return `the scope leaves out ${left.join(", ")}`;
  for (const field of ["requested", "source"]) {
    if (!/^[0-9a-f]{40}$/.test(record.clients?.[field] ?? "")) return `no client hash for the ${field} release`;
    if (!/^[0-9a-f]{64}$/.test(record.remapped?.[field] ?? "")) return `no remapped hash for the ${field} release`;
  }
  if (record.evidence !== evidenceHash(record)) return "the evidence hash does not match the record";
  return null;
}

function render() {
  const aliases = {};
  for (const [requested, alias] of Object.entries(ALIASES)) aliases[requested] = aliasRecord(requested, alias);
  return `${JSON.stringify({ schema: 1, generatedBy: "scripts/generate-release-aliases.cjs", aliases }, null, 2)}\n`;
}

if (require.main === module) {
  const text = render();
  if (process.argv.includes("--check")) {
    const current = fs.existsSync(outputPath) ? fs.readFileSync(outputPath, "utf8").replace(/\r\n/g, "\n") : "";
    if (current !== text) {
      console.error("generated/release-aliases.json is stale: run scripts/generate-release-aliases.cjs");
      process.exit(1);
    }
    console.log("generated/release-aliases.json is current.");
  } else {
    fs.writeFileSync(outputPath, text);
    console.log(`Compared ${Object.keys(JSON.parse(text).aliases).length} alias.`);
  }
}

module.exports = { ALIASES, REQUIRED_SCOPE, aliasRecord, aliasDefect, compareJars, evidenceHash, javap };

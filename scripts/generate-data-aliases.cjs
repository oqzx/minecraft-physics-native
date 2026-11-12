// Writes generated/data-aliases.json: the proof behind six exact releases of minecraft-data that read their tables, by
// path, from the release of the same protocol (B302, B029): 1.19.1 from 1.19.2, 1.21.2 from 1.21.3, 1.21.7 from 1.21.8,
// 1.21.10 from 1.21.9, 26.1.1 and 26.1.2 from 26.1. Each declares itself (its own version.json, protocol and name), so the
// adapter serves it on its own registry like any other release; nothing here is an alias at run time. Each pair rests on
// three comparisons of the real jars, and one of minecraft-data:
//
//   - the vanilla data generator of both servers (net.minecraft.data.Main --server --reports): every report (blocks and
//     their states, items, registries, packets, commands) and every data file (tags, recipes, biomes, dimensions) by hash;
//   - both clients, remapped to intermediary (26.x as it ships, unobfuscated), class by class. A class that differs or stands alone in one must have a
//     reviewed reason here, or the run fails: what the generator cannot show, as a block's collision shape, is in a class;
//   - both clients' assets/minecraft/lang/en_us.json, which neither of the above holds: where they differ (1.19.1) the
//     release must have its own language table, and in every case its table must be its own jar's;
//   - minecraft-data's dataPaths entry of the release: every table that of the pair's release, but its version.json and
//     a language that differs, with the jar's protocol.
//
// It says nothing of the physics, which each release has from its own sources. Pass --check to compare with the
// record instead of writing. MC_REPORTS_DIR keeps the generated trees between runs (they take half a minute a release).
//
// The jars are held to their pins before anything is compared (`jars` of each record): the client at the sha1 its release
// pins (scripts/java-oracle/classpath.cjs clientPin), the server at the sha1 the release's pinned version json gives
// (MC_VERSION_JSONS/<release>/version.json, default MC_CLIENT_JAR_ROOT, held to generated/mojang-releases.json), and the
// intermediary remap, which unimined makes and Mojang does not publish, at the sha256 the committed record holds.
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const crypto = require("node:crypto");
const { execFileSync } = require("node:child_process");
const { clientJarRoot } = require("./source-pins.cjs");
const { openZip } = require("./zip-read.cjs");
const { clientPin } = require("./java-oracle/classpath.cjs");
const { versionJsonPins } = require("./fetch-jar-inputs.cjs");

const outputPath = path.resolve(__dirname, "..", "generated", "data-aliases.json");
const JAVA = process.env.MC_JAVA ?? "C:/Program Files/Java/jdk-21.0.11/bin/java.exe";
const JAVA_26 = process.env.MC_JAVA_26 ?? "C:/Program Files/Java/jdk-25.0.3/bin/java.exe";
const unobfuscated = (release) => /^\d\d\./.test(release);

const VERSION_ONLY = {
  "net.minecraft.class_155": "SharedConstants: the version, data version and protocol constants",
  "net.minecraft.class_3797": "DetectedVersion: the version's name",
  "net.minecraft.class_7569": "ReportEnvironment: the client version in a chat report",
};

/** The classes each pair differs in, with what the difference is. No registry, block, item or shape class is among the first three. */
const ALIASES = {
  "1.19.1": {
    data: "1.19.2",
    protocol: 760,
    reviewed: {
      ...VERSION_ONLY,
      "net.minecraft.class_310": "Minecraft: the profile key manager",
      "net.minecraft.class_310$1": "Minecraft: a switch of the class",
      "net.minecraft.class_310$class_5859": "Minecraft.ChatStatus",
      "net.minecraft.class_310$class_5859$1": "Minecraft.ChatStatus",
      "net.minecraft.class_310$class_5859$2": "Minecraft.ChatStatus",
      "net.minecraft.class_310$class_5859$3": "Minecraft.ChatStatus",
      "net.minecraft.class_310$class_5859$4": "Minecraft.ChatStatus",
      "net.minecraft.class_3244": "ServerGamePacketListenerImpl: chat session keys",
      "net.minecraft.class_3244$1": "ServerGamePacketListenerImpl: a switch of the class",
      "net.minecraft.class_3244$2": "ServerGamePacketListenerImpl: a switch of the class",
      "net.minecraft.class_3248": "ServerLoginPacketListenerImpl: the key validation of the login",
      "net.minecraft.class_3248$1": "ServerLoginPacketListenerImpl: a thread of the class",
      "net.minecraft.class_3248$class_3249": "ServerLoginPacketListenerImpl.State",
      "net.minecraft.class_3248$class_7423": "ServerLoginPacketListenerImpl.PublicKeyValidationException, moved to ProfilePublicKey",
      "net.minecraft.class_412": "ConnectScreen: the profile key on connecting",
      "net.minecraft.class_412$1": "ConnectScreen: a thread of the class",
      "net.minecraft.class_4902": "RealmsConnect: the profile key on connecting",
      "net.minecraft.class_4902$1": "RealmsConnect: a thread of the class",
      "net.minecraft.class_5521": "SocialInteractionsPlayerList: the chat key of a player",
      "net.minecraft.class_640": "PlayerInfo: the chat key of a player",
      "net.minecraft.class_7428": "ProfilePublicKey: its validation",
      "net.minecraft.class_7428$class_7443": "ProfilePublicKey.Data",
      "net.minecraft.class_7428$class_7652": "ProfilePublicKey.ValidationException, 1.19.2's",
      "net.minecraft.class_7434": "ProfileKeyPairManager: the state of the key",
      "net.minecraft.class_7434$class_7653": "ProfileKeyPairManager.Result, 1.19.2's",
    },
  },
  "1.21.2": {
    data: "1.21.3",
    protocol: 768,
    reviewed: {
      ...VERSION_ONLY,
      "net.minecraft.class_10369": "EntitySalmonSizeFix, 1.21.3's: a datafix of saved salmon",
      "net.minecraft.class_1462$class_9869": "Salmon.Variant: the saved name; its sizes 0.5, 1 and 1.5 are the same in both",
      "net.minecraft.class_3551": "DataFixers: the salmon fix",
      "net.minecraft.class_3551$1": "DataFixers: the salmon fix",
      "net.minecraft.class_3551$2": "DataFixers: the salmon fix",
      "net.minecraft.class_4439": "GetServerDetailsTask: Realms",
    },
  },
  "1.21.7": {
    data: "1.21.8",
    protocol: 772,
    reviewed: {
      ...VERSION_ONLY,
      "net.minecraft.class_11541": "GraphicsWorkarounds, 1.21.8's: rendering",
      "com.mojang.blaze3d.vertex.VertexFormat": "rendering",
      "com.mojang.blaze3d.vertex.VertexFormat$Builder": "rendering",
      "com.mojang.blaze3d.vertex.VertexFormat$class_5595": "rendering",
      "com.mojang.blaze3d.vertex.VertexFormat$class_5596": "rendering",
      "net.minecraft.class_10869$class_10871": "VertexArrayCache: rendering",
      "net.minecraft.class_10869$class_10872": "VertexArrayCache: rendering",
      "net.minecraft.class_5944": "GlProgram: rendering",
      "net.minecraft.class_5944$1": "GlProgram: rendering",
    },
  },
  // 1.21.10's blocks are in this list for Block.entityInside's new argument and what reads it, by the two source trees'
  // diff, where no shape, property or registration changes: behavior, which the release module has from its own sources
  "1.21.10": {
    data: "1.21.9",
    protocol: 773,
    reviewed: {
      "net.minecraft.class_155": VERSION_ONLY["net.minecraft.class_155"],
      "net.minecraft.class_3797": VERSION_ONLY["net.minecraft.class_3797"],
      ...Object.fromEntries([
        "class_10374", "class_2231", "class_2258", "class_2266", "class_2269", "class_2302", "class_2313", "class_2329", "class_2334",
        "class_2377", "class_2423", "class_2538", "class_2553", "class_2560", "class_2563", "class_3830", "class_3922", "class_4622",
        "class_4770", "class_5553", "class_5556", "class_5635", "class_5801", "class_7113", "class_8237",
      ].map((name) => [`net.minecraft.${name}`, "a block's entityInside: its new argument (1.21.10 BlockBehaviour)"])),
      ...Object.fromEntries(["class_4970", "class_4970$class_2250", "class_4970$class_2251", "class_4970$class_4971", "class_4970$class_4971$class_3752"]
        .map((name) => [`net.minecraft.${name}`, "BlockBehaviour: entityInside's new argument; a synthetic of the nest"])),
      ...Object.fromEntries(["class_1297", "class_1297$1", "class_1297$class_11344", "class_1297$class_5529", "class_1297$class_5799"]
        .map((name) => [`net.minecraft.${name}`, "Entity: behavior, the release module's own"])),
      "net.minecraft.class_1530": "HangingEntity: behavior",
      "net.minecraft.class_1530$1": "HangingEntity: behavior",
      "net.minecraft.class_1533": "ItemFrame: behavior",
      "net.minecraft.class_1671": "FireworkRocketEntity: behavior",
      "net.minecraft.class_1682": "ThrowableProjectile: behavior",
      ...Object.fromEntries(["", "$2", "$class_6414", "$class_6414$1", "$class_6897", "$class_7460"]
        .map((name) => [`net.minecraft.server.MinecraftServer${name}`, "MinecraftServer: not data"])),
    },
  },
  // 26.x ships unobfuscated, so its clients are compared as they ship, with Mojang's names (javap diff of each class)
  "26.1.1": {
    data: "26.1",
    protocol: 775,
    reviewed: {
      "net.minecraft.DetectedVersion": VERSION_ONLY["net.minecraft.class_3797"],
      "net.minecraft.SharedConstants": "SharedConstants: the data version 4788 and the snapshot protocol number",
      "net.minecraft.client.gui.screens.social.PlayerEntry": "the social interactions screen: one inverted branch",
    },
  },
  "26.1.2": {
    data: "26.1",
    protocol: 775,
    reviewed: {
      "net.minecraft.DetectedVersion": VERSION_ONLY["net.minecraft.class_3797"],
      "net.minecraft.SharedConstants": "SharedConstants: the data version 4790 and the snapshot protocol number",
      "net.minecraft.client.gui.screens.social.PlayerEntry": "the social interactions screen: one inverted branch",
      "net.minecraft.client.gui.components.Checkbox": "rendering: a tooltip when the label overflows two rows",
      "net.minecraft.client.gui.components.Checkbox$Builder": "rendering: the same tooltip",
      "net.minecraft.client.gui.components.Checkbox$OnValueChange": "rendering: recompiled with Checkbox, no change of its code",
      "net.minecraft.client.gui.screens.reporting.AbstractReportScreen": "the report screen: the attestation checkbox's tooltip",
      "net.minecraft.client.gui.screens.reporting.AbstractReportScreen$DiscardReportWarningScreen": "the report screen: recompiled with it, no change of its code",
      "net.minecraft.server.network.ServerGamePacketListenerImpl": "handleAttack refuses a spectator: an attack packet, no registry, block or movement",
    },
  },
};

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");

/** The generated tree of a release: path -> hash, the generator's cache left out. */
function vanillaData(release, root) {
  const dir = path.join(root, release);
  const out = path.join(dir, "gen");
  if (!fs.existsSync(out)) {
    fs.mkdirSync(dir, { recursive: true });
    const server = serverJar(release);
    execFileSync(unobfuscated(release) ? JAVA_26 : JAVA, ["-DbundlerMainClass=net.minecraft.data.Main", "-jar", server, "--server", "--reports", "--output", "gen"], { cwd: dir, stdio: "ignore" });
  }
  const files = {};
  const walk = (at) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true })) {
      const file = path.join(at, entry.name);
      if (entry.isDirectory()) walk(file);
      else files[path.relative(out, file).split(path.sep).join("/")] = sha256(fs.readFileSync(file));
    }
  };
  walk(out);
  for (const name of Object.keys(files)) if (name.startsWith(".cache/")) delete files[name];
  return files;
}

const digest = (file, algorithm) => crypto.createHash(algorithm).update(fs.readFileSync(file)).digest("hex");
const clientJar = (release) => path.join(clientJarRoot(), release, `minecraft-${release}-client.jar`);
const serverJar = (release) => path.join(clientJarRoot(), release, `minecraft-${release}-server.jar`);
const intermediaryJar = (release) => path.join(clientJarRoot(), release, `intermediary-${release}-mojmap-${release}`,
  `minecraft-${release}-fabric-merged+custom-no-environment+fixed-intermediary.jar`);

/**
 * The jars of a release, each at its pin: `{ client, server, intermediary? }`. `recorded` is the committed record's, which
 * pins the intermediary remap; a jar off its pin throws, naming it, before it is read.
 */
function jarIdentity(release, recorded = null) {
  const fail = (what, file, actual, pinned) => {
    throw new Error(`${release}: the ${what} ${file} is ${actual}, pinned ${pinned ?? "nothing"}`);
  };
  const client = digest(clientJar(release), "sha1");
  if (client !== clientPin(release)) fail("client", clientJar(release), client, clientPin(release));
  const jsonFile = path.join(process.env.MC_VERSION_JSONS ?? clientJarRoot(), release, "version.json");
  if (!fs.existsSync(jsonFile)) throw new Error(`${release}: no version json at ${jsonFile}: set MC_VERSION_JSONS`);
  const jsonSha1 = digest(jsonFile, "sha1");
  if (jsonSha1 !== versionJsonPins().get(release)) fail("version json", jsonFile, jsonSha1, versionJsonPins().get(release));
  const serverPin = JSON.parse(fs.readFileSync(jsonFile, "utf8")).downloads.server.sha1;
  const server = digest(serverJar(release), "sha1");
  if (server !== serverPin) fail("server", serverJar(release), server, serverPin);
  if (unobfuscated(release)) return { client, server };
  const intermediary = digest(intermediaryJar(release), "sha256");
  if (recorded?.intermediary && intermediary !== recorded.intermediary) fail("intermediary remap", intermediaryJar(release), intermediary, recorded.intermediary);
  return { client, server, intermediary };
}

const treeHash = (files) => sha256(Object.keys(files).sort().map((name) => `${name} ${files[name]}\n`).join(""));

/** The two clients, in intermediary or as 26.x ships them: which classes differ or stand alone. */
function compareClients(requested, data) {
  const open = (release) => openZip(unobfuscated(release) ? clientJar(release) : intermediaryJar(release));
  const left = open(requested);
  const right = open(data);
  const classes = (zip) => new Set(zip.names.filter((name) => name.endsWith(".class")));
  const leftClasses = classes(left);
  const rightClasses = classes(right);
  const differing = [...leftClasses].filter((name) => (rightClasses.has(name) ? sha256(left.read(name)) !== sha256(right.read(name)) : true));
  const alone = [...rightClasses].filter((name) => !leftClasses.has(name));
  return {
    classes: leftClasses.size,
    differing: [...differing, ...alone].map((name) => name.replace(/\.class$/, "").replaceAll("/", ".")).sort(),
  };
}

/** The release's en_us.json from its client jar. */
const jarLanguage = (release) => JSON.parse(openZip(clientJar(release))
  .read("assets/minecraft/lang/en_us.json").toString("utf8"));

const differingKeys = (left, right) => [...new Set([...Object.keys(left), ...Object.keys(right)])].filter((key) => left[key] !== right[key]).sort();

/** Both jars' languages, and minecraft-data's table of the release held to its own jar. */
function compareLanguage(requested, data) {
  const own = jarLanguage(requested);
  const differing = differingKeys(own, jarLanguage(data));
  const table = require("minecraft-data")(requested).language;
  const wrong = differingKeys(own, table);
  if (wrong.length > 0) throw new Error(`${requested}: minecraft-data's language is not the jar's: ${wrong.slice(0, 5).join(", ")}`);
  return { keys: Object.keys(own).length, differing };
}

/** minecraft-data's entry of the release: its own declaration, every other table its pair's, but a language of its own where the jars differ. */
function compareMinecraftData(requested, alias, languageDiffers) {
  const paths = require("minecraft-data/minecraft-data/data/dataPaths.json").pc;
  const own = paths[requested];
  const pair = paths[alias.data];
  const ownTables = Object.keys({ ...own, ...pair }).filter((table) => own[table] !== pair[table]).sort();
  const expected = languageDiffers ? ["language", "version"] : ["version"];
  if (JSON.stringify(ownTables) !== JSON.stringify(expected)) throw new Error(`${requested}: minecraft-data's own tables are ${ownTables.join(", ")}, not ${expected.join(", ")}`);
  if (own.version !== `pc/${requested}` || (languageDiffers && own.language !== `pc/${requested}`)) throw new Error(`${requested}: minecraft-data's tables are not under its name`);
  const { version } = require("minecraft-data")(requested);
  if (version.minecraftVersion !== requested || version.version !== alias.protocol) throw new Error(`${requested}: minecraft-data declares ${version.minecraftVersion}, protocol ${version.version}`);
  return ownTables;
}

function aliasRecord(requested, alias, reportsRoot, recorded = null) {
  // every jar at its pin before any is read
  const jars = { [requested]: jarIdentity(requested, recorded?.jars?.[requested]), [alias.data]: jarIdentity(alias.data, recorded?.jars?.[alias.data]) };
  const files = vanillaData(requested, reportsRoot);
  const dataFiles = vanillaData(alias.data, reportsRoot);
  if (Object.keys(files).length === 0) throw new Error(`${requested}: the data generator wrote nothing`);
  const different = [...new Set([...Object.keys(files), ...Object.keys(dataFiles)])].filter((name) => files[name] !== dataFiles[name]);
  if (different.length > 0) throw new Error(`${requested} -> ${alias.data}: the vanilla data differs: ${different.slice(0, 5).join(", ")}`);
  const clients = compareClients(requested, alias.data);
  const unreviewed = clients.differing.filter((name) => !alias.reviewed[name]);
  if (unreviewed.length > 0) throw new Error(`${requested} -> ${alias.data}: no review of ${unreviewed.join(", ")}`);
  const stale = Object.keys(alias.reviewed).filter((name) => !clients.differing.includes(name));
  if (stale.length > 0) throw new Error(`${requested} -> ${alias.data}: a review of what does not differ: ${stale.join(", ")}`);
  const language = compareLanguage(requested, alias.data);
  const ownTables = compareMinecraftData(requested, alias, language.differing.length > 0);
  return {
    data: alias.data,
    protocol: alias.protocol,
    jars,
    vanillaDataFiles: Object.keys(files).length,
    vanillaData: treeHash(files),
    classes: clients.classes,
    differing: Object.fromEntries(clients.differing.map((name) => [name, alias.reviewed[name]])),
    languageKeys: language.keys,
    languageDiffering: language.differing,
    ownTables,
  };
}

function output(reportsRoot) {
  const committed = fs.existsSync(outputPath) ? JSON.parse(fs.readFileSync(outputPath, "utf8")).aliases ?? {} : {};
  const aliases = {};
  for (const [requested, alias] of Object.entries(ALIASES)) aliases[requested] = aliasRecord(requested, alias, reportsRoot, committed[requested]);
  return `${JSON.stringify({ schema: 1, generatedBy: "scripts/generate-data-aliases.cjs", aliases }, null, 2)}\n`;
}

if (require.main === module) {
  const reportsRoot = process.env.MC_REPORTS_DIR ?? fs.mkdtempSync(path.join(os.tmpdir(), "mc-reports-"));
  const text = output(reportsRoot);
  if (process.argv.includes("--check")) {
    if (fs.readFileSync(outputPath, "utf8").replace(/\r\n/g, "\n") !== text) {
      console.error("generated/data-aliases.json is stale: run scripts/generate-data-aliases.cjs");
      process.exitCode = 1;
    }
  } else {
    fs.writeFileSync(outputPath, text);
  }
  console.log(`${Object.keys(ALIASES).length} releases on their pair's tables compared.`);
}

module.exports = { ALIASES, jarIdentity };

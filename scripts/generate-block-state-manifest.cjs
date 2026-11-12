const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");
const { javaCode, matchingBracket, hoistStaticFields } = require("./java-source.cjs");
const { enumConstants } = require("./legacy-release-data.cjs");
const { stateResolver } = require("./block-state-fields.cjs");
const { modernTree, recordedSource, decompRoot: sourceRoot } = require("./source-roots.cjs");

const repositoryRoot = path.resolve(__dirname, "..");
const versionsPath = path.join(repositoryRoot, "generated", "minecraft-versions.json");
const outputPath = path.resolve(
  process.env.MC_BLOCK_STATE_MANIFEST
    ?? path.join(repositoryRoot, "generated", "block-state-definitions.json"),
);
const manifest = JSON.parse(fs.readFileSync(versionsPath, "utf8"));
const versionFamily = (version) => version.split(".").slice(0, 2).join(".");
const versionTree = (version) => modernTree(version) ?? path.join(sourceRoot, versionFamily(version), version);
const blockRoot = (version) => path.join(
  versionTree(version),
  "net",
  "minecraft",
  "world",
  "level",
  "block",
);

const propertyDeclaration = /\b(BooleanProperty|IntegerProperty|EnumProperty(?:<[^>]+>)?|DirectionProperty)\s+([A-Z][A-Z0-9_]*)\s*=/g;
const setValue = /\.setValue\(\s*([A-Z][A-Z0-9_]*)\s*,\s*([^)]+)\)/g;
const stateDefinitionAdd = /createBlockStateDefinition\s*\([^)]*\)\s*\{([\s\S]*?)\}/;
const integerCreate = /IntegerProperty\.create\(\s*"([a-z0-9_]+)"\s*,\s*(-?\d+)\s*,\s*(\d+)\s*\)/;
const enumDeclaration = /\benum\s+([A-Za-z0-9_]+)\s*(?:implements[^{]+)?\{([\s\S]*?)\}/;
const sharedDeclaration = /public\s+static\s+final\s+(BooleanProperty|IntegerProperty|EnumProperty<[^>]+>|DirectionProperty)\s+([A-Z][A-Z0-9_]*)\s*=\s*([\s\S]*?);/g;

function hash(filePath) {
  return crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

function sourceFiles(root) {
  const files = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const filePath = path.join(root, entry.name);
    if (entry.isDirectory()) files.push(...sourceFiles(filePath));
    else if (entry.isFile() && entry.name.endsWith(".java")) files.push(filePath);
  }
  return files;
}

const listings = new Map();

/**
 * The file of a class, by the directory's listing and not by asking the disk: 1.14.4's `class
 * SoulsandBlock` is in SoulSandBlock.java, which a case-insensitive disk finds under either name and
 * a case-sensitive one does not, so the result would depend on the host.
 */
function classFile(directory, className) {
  if (!listings.has(directory)) listings.set(directory, fs.existsSync(directory) ? fs.readdirSync(directory) : []);
  const wanted = `${className}.java`;
  const names = listings.get(directory);
  const name = names.find((entry) => entry === wanted) ?? names.find((entry) => entry.toLowerCase() === wanted.toLowerCase());
  return name ? path.join(directory, name) : null;
}

/** The block classes of a release, by file: `block/` and the pistons in `block/piston/`. */
function blockClassFiles(root) {
  return ["", "piston"]
    .map((directory) => path.join(root, directory))
    .filter((directory) => fs.existsSync(directory))
    .flatMap((directory) => fs.readdirSync(directory, { withFileTypes: true })
      .filter((entry) => entry.isFile() && entry.name.endsWith("Block.java"))
      .map((entry) => path.join(directory, entry.name)))
    .sort((left, right) => path.basename(left).localeCompare(path.basename(right)));
}

function canonicalDomain(type, name, source, sharedProperties, enumDomains) {
  const shared = sharedProperties[name];
  if (shared) return shared;
  if (type === "BooleanProperty") return { kind: "boolean", values: ["false", "true"], confidence: "source" };
  if (type === "DirectionProperty") {
    return { kind: "unknown", confidence: "unresolved" };
  }
  if (type === "IntegerProperty") {
    const declarations = [...source.matchAll(/IntegerProperty\.create\(\s*"([a-z0-9_]+)"\s*,\s*(-?\d+)\s*,\s*(\d+)\s*\)/g)];
    const declaration = declarations.find((candidate) => candidate[1] === name.toLowerCase())
      ?? declarations.find((candidate) => source.includes(`"${name.toLowerCase()}"`));
    if (declaration) {
      const min = Number(declaration[2]);
      const max = Number(declaration[3]);
      if (declaration[1] === name.toLowerCase() || source.includes(`"${name.toLowerCase()}"`)) {
        return {
          kind: "integer",
          min,
          max,
          values: Array.from({ length: max - min + 1 }, (_, index) => String(min + index)),
          confidence: "source",
        };
      }
    }
    const suffix = name.match(/(?:AGE|LAYERS?|DISTANCE|POWER|DELAY|BITES|HATCH|STAGE|LEVEL|MOISTURE|PICKLES?)_(\d+)$/);
    if (suffix) {
      const max = Number(suffix[1]);
      return {
        kind: "integer",
        min: 0,
        max,
        values: Array.from({ length: max + 1 }, (_, index) => String(index)),
        confidence: "canonical",
      };
    }
  }
  const enumMatch = enumDeclaration.exec(source);
  if (type === "EnumProperty" && enumMatch) {
    const values = enumMatch[2]
      .split(",")
      .map((value) => value.trim().match(/^([A-Z][A-Z0-9_]*)/)?.[1]?.toLowerCase())
      .filter(Boolean);
    if (values.length > 0) return { kind: "enum", values, confidence: "source" };
  }
  if (type === "EnumProperty") {
    const enumType = new RegExp(`\\bEnumProperty<([^>]+)>\\s+${name}\\b`).exec(source)?.[1]?.trim().split(".").at(-1);
    const values = enumType ? enumDomains[enumType] : undefined;
    if (values?.length) return { kind: "enum", values, enumType, confidence: "source-enum" };
  }
  return { kind: type === "IntegerProperty" ? "integer" : "unknown", confidence: "unresolved" };
}

function extractBlock(version, filePath, sharedProperties, enumDomains) {
  const source = hoistStaticFields(fs.readFileSync(filePath, "utf8"));
  const properties = {};
  for (const match of source.matchAll(propertyDeclaration)) {
    const type = match[1].startsWith("EnumProperty") ? "EnumProperty" : match[1];
    const sharedName = new RegExp(`\\b${match[2]}\\s*=\\s*BlockStateProperties\\.([A-Z][A-Z0-9_]*)\\b`).exec(source)?.[1];
    const domainProperties = sharedName && sharedProperties[sharedName]
      ? { [match[2]]: sharedProperties[sharedName] }
      : sharedProperties;
    properties[match[2]] = {
      type,
      domain: canonicalDomain(type, match[2], source, domainProperties, enumDomains),
      defaults: [],
    };
  }
  const defaultState = /registerDefaultState\s*\(([\s\S]*?)\);/.exec(source)?.[1] ?? "";
  for (const match of defaultState.matchAll(setValue)) {
    const property = properties[match[1]];
    if (property && !property.defaults.includes(match[2].trim())) {
      property.defaults.push(match[2].trim());
    }
  }
  const stateDefinition = stateDefinitionAdd.exec(source)?.[1] ?? "";
  const propertyOrder = [...stateDefinition.matchAll(/\.add\(([\s\S]*?)\)/g)]
    .flatMap((match) => match[1].split(",").map((name) => name.trim()))
    .filter((name) => properties[name]);
  return {
    source: recordedSource(version, path.dirname(outputPath), filePath),
    sha256: hash(filePath),
    properties,
    propertyOrder,
    stateCount: Object.values(properties).every((property) => property.domain.values?.length > 0)
      ? Object.values(properties).reduce((count, property) => count * property.domain.values.length, 1)
      : null,
    unresolvedProperties: Object.entries(properties)
      .filter(([, property]) => property.domain.confidence === "unresolved")
      .map(([name]) => name),
  };
}

// a release whose sources are absent keeps the entry they last produced, only with --cached
const cachedMode = process.argv.includes("--cached");
const previousReleases = fs.existsSync(outputPath)
  ? JSON.parse(fs.readFileSync(outputPath, "utf8")).releases ?? {}
  : {};
const releases = {};
const enumFilesOf = new Map();
for (const version of manifest.versions) {
  const root = blockRoot(version);
  const versionRoot = versionTree(version);
  if (!fs.existsSync(root) && previousReleases[version] && cachedMode) {
    releases[version] = previousReleases[version];
    continue;
  }
  if (!fs.existsSync(root)) throw new Error(`Missing block source root for ${version}: ${root}; run scripts/materialize-sources.cjs, or pass --cached to keep its entry`);
  const propertiesPath = path.join(root, "state", "properties", "BlockStateProperties.java");
  if (!fs.existsSync(propertiesPath)) {
    throw new Error(`Missing shared block property source for ${version}: ${propertiesPath}`);
  }
  const propertiesSource = hoistStaticFields(fs.readFileSync(propertiesPath, "utf8"));
  const sharedProperties = {};
  for (const match of propertiesSource.matchAll(sharedDeclaration)) {
    const type = match[1].startsWith("EnumProperty") ? "EnumProperty" : match[1];
    const declaration = match[3];
    let domain;
    if (type === "BooleanProperty") {
      domain = { kind: "boolean", values: ["false", "true"], confidence: "source-shared" };
    } else if (type === "IntegerProperty") {
      const integer = /create\(\s*"[^"]+"\s*,\s*(-?\d+)\s*,\s*(\d+)\s*\)/.exec(declaration);
      if (integer) {
        const min = Number(integer[1]);
        const max = Number(integer[2]);
        domain = {
          kind: "integer",
          min,
          max,
          values: Array.from({ length: max - min + 1 }, (_, index) => String(min + index)),
          confidence: "source-shared",
        };
      }
      const rotationSource = /RotationSegment\.getMaxSegmentIndex\(\)/.test(declaration)
        ? fs.readFileSync(path.join(root, "state", "properties", "RotationSegment.java"), "utf8")
        : "";
      const precision = /SegmentedAnglePrecision\((\d+)\)/.exec(rotationSource)?.[1];
      const literalRotationMax = /MAX_SEGMENT_INDEX\s*=\s*(\d+)/.exec(rotationSource)?.[1];
      if (precision || literalRotationMax) {
        const max = literalRotationMax
          ? Number(literalRotationMax)
          : (2 ** Number(precision)) - 1;
        domain = {
          kind: "integer",
          min: 0,
          max,
          values: Array.from({ length: max + 1 }, (_, index) => String(index)),
          confidence: "source-shared-derived",
        };
      }
    } else if (type === "DirectionProperty") {
      const directions = [...declaration.matchAll(/\bDirection\.([A-Z]+)\b/g)]
        .map((direction) => direction[1].toLowerCase());
      if (declaration.includes("Plane.HORIZONTAL")) {
        directions.push("north", "east", "south", "west");
      }
      if (declaration.includes("var0 -> var0 != Direction.UP")) {
        directions.push("down", "north", "south", "west", "east");
      }
      domain = {
        kind: "enum",
        values: directions.length > 0
          ? [...new Set(directions)]
          : [],
        confidence: directions.length > 0 ? "source-shared" : "unresolved",
      };
    }
    if (!domain && type === "EnumProperty") {
      domain = {
        kind: "enum",
        enumType: match[1].slice("EnumProperty<".length, -1).trim().split(".").at(-1),
        confidence: "source-shared",
      };
    }
    if (domain) sharedProperties[match[2]] = domain;
  }
  const enumDomains = {};
  const enumFiles = new Map();
  enumFilesOf.set(version, enumFiles);
  for (const filePath of sourceFiles(versionRoot)) {
    const source = fs.readFileSync(filePath, "utf8");
    for (const match of source.matchAll(/\benum\s+([A-Za-z0-9_]+)\s*(?:implements[^{]+)?\{([\s\S]*?)\}/g)) {
      if (!enumFiles.has(match[1]) || filePath.includes(`properties${path.sep}`)) enumFiles.set(match[1], filePath);
      const values = match[2].split(",")
        .map((value) => value.trim().match(/^([A-Z][A-Z0-9_]*)/)?.[1]?.toLowerCase())
        .filter(Boolean);
      if (values.length > 0) enumDomains[match[1]] = values;
    }
  }
  if (!enumDomains.Axis) {
    const axisValues = [...new Set(sourceFiles(versionRoot)
      .flatMap((filePath) => [...fs.readFileSync(filePath, "utf8").matchAll(/\bAxis\.([XYZ])\b/g)]
        .map((match) => match[1].toLowerCase())))];
    if (axisValues.length > 0) enumDomains.Axis = axisValues;
  }
  for (const property of Object.values(sharedProperties)) {
    if (property.enumType && enumDomains[property.enumType]) {
      property.values = enumDomains[property.enumType];
      property.confidence = "source-shared-enum";
    }
  }
  const blocks = {};
  for (const filePath of blockClassFiles(root)) {
    blocks[path.basename(filePath, ".java")] = extractBlock(version, filePath, sharedProperties, enumDomains);
  }
  if (Object.keys(blocks).length === 0) throw new Error(`No block classes found for ${version}`);
  releases[version] = {
    sourceRoot: recordedSource(version, path.dirname(outputPath), root),
    sharedPropertiesSource: recordedSource(version, path.dirname(outputPath), propertiesPath),
    sharedPropertiesSha256: hash(propertiesPath),
    sharedPropertyCount: Object.keys(sharedProperties).length,
    blocks,
    blockClassCount: Object.keys(blocks).length,
    unresolvedPropertyCount: Object.values(blocks)
      .reduce((total, block) => total + block.unresolvedProperties.length, 0),
  };
}

// A release with its sources takes every block's definition from them: what createBlockStateDefinition
// adds, through its superclasses, each property as its field creates it. A release without them
// keeps its entry untouched, so a run on any host leaves it as the last run with sources did.
const registries = JSON.parse(fs.readFileSync(path.join(path.dirname(versionsPath), "block-registries.json"), "utf8")).versions;
for (const [version, release] of Object.entries(releases)) {
  if (!modernTree(version)) continue;
  const root = blockRoot(version);
  const enumFiles = enumFilesOf.get(version);
  const classFiles = new Map();
  const readClass = (className) => {
    if (classFiles.has(className)) return classFiles.get(className);
    const file = ["", "piston", "state", "state/properties"]
      .map((directory) => classFile(path.join(root, directory), className))
      .find(Boolean);
    const text = file ? fs.readFileSync(file, "utf8") : null;
    classFiles.set(className, text);
    return text;
  };
  const enumCache = new Map();
  const enumConstantsOf = (enumType) => {
    if (!enumCache.has(enumType)) {
      const file = enumFiles.get(enumType);
      const code = file ? javaCode(fs.readFileSync(file, "utf8")) : "";
      const declaration = new RegExp(String.raw`\benum\s+${enumType}\b[^{]*\{`).exec(code);
      if (!declaration) {
        enumCache.set(enumType, null);
      } else {
        const open = declaration.index + declaration[0].length - 1;
        const body = fs.readFileSync(file, "utf8").slice(open + 1, matchingBracket(code, open));
        enumCache.set(enumType, enumConstants({ body }));
      }
    }
    return enumCache.get(enumType);
  };
  const resolver = stateResolver({ readClass, enumConstantsOf });
  for (const className of new Set(Object.values(registries[version].classes).map((entry) => entry.className).filter(Boolean))) {
    if (release.blocks[className] || className === "Block" || !readClass(className)) continue;
    const file = classFile(root, className) ?? path.join(root, `${className}.java`);
    release.blocks[className] = { source: recordedSource(version, path.dirname(outputPath), file), sha256: hash(file), properties: {}, propertyOrder: [], stateCount: 1, unresolvedProperties: [] };
  }
  const declared = new Map(Object.entries(release.blocks).map(([className, block]) => [className, block.properties]));
  for (const [className, block] of Object.entries(release.blocks)) {
    const { properties, dynamic } = resolver.definition(className);
    // a default is set by the class that registers it, or by an ancestor for its own property
    const defaultsOf = (name) => resolver.lineage(className)
      .map((owner) => declared.get(owner)?.[name]?.defaults)
      .find((defaults) => defaults?.length > 0) ?? [];
    block.properties = Object.fromEntries(properties.map(({ name, domain }) => [name, {
      type: { boolean: "BooleanProperty", integer: "IntegerProperty", enum: "EnumProperty" }[domain?.kind] ?? "unknown",
      domain: domain ?? { kind: "unknown", confidence: "unresolved" },
      defaults: defaultsOf(name),
    }]));
    block.propertyOrder = properties.map(({ name }) => name);
    block.unresolvedProperties = [...properties.filter(({ domain }) => !domain).map(({ name }) => name), ...(dynamic ? ["<dynamic>"] : [])];
    block.stateCount = block.unresolvedProperties.length > 0
      ? null
      : properties.reduce((count, { domain }) => count * domain.values.length, 1);
    delete block.inheritedFrom;
  }
  release.blockClassCount = Object.keys(release.blocks).length;
  release.unresolvedPropertyCount = Object.values(release.blocks)
    .reduce((total, block) => total + block.unresolvedProperties.length, 0);
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify({
  schema: 1,
  sourceManifest: path.relative(path.dirname(outputPath), versionsPath).replaceAll(path.sep, "/"),
  releases,
}, null, 2)}\n`);

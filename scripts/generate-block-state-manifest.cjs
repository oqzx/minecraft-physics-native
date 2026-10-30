const fs = require("node:fs");
const path = require("node:path");
const crypto = require("node:crypto");

const repositoryRoot = path.resolve(__dirname, "..");
const versionsPath = path.join(repositoryRoot, "generated", "minecraft-versions.json");
const outputPath = path.resolve(
  process.env.MC_BLOCK_STATE_MANIFEST
    ?? path.join(repositoryRoot, "generated", "block-state-definitions.json"),
);
const manifest = JSON.parse(fs.readFileSync(versionsPath, "utf8"));
const sourceRoot = path.resolve(path.dirname(versionsPath), manifest.sourceRoot);
const versionFamily = (version) => version.split(".").slice(0, 2).join(".");
const blockRoot = (version) => path.join(
  sourceRoot,
  versionFamily(version),
  version,
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

function extractBlock(filePath, sharedProperties, enumDomains) {
  const source = fs.readFileSync(filePath, "utf8");
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
    source: path.relative(path.dirname(outputPath), filePath).replaceAll(path.sep, "/"),
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

const releases = {};
for (const version of manifest.versions) {
  const root = blockRoot(version);
  const versionRoot = path.join(sourceRoot, versionFamily(version), version);
  if (!fs.existsSync(root)) throw new Error(`Missing block source root for ${version}: ${root}`);
  const propertiesPath = path.join(root, "state", "properties", "BlockStateProperties.java");
  if (!fs.existsSync(propertiesPath)) {
    throw new Error(`Missing shared block property source for ${version}: ${propertiesPath}`);
  }
  const propertiesSource = fs.readFileSync(propertiesPath, "utf8");
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
  for (const filePath of sourceFiles(versionRoot)) {
    const source = fs.readFileSync(filePath, "utf8");
    for (const match of source.matchAll(/\benum\s+([A-Za-z0-9_]+)\s*(?:implements[^{]+)?\{([\s\S]*?)\}/g)) {
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
  const files = fs.readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith("Block.java"))
    .sort((left, right) => left.name.localeCompare(right.name));
  for (const entry of files) {
    blocks[entry.name.slice(0, -".java".length)] = extractBlock(
      path.join(root, entry.name),
      sharedProperties,
      enumDomains,
    );
  }
  if (Object.keys(blocks).length === 0) throw new Error(`No block classes found for ${version}`);
  releases[version] = {
    sourceRoot: path.relative(path.dirname(outputPath), root).replaceAll(path.sep, "/"),
    sharedPropertiesSource: path.relative(path.dirname(outputPath), propertiesPath).replaceAll(path.sep, "/"),
    sharedPropertiesSha256: hash(propertiesPath),
    sharedPropertyCount: Object.keys(sharedProperties).length,
    blocks,
    blockClassCount: Object.keys(blocks).length,
    unresolvedPropertyCount: Object.values(blocks)
      .reduce((total, block) => total + block.unresolvedProperties.length, 0),
  };
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify({
  schema: 1,
  sourceManifest: path.relative(path.dirname(outputPath), versionsPath).replaceAll(path.sep, "/"),
  releases,
}, null, 2)}\n`);

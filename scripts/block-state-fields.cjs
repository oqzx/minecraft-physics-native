// A block class's state definition as the source declares it: the arguments of the `.add(...)` in
// its createBlockStateDefinition, resolved through its superclasses and through the fields they
// name to the BooleanProperty, IntegerProperty, EnumProperty or DirectionProperty created there,
// the filter of a filtered enum included. A definition built in a loop or through a method call
// is reported as `dynamic`, and a field that does not resolve as unresolved: neither is guessed. Two
// loops have one meaning and are read: a list field's forEach, and MultifaceBlock's loop over the
// directions, whose isFaceSupported is checked in the leaf's whole lineage.
const { javaCode, matchingBracket, splitTopLevel } = require("./java-source.cjs");
const { enumConstants } = require("./legacy-release-data.cjs");

/** The initializers of a class's fields, at the declaration or in a static block: name -> text. */
function fieldInitializers(source) {
  const code = javaCode(source);
  const fields = new Map();
  const pattern = /^[ \t]+(?:(?:public|protected|private|static|final)\s+)*(?:[\w$.<>?, [\]]+?\s+)?([A-Za-z_$][\w$]*)\s*=(?!=)/gm;
  for (const match of code.matchAll(pattern)) {
    let depth = 0;
    let end = match.index + match[0].length;
    for (; end < code.length; end += 1) {
      if ("({[".includes(code[end])) depth += 1;
      else if (")}]".includes(code[end])) depth -= 1;
      else if (code[end] === ";" && depth === 0) break;
    }
    if (!fields.has(match[1])) fields.set(match[1], source.slice(match.index + match[0].length, end).trim());
  }
  // a decompile (1.18.1) that left a field obfuscated says what it was: `// $FF: renamed from: UP`
  for (const [, name, field] of source.matchAll(/\/\/ \$FF: renamed from: (\w+) [^\n]*\n\s*(?:[\w.<>?\[\]]+\s+)*(field_\d+)\s*;/g)) {
    if (fields.has(field) && !fields.has(name)) fields.set(name, fields.get(field));
  }
  return fields;
}

const uncast = (text) => text.replace(/^(?:\([\w$.<>?[\]]+\)\s*(?!\s*->))+/, "").trim();

/** The parts of a `createBlockStateDefinition`: what it adds, whether it calls super, or null. */
function stateDefinitionOf(source) {
  const code = javaCode(source);
  const declaration = /\bvoid\s+createBlockStateDefinition\s*\([^)]*\)\s*\{/.exec(code);
  if (!declaration) return null;
  const open = declaration.index + declaration[0].length - 1;
  const close = matchingBracket(code, open);
  const body = source.slice(open + 1, close);
  const bodyCode = code.slice(open + 1, close);
  const added = [];
  // a list directly, or through the local a decompiler copied it to
  const lists = [...bodyCode.matchAll(/\b([A-Z][A-Z0-9_]*)\.forEach\(/g)].map((match) => match[1]);
  for (const local of bodyCode.matchAll(/\b(\w+)\s*=\s*([A-Z][A-Z0-9_]*)\s*;/g)) {
    if (new RegExp(String.raw`\b${local[1]}\.forEach\(`).test(bodyCode)) lists.push(local[2]);
  }
  // for each of DIRECTIONS, however the loop was decompiled, add the face if it is supported
  const faces = /\bDIRECTIONS\b/.test(bodyCode) && (bodyCode.match(/\bfor\s*\(/g) ?? []).length === 1
    && /\bif\s*\(\s*this\.isFaceSupported\((\w+)\)\s*\)\s*\{?\s*\w+\.add\(\s*getFaceProperty\(\1\)\s*\)/.test(bodyCode);
  const read = (lists.length > 0 ? bodyCode.replaceAll(".forEach(", "(") : bodyCode).replace(faces ? /\bfor\s*\(/ : /$^/, "(");
  let dynamic = /\bfor\s*\(|\.forEach\(|\.stream\(/.test(read);
  for (const match of bodyCode.matchAll(/\.add\(/g)) {
    const start = match.index + match[0].length - 1;
    for (const argument of splitTopLevel(body.slice(start + 1, matchingBracket(bodyCode, start)))) {
      const reference = uncast(argument);
      if (faces && /getFaceProperty/.test(reference)) continue;
      if (lists.length > 0 && /^[a-z]\w*$|^xva\$\d+$/.test(reference)) continue;
      if (/^(?:[A-Z]\w*\.)*(?:[A-Z]|field_)[\w$]*(?:\[\d+\])?$/.test(reference) || /^(?:this\.)?\w+\(\)$/.test(reference)) added.push(reference);
      else dynamic = true;
    }
  }
  return { added, lists, faces, callsSuper: /\bsuper\.createBlockStateDefinition\b/.test(bodyCode), dynamic };
}

const parentOf = (className, source) => new RegExp(
  String.raw`\bclass\s+${className}\b(?:<[^{]*?>)?\s+extends\s+([A-Za-z0-9_]+)`,
).exec(javaCode(source))?.[1] ?? null;

/** The interfaces a class names (CaveVines holds BERRIES for the blocks that implement it). */
const interfacesOf = (className, source) => {
  const header = new RegExp(String.raw`\b(?:class|interface)\s+${className}\b([^{]*)\{`).exec(javaCode(source))?.[1] ?? "";
  const list = /\b(?:implements|extends)\s+([^{]*)$/.exec(header.replace(/^.*?\bimplements\b/s, "implements"))?.[1] ?? "";
  return list.split(",").map((name) => name.trim().split(/[<\s]/)[0].split(".").at(-1)).filter(Boolean);
};

const booleanDomain = () => ({ kind: "boolean", values: ["false", "true"], confidence: "source" });
const integerDomain = (min, max) => ({
  kind: "integer",
  min,
  max,
  values: Array.from({ length: max - min + 1 }, (_, index) => String(min + index)),
  confidence: "source",
});

/** The constants an EnumProperty.create's arguments after the class keep, null if unreadable. */
function selectedConstants(constants, enumType, args) {
  if (args.length === 0) return constants;
  const named = (text) => constants.find((constant) => constant.name === text.split(".").at(-1));
  const only = uncast(args[0]);
  // a decompiler's `(Enum[])(A, B)` for the varargs
  if (args.length === 1 && /^\(Enum\[\]\)\s*\(|^\([\w.]+\[\]\)\s*\(/.test(args[0])) {
    const open = args[0].indexOf("(", args[0].indexOf(")"));
    const listed = splitTopLevel(args[0].slice(open + 1, matchingBracket(javaCode(args[0]), open))).map((argument) => named(uncast(argument)));
    return listed.every(Boolean) ? listed : null;
  }
  if (args.length === 1 && /\.values\(\)$/.test(only)) return constants;
  if (args.length === 1 && /\bPlane\.HORIZONTAL$/.test(only)) {
    return ["north", "east", "south", "west"].map((value) => constants.find((constant) => constant.value === value));
  }
  const lambda = /^\(*(?:\w+\s+)?(\w+)\)*\s*->\s*\(*([\s\S]*)$/.exec(only);
  if (args.length === 1 && lambda) {
    const body = lambda[2].replace(/^\{\s*return\s+/, "").replace(/;\s*\}\s*\)*$/, "").replace(/\)+$/, "").trim();
    const terms = body.split(/\s*(&&|\|\|)\s*/);
    const conditions = terms.filter((_, index) => index % 2 === 0);
    const joiners = new Set(terms.filter((_, index) => index % 2 === 1));
    if (joiners.size > 1) return null;
    const tests = conditions.map((condition) => /^\w+\s*(==|!=)\s*(\S+)$/.exec(condition.replace(/[()]/g, "")));
    if (tests.some((test) => !test || !named(test[2]))) return null;
    const keeps = (constant) => {
      const results = tests.map(([, operator, other]) => (operator === "==") === (named(other) === constant));
      return joiners.has("||") ? results.some(Boolean) : results.every(Boolean);
    };
    return constants.filter(keeps);
  }
  const listed = args.map((argument) => named(uncast(argument)));
  return listed.every(Boolean) ? listed : null;
}

/**
 * Resolves class definitions of one release. `readClass(name)` gives a class's source or null,
 * `enumConstantsOf(type)` the `{ name, value }` constants of an enum or null.
 */
function stateResolver({ readClass, enumConstantsOf }) {
  const fieldCache = new Map();
  const fieldsOf = (className) => {
    if (!fieldCache.has(className)) {
      const source = readClass(className);
      fieldCache.set(className, source ? fieldInitializers(source) : new Map());
    }
    return fieldCache.get(className);
  };

  /** The constants of an enum declared inside a class, not one of the same name elsewhere. */
  function nestedEnum(className, enumType) {
    const source = className ? readClass(className) : null;
    if (!source) return null;
    const code = javaCode(source);
    const declaration = new RegExp(String.raw`\benum\s+${enumType}\b[^{]*\{`).exec(code);
    if (!declaration) return null;
    const open = declaration.index + declaration[0].length - 1;
    return enumConstants({ body: source.slice(open + 1, matchingBracket(code, open)) });
  }

  function integer(text, className, seen) {
    const value = uncast(text);
    if (/^-?\d+$/.test(value)) return Number(value);
    if (/^\d+\s*-\s*1$/.test(value)) return Number(value.split("-")[0]) - 1;
    const named = /^(?:(\w+)\.)?([A-Z_$][\w$]*)$/.exec(value);
    if (named) {
      const initializer = resolveInitializer(named[1] ?? className, named[2], seen);
      return initializer ? integer(initializer.text, initializer.owner, seen) : null;
    }
    return /\bgetMaxSegmentIndex\(\)/.test(value) ? 15 : null;
  }

  /** The field's initializer, looked for in the class and its ancestors. */
  function resolveInitializer(className, field, seen) {
    const key = `${className}.${field}`;
    if (seen.has(key)) return null;
    seen.add(key);
    const pending = [className];
    const visited = new Set();
    while (pending.length > 0) {
      const owner = pending.shift();
      if (visited.has(owner)) continue;
      visited.add(owner);
      const text = fieldsOf(owner).get(field);
      if (text !== undefined) return { text, owner };
      const source = readClass(owner);
      if (!source) continue;
      const parent = parentOf(owner, source);
      if (parent) pending.push(parent);
      pending.push(...interfacesOf(owner, source));
    }
    return null;
  }

  function domainOf(text, owner, seen) {
    const value = uncast(text);
    const reference = /^(?:((?:[A-Z]\w*\.)*[A-Z]\w*)\.)?([A-Za-z_$][\w$]*)$/.exec(value);
    if (reference) {
      const initializer = resolveInitializer(reference[1]?.split(".").at(-1) ?? owner, reference[2], seen);
      return initializer ? domainOf(initializer.text, initializer.owner, seen) : null;
    }
    const code = javaCode(value);
    const call = /^(BooleanProperty|IntegerProperty|EnumProperty|DirectionProperty)\s*\.\s*(?:<[^>]*>\s*)?create\s*\(/.exec(code);
    if (!call) return null;
    const open = call[0].length - 1;
    const args = splitTopLevel(value.slice(open + 1, matchingBracket(code, open)));
    if (call[1] === "BooleanProperty") return booleanDomain();
    if (call[1] === "IntegerProperty") {
      const [min, max] = [integer(args[1], owner, new Set()), integer(args[2], owner, new Set())];
      return min === null || max === null ? null : integerDomain(min, max);
    }
    const hasClass = /\.class$/.test(args[1] ?? "");
    const enumType = call[1] === "DirectionProperty"
      ? "Direction"
      : args[1]?.replace(/\.class$/, "").split(".").at(-1);
    const qualifier = args[1]?.replace(/\.class$/, "").split(".").at(-2);
    const constants = enumType
      ? nestedEnum(qualifier, enumType) ?? nestedEnum(owner, enumType) ?? enumConstantsOf(enumType)
      : null;
    if (!constants) return null;
    const selected = selectedConstants(constants, enumType, args.slice(hasClass ? 2 : 1));
    if (!selected) return null;
    return { kind: "enum", enumType, values: selected.map((constant) => constant.value), confidence: "source" };
  }

  /** The field a virtual accessor returns, the most derived override first. */
  function accessorField(method, leaf) {
    const pending = [leaf];
    while (pending.length > 0) {
      const owner = pending.shift();
      const source = readClass(owner);
      if (!source) continue;
      const body = new RegExp(String.raw`\b${method}\s*\(\s*\)\s*\{\s*return\s+([\w.]+)\s*;`).exec(javaCode(source));
      if (body) return { reference: body[1], owner };
      const parent = parentOf(owner, source);
      if (parent) pending.push(parent);
      pending.push(...interfacesOf(owner, source));
    }
    return null;
  }

  /** A reference of an `.add(...)`: `{ name, domain }`, an array element by its element's name. */
  function property(reference, className, leaf) {
    const accessor = /^(?:this\.)?(\w+)\(\)$/.exec(reference);
    if (accessor) {
      const found = accessorField(accessor[1], leaf);
      if (!found) return { name: reference, domain: null };
      return { name: found.reference.split(".").at(-1), domain: domainOf(found.reference, found.owner, new Set()) };
    }
    const element = /^(.*)\[(\d+)\]$/.exec(reference);
    if (element) {
      const array = resolveInitializer(className, element[1].split(".").at(-1), new Set());
      const items = array && /\{([\s\S]*)\}/.exec(array.text)?.[1];
      const item = items && splitTopLevel(items)[Number(element[2])];
      if (!item) return { name: reference, domain: null };
      return { name: uncast(item).split(".").at(-1), domain: domainOf(item, array.owner, new Set()) };
    }
    const name = reference.split(".").at(-1);
    return { name, domain: domainOf(reference, className, new Set()) };
  }

  const definitions = new Map();
  /** `{ properties: [{ name, domain }], dynamic, complete }` of a class and its ancestors. */
  function definition(className, leaf = className) {
    const key = `${leaf}\0${className}`;
    if (definitions.has(key)) return definitions.get(key);
    const source = readClass(className);
    if (!source) {
      const none = { properties: [], dynamic: false, complete: className === "Block" || className === "BlockBehaviour" };
      definitions.set(key, none);
      return none;
    }
    const parent = parentOf(className, source);
    const own = stateDefinitionOf(source);
    const inherited = parent ? definition(parent, leaf) : { properties: [], dynamic: false, complete: true };
    if (!own) {
      definitions.set(key, inherited);
      return inherited;
    }
    const base = own.callsSuper ? inherited : { properties: [], dynamic: false, complete: true };
    const properties = [...base.properties];
    const add = (reference, owner = className) => {
      const resolved = property(reference, owner, leaf);
      if (!properties.some((known) => known.name === resolved.name)) properties.push(resolved);
    };
    let dynamic = base.dynamic || own.dynamic;
    for (const reference of own.added) add(reference);
    for (const list of own.lists) {
      // at its declaration, or in a static block where a decompiler moved it
      const code = javaCode(source);
      const assignment = new RegExp(String.raw`\b${list}\s*=\s*(?:List|ImmutableList)\.of\(`).exec(code);
      if (assignment) {
        const open = assignment.index + assignment[0].length - 1;
        splitTopLevel(source.slice(open + 1, matchingBracket(code, open))).forEach((member) => add(uncast(member)));
      } else {
        dynamic = true;
      }
    }
    if (own.faces) {
      // a face a class rules out is a body other than `return true`
      const ruledOut = lineage(leaf).some((owner) => {
        const body = /\bisFaceSupported\s*\([^)]*\)\s*\{([^}]*)\}/.exec(javaCode(readClass(owner) ?? ""))?.[1];
        return body !== undefined && !/^\s*return\s+true\s*;\s*$/.test(body);
      });
      if (ruledOut) dynamic = true;
      else for (const face of ["DOWN", "UP", "NORTH", "SOUTH", "WEST", "EAST"]) add(`BlockStateProperties.${face}`);
    }
    const result = { properties, dynamic, complete: base.complete };
    definitions.set(key, result);
    return result;
  }

  /** A class and the ancestors of it that have sources, nearest first. */
  function lineage(className) {
    const chain = [];
    for (let current = className; current && !chain.includes(current);) {
      chain.push(current);
      const source = readClass(current);
      current = source ? parentOf(current, source) : null;
    }
    return chain;
  }

  return { definition, lineage };
}

module.exports = { stateResolver, fieldInitializers, stateDefinitionOf, enumConstants };

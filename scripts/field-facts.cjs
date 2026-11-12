// What the sources themselves say of each public field (B209, B271): its Rust type, what the binding reads when it is
// absent, where it narrows a double, and which adapter expression fills it and which takes it back. Everything here is
// read from native/src/napi_*.rs and src/mineflayer, so docs/field-catalogue.md and docs/field-maps.md move with the code.
const fs = require("node:fs");
const path = require("node:path");
const ts = require("typescript");

const root = path.resolve(__dirname, "..");
const camel = (name) => name.replace(/_(\w)/g, (_, letter) => letter.toUpperCase());
const snake = (name) => name.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
const bindingFiles = () => fs.readdirSync(path.join(root, "native", "src")).filter((file) => /^napi_.*\.rs$/.test(file));

/** interface -> field -> Rust type, from each #[napi(object)] struct. */
function rustTypes() {
  const types = {};
  for (const file of bindingFiles()) {
    const text = fs.readFileSync(path.join(root, "native", "src", file), "utf8").replace(/\r\n/g, "\n");
    for (const [, name, body] of text.matchAll(/#\[napi\(object[^\]]*\)\]\s*(?:#\[[^\]]*\]\s*)*pub struct (\w+)\s*\{([\s\S]*?)\n\}/g)) {
      const fields = {};
      for (const [, attribute, field, type] of body.matchAll(/(?:#\[napi\(([^\]]*)\)\]\s*)?pub (\w+):\s*([^\n]+),/g)) {
        fields[/js_name\s*=\s*"(\w+)"/.exec(attribute ?? "")?.[1] ?? camel(field)] = type;
      }
      types[name] = fields;
    }
  }
  return types;
}

/** The top-level items of a Rust file with their text: a fn or an impl, by the line that opens it and the `}` that closes it. */
function items(text) {
  const found = [];
  const lines = text.split("\n");
  for (let at = 0; at < lines.length; at += 1) {
    if (!/^(pub(\(crate\))? )?(fn|impl)\b/.test(lines[at])) continue;
    let end = at;
    let signature = lines[at];
    while (end < lines.length && lines[end] !== "}" && !(end === at && lines[at].endsWith("}"))) {
      end += 1;
      if (end - at < 8 && !signature.includes("{")) signature += lines[end];
    }
    found.push({ name: /(?:fn\s+(\w+)|impl(?:<[^>]*>)?\s+(.+?)\s*\{)/.exec(signature)?.slice(1).find(Boolean) ?? "", signature, body: lines.slice(at, end + 1).join("\n") });
  }
  return found;
}

/** A Rust reader's expression, cut at the end of its call: `.unwrap_or(0.0) as f32`, `.map_or(1, |x| x)`. */
function callAfter(text, at) {
  let depth = 0;
  let end = at;
  for (; end < text.length; end += 1) {
    const letter = text[end];
    if (letter === "(" || letter === "[" || letter === "{") depth += 1;
    if (letter === ")" || letter === "]" || letter === "}") {
      if (depth === 0) break;
      depth -= 1;
    }
    if ((letter === "," || letter === ";") && depth === 0) break;
  }
  return text.slice(at, end).replace(/\s+/g, " ").replace(/^ /, "").trim();
}

/**
 * interface -> field -> how the binding reads it, from the functions that take that struct by value or reference:
 * `{ reader, expression, narrowed }`, with the expression as written after the field's name, and narrowed when any reader
 * rounds it to a float, and `helper` for the function it is handed to.
 */
function readers(types) {
  const read = {};
  const all = bindingFiles().flatMap((file) => items(fs.readFileSync(path.join(root, "native", "src", file), "utf8").replace(/\r\n/g, "\n")));
  for (const [name, fields] of Object.entries(types)) {
    const parameter = new RegExp(String.raw`(\w+): (?:&(?:mut )?)?(?:Option<)?&?${name}\b`);
    const taking = all.filter((item) => parameter.test(item.signature) || item.signature.includes(`From<${name}> for`) || item.signature.includes(`From<&${name}> for`));
    read[name] = {};
    for (const field of Object.keys(fields)) {
      const rust = snake(field);
      for (const item of taking) {
        const argument = parameter.exec(item.signature)?.[1] ?? "value";
        const sites = [...item.body.matchAll(new RegExp(String.raw`\b${argument}\s*\.${rust}\b`, "g"))];
        for (const site of sites) {
          const expression = callAfter(item.body, site.index + site[0].length);
          const narrowed = /\bas f32\b/.test(expression) || read[name][field]?.narrowed === true;
          // a helper the field is handed to, as `vector_or_zero(value.push, "push")`, says what absent reads as
          const helper = /(\w+)\(\s*$/.exec(item.body.slice(Math.max(0, site.index - 40), site.index))?.[1];
          if (!/unwrap_or|map_or/.test(read[name][field]?.expression ?? "")) read[name][field] = { reader: item.name, expression, ...(helper && !read[name][field]?.helper ? { helper } : {}) };
          read[name][field].narrowed = narrowed;
        }
      }
    }
  }
  return read;
}

/** What an adapter's object literal typed as an interface sends each field from, and what it does with a tick's result. */
function adapterMaps(files, interfaces) {
  const into = {};
  const take = (kind, field, entry) => {
    (into[kind] ??= {})[field] ??= [];
    if (!into[kind][field].includes(entry)) into[kind][field].push(entry);
  };
  for (const file of files) {
    const source = ts.createSourceFile(file, fs.readFileSync(path.join(root, file), "utf8"), ts.ScriptTarget.Latest, true);
    const text = (node) => node.getText(source).replace(/\s+/g, " ");
    const visit = (node) => {
      const typed = ts.isVariableDeclaration(node) && node.type && node.initializer && ts.isObjectLiteralExpression(node.initializer) ? [node.type, node.initializer]
        : ts.isAsExpression(node) && ts.isObjectLiteralExpression(node.expression) ? [node.type, node.expression] : null;
      if (typed) {
        const kind = /Native\w+/.exec(typed[0].getText(source))?.[0];
        if (interfaces.has(kind)) {
          for (const property of typed[1].properties) {
            if (ts.isPropertyAssignment(property)) take(kind, property.name.getText(source), `${path.basename(file)}: ${text(property.initializer)}`);
            else if (ts.isShorthandPropertyAssignment(property)) take(kind, property.name.getText(source), `${path.basename(file)}: ${property.name.getText(source)}`);
          }
        }
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
  }
  return into;
}

module.exports = { adapterMaps, camel, items, readers, rustTypes, snake };

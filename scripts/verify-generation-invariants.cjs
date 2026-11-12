// The last stage of generate-all: what the generated method audit must never hold again, each a defect the 2026-11-04 audit
// found, checked on the output so that a run that reproduces one fails. MC_GENERATED_ROOT reads another folder, which is
// how the negative fixtures run this for real.
//
//   M30  a method's fingerprint must be of its declaration, not of a call site that came first
//   M31  LocalPlayer must be read from net/minecraft/client/player, present in every modern release, with its aiStep
const fs = require("node:fs");
const path = require("node:path");

const repositoryRoot = path.resolve(__dirname, "..");

/** Whether `text` opens with the declaration of `name`, not with a statement that calls it. */
function isDeclaration(text, name) {
  const header = text.trimStart().split("{")[0];
  if (!new RegExp(`\\b${name}\\s*\\(`).test(header)) return false;
  // a call has a receiver or ends a statement before any body; a declaration has a type before its name
  if (/;/.test(header) || new RegExp(`[.]\\s*${name}\\s*\\(`).test(header)) return false;
  return new RegExp(`[\\w>\\]]\\s+${name}\\s*\\(`).test(header);
}

function defects(root) {
  const read = (name) => JSON.parse(fs.readFileSync(path.join(root, name), "utf8"));
  const found = [];
  const audit = read("physics-methods.json").releases;
  for (const [version, release] of Object.entries(audit)) {
    for (const [file, { methods }] of Object.entries(release.files)) {
      for (const [name, declarations] of Object.entries(methods)) {
        for (const declaration of declarations) {
          if (!isDeclaration(declaration.source, declaration.matchedName ?? name)) {
            found.push(`M30 ${version} ${file}#${name} line ${declaration.line}: the fingerprint is not of a declaration: ${declaration.source.trimStart().slice(0, 60)}`);
          }
        }
      }
    }
  }
  const sources = read("minecraft-versions.json");
  for (const version of sources.versions) {
    const local = sources.sourceFiles[version]?.files.find((file) => file.path.endsWith("LocalPlayer.java"));
    if (local?.path !== "net/minecraft/client/player/LocalPlayer.java") found.push(`M31 ${version}: LocalPlayer is at ${local?.path ?? "no path"}`);
    else if (!local.present) found.push(`M31 ${version}: LocalPlayer is not in the source read`);
    if (!audit[version]?.files["LocalPlayer.java"]?.methods?.aiStep?.length) found.push(`M31 ${version}: the audit has no LocalPlayer.aiStep`);
  }
  return found;
}

function main() {
  const found = defects(process.env.MC_GENERATED_ROOT ?? path.join(repositoryRoot, "generated"));
  for (const defect of found.slice(0, 20)) console.error(defect);
  if (found.length > 20) console.error(`and ${found.length - 20} more`);
  if (found.length > 0) process.exit(1);
  console.log("Generated method audit: no reproduced defect.");
}

if (require.main === module) main();

module.exports = { defects, isDeclaration };

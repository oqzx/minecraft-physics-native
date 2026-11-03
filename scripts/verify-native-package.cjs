const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { verifyArtifactDirectory } = require("./native-artifacts.cjs");

const root = path.resolve(__dirname, "..");
const generatedData = path.join(root, "native", "data");
const generatedReleases = path.join(generatedData, "releases");
assert.equal(fs.existsSync(path.join(generatedData, "mod.rs")), true, "native/data/mod.rs was not generated");
assert.equal(fs.existsSync(generatedReleases), true, "native/data/releases was not generated");
assert.equal(
    fs.readdirSync(generatedReleases).filter((entry) => entry.endsWith(".rs")).length,
    [...fs.readFileSync(path.join(root, "native", "src", "versioned", "mod.rs"), "utf8").matchAll(/#\[path = "v-[^"]+\.rs"\]/g)].length,
    "expected one generated release module for each supported release",
);
const artifacts = path.join(root, "native", "artifacts");
const declaration = path.join(artifacts, "native.d.ts");
const loader = path.join(artifacts, "native.js");
const nativeApi = fs.readFileSync(path.join(root, "native", "src", "napi_api.rs"), "utf8");
const nativeExports = fs.readFileSync(path.join(root, "native", "src", "lib.rs"), "utf8");

assert.equal(fs.existsSync(declaration), true, "native.d.ts was not generated");
assert.equal(fs.existsSync(loader), true, "native.js was not generated");
assert.match(fs.readFileSync(declaration, "utf8"), /applyBlockUpdates/);
assert.match(fs.readFileSync(declaration, "utf8"), /versionCapabilities/);
assert.match(fs.readFileSync(declaration, "utf8"), /createSimulationState/);
assert.match(fs.readFileSync(declaration, "utf8"), /NativeEntityDescriptor/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedBlockHasTag/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedStateIndex/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedBlockStateCount/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedFluidStateCount/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedFluidStateIndex/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedBlockDefaults/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedMethodShapeBoxes/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedExactMethodShapeBoxes/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedExactMethodShapeBoxesForState/);
assert.match(fs.readFileSync(declaration, "utf8"), /NativeGeneratedShapeBox/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedShapePropertyDependencies/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedMethodShapePropertyDependencies/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedMethodShapePredicates/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedMethodShapePredicatesMatchState/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedMethodShapeIsStateDependent/);
assert.match(fs.readFileSync(declaration, "utf8"), /generatedMethodShapeProvenance/);
assert.match(nativeApi, /pub fn generated_method_shape_property_dependencies/);
assert.match(nativeApi, /pub fn generated_method_shape_predicates/);
assert.match(nativeApi, /pub fn generated_method_shape_predicates_match_state/);
assert.match(nativeApi, /pub fn generated_method_shape_is_state_dependent/);
assert.match(nativeApi, /pub fn generated_exact_method_shape_boxes_for_state/);
assert.match(nativeApi, /pub fn generated_method_shape_provenance/);
assert.match(nativeExports, /generated_method_shape_property_dependencies/);
assert.match(nativeExports, /generated_method_shape_predicates/);
assert.match(nativeExports, /generated_method_shape_predicates_match_state/);
assert.match(nativeExports, /generated_method_shape_is_state_dependent/);
assert.match(nativeExports, /generated_method_shape_provenance/);

const platformArtifact = fs
    .readdirSync(artifacts)
    .find((entry) => entry.endsWith(".node"));
assert.ok(platformArtifact, "platform native artifact was not generated");

// the declarations of the release registry and the bulk tick's scene
const declarationText = fs.readFileSync(declaration, "utf8");
for (const name of ["isSupportedVersion(", "simulateVersionedTicks(", "simulateVersionedTickOutcome(", "fireworkRocketTicks?: Array<number>"]) {
    assert.ok(declarationText.includes(name), `native.d.ts declares no ${name}`);
}

// native/artifacts holds only what napi-rs writes there: native.js, its declarations, the addons
// and the WebAssembly builds with their loaders and declarations (README, "WebAssembly"); a
// retired build left there is refused
verifyArtifactDirectory(artifacts);

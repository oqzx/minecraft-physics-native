# Source records

Every registered release has one record in `generated/release-manifest.json`, written by
`scripts/generate-release-manifest.cjs`. `scripts/source-admission.cjs` decides whether it can be cited, and
`test/source-admission.test.cjs` holds it to that.

## Schema

| Field | Meaning |
| --- | --- |
| `release` | The exact release id. |
| `versionJsonSha1` | Mojang's version json, for the distribution's identity. |
| `mappings.kind`, `mappings.sha1` | `mojang` with the client mappings' sha1, `unobfuscated` for 26.x, `unpublished` before 1.14.4. |
| `source.status` | `embedded`, `alias`, or a refusal: `directory-only`, `rejected`, `stand-in`, `unavailable`. |
| `source.embedded` | The release the source names inside itself, never a branch or folder name. |
| `source.commit` | The immutable commit of the extracted data repository. |
| `source.clientJar.sha1` | The client jar, as Mojang's version json declares it. |
| `source.clientJar.remappedSha256` | The jar remapped to Mojang's names by unimined. |
| `source.decompiler` | The tool and revision that produced the sources. |
| `source.tree` | sha256 of a local tree's `.java` files, by relative path, line endings aside. |
| `source.mapping` | The names the source is in. |
| `source.rejectedRef` | A branch that names this release and embeds another. Never read. |
| `source.scope`, `source.evidence` | For an alias: what it covers, and the hash of its proof. |

The distribution side is the client. Server jars are used only by the alias proofs in
`generated/release-aliases.json` and by `generated/data-aliases.json`.

## Admission

`sourceRecordProblems` fails a record, naming the release, when:

- the source embeds another release, or is not `embedded` or `alias`;
- the mappings are missing, or a Mojang mapping has no sha1, or the source is in other names;
- a commit, client jar or remapped jar differs from `scripts/source-pins.cjs`;
- nothing identifies the source.

`node scripts/release-admission.cjs <version>` shows this as its `source` dimension.

## Standing

| Releases | Status | Input |
| --- | --- | --- |
| 1.8.8, 1.9.4, 1.10.2, 1.11.2, 1.12.2, 1.13.2 | `exact` | client jar, unimined remap by `legacy-mappings/<release>.tiny` (`NAMED_JARS`), vineflower 1.12.0 (mapping `recorder`) |
| 1.14.4, 1.18.1, 1.19, 1.19.1, 1.19.3, 1.19.4, 1.20 to 1.21.11 except 1.20.x gaps below, 26.1, 26.1.1 | `exact` | pinned commit |
| 1.15, 1.15.1, 1.15.2, 1.16 to 1.16.5, 1.17, 1.17.1, 1.18, 1.18.2, 1.19.2 | `exact` | client jar, unimined remap, vineflower 1.12.0 |
| 26.1.2 | `exact` | client jar, vineflower 1.12.0 |
| 26.2, 26.3 | `exact` | `E:/pr/mc-source` tree, with its hash and the client jar's sha1 |

1.16.4 keeps `rejectedRef`: `origin/client1.16.4` embeds 20w45a. Its record comes from the client jar instead.

### The legacy releases

1.8.8 to 1.13.2 are read from their own client, remapped by the recorder's mapping and decompiled by the pinned Vineflower
(`scripts/materialize-sources.cjs`, `MC_LEGACY_SOURCES`), like the pinned clients of later releases. The record names the
client sha1, the remapped jar's sha256, the decompiler and the tree hash, and mapping `recorder`: Mojang published no
mappings before 1.14.4. The names are Mojang's where 1.14.4 has the member, Yarn's or intermediary's otherwise, and the
mapping is the recorder's own, so it has its faults: across 1.13 it has Boat's `inputLeft` and `inputDown` the wrong way
round (the recorder's `mod/mc/1.12.2/.../BoatAccessor.java`), which makes `generated/physics-breakpoints.json` see
`Boat.controlBoat` change at 1.12.2 to 1.13.2 where only the two names change.

Until 2026-11-09 they were read from yarn-named trees no record could make again, which kept them `blocked`, and 1.8.8 was
served from 1.8.9's (`generated/release-aliases.json`, which stays as the evidence that the two are the same code).

## Downstream

`generated/conformance-evidence.json` cites the hash of the source record, so changing a release's source
takes its trace claims. `node scripts/verify-version-coverage.cjs --fresh` re-reads the inputs of the
generators. An input that changes is replaced in `source-pins.cjs`, and the old value stays in git.

## Entity geometry

`generated/entity-data.json` holds what each release's own code answers, from its pinned jar. Nothing is copied from a
neighbouring release, and an entity that cannot be built is named in `noLive` with why.

| field | releases | from |
|---|---|---|
| `entities`, `poses` | 1.14.4 on, status `exact` | `EntityDump.java`: `EntityType` dimensions, pose overrides, fixed flag, and the eye height from 1.20.5 |
| `live.<id>.size` | 1.8.8 to 1.13.2, status `legacy` | `LegacyEntityDump.java`: `width` and `height` after the constructor |
| `eye`, `eyeCrouching`, `playerEyes` | `getEyeHeight`, before 1.20.5 a method of a live entity | a built entity; `playerEyes` before 1.14 is not read |
| `passengersRidingOffset`, `myRidingOffset` | to 1.20.1 | a built entity |
| `riderPoint` | 1.20.2 on | `getPassengerAttachmentPoint`, facing south |
| `attachments` | 1.20.5 on | `EntityDimensions.attachments()`, a table of the type, so also for an entity that cannot be built |
| `riderCases` | all 50 | `Entity.positionRider` itself |

A row of `riderCases` is `[slot, yaw, size, x, y, z]`: a zombie put by the vehicle's own method on a vehicle at the origin
turned to `yaw`. `slot` -1 is one rider alone in what seats more, otherwise every seat is full. `size` is 1, 2 (the `scale`
attribute, 1.20.5 on) or `"baby"`. The zombie's own offset is in `y`, and a passenger that is an `Animal` sits differently on a
boat or a camel, which a row does not show. 1.8.8 has one seat, a field, and `slot` 0.

Vehicles covered are in `LiveEntityDump.VEHICLES`. A vehicle a blank level cannot build has no `riderCases`: `pig`, `chicken` from 1.21.5 and
the minecarts from 1.21.2 need a bound registry or item components, and those with a table keep it. `test/entity-data.test.cjs` has the engine's
boat seat against these rows at each yaw, alone and with two aboard, in all 50 releases.

`size` is the entity as its `(Level)` constructor leaves it, which is not always the size it plays at. `slime`, `magma_cube`,
`falling_block`, `skeleton` (1.8.8 to 1.10.2) and `llama` (1.11.2, 1.12.2) get theirs later, from their size, block, type or
age. `E:\pr\minecraft-data-generator` reads the same field in a running server and agrees on all 253 entities of 1.11.2, 1.12.2
and 1.13.2. minecraft-data holds the played sizes for those, plus `experience_orb` at 0.5 from the constructor a spawn uses, and
`item_frame` and `llama_spit`. Its other legacy values match.

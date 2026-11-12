# Passenger fluid context

`NativeSimulationState` and the Mineflayer adapter's structural `PlayerState`
accept an optional `boatVehicle` input:

```ts
const boatVehicle = {
    boundingBox: {
        min: { x: 10, y: 63.5, z: 10 },
        max: { x: 11.375, y: 64.0625, z: 11.375 },
    },
    underwater: false,
};
const next = native.simulateVersionedTick(version, world, {
    ...playerState,
    passenger: true,
    boatVehicle,
}, {});
```

The box is the boat's actual world-space bounding box at the fluid-query phase.
`underwater` means boat status `UNDER_WATER` or `UNDER_FLOWING_WATER`, not ordinary
`IN_WATER` contact. The caller supplies both values; the adapter does not infer
them from an arbitrary `bot.vehicle.inWater` flag.

The context is applied only when `passenger` is true. Omit or clear it for other
vehicles. Dismounting disables it even if the context remains in the state.
Whole-state replacement without the context clears it. Returned state retains
the supplied geometry/status so repeated calls and fixed-context batches can
continue consistently. `NativePlayer` keeps it between packed calls through
`setState`/`state`; no packed-array indices change.

Supply updated geometry/status when the boat moves or changes submersion.
A batch holds the supplied context fixed: it does not predict the boat's motion,
seat offsets, mount/dismount events, or full vanilla `rideTick`. These remain
separate vehicle/passenger responsibilities. Fluid adjustment never replaces
the passenger's physical collision box, pose dimensions, or eye position.

The recorder replay currently routes ridden ticks through its vehicle layer and
does not run a passenger fluid tick. Its exhaustive field registry accounts for
this input as excluded for that path; adding the API does not extend recorder
coverage to passenger fluid behavior.

## Release rules

| Registered releases | Water contact/current | Eye-fluid exclusion |
| --- | --- | --- |
| 1.9.4–1.15.2 | Any boat excludes water. | Any boat excludes water/lava eye queries. |
| 1.16–1.16.1 | Any boat excludes water. | A non-underwater boat excludes an eye sample inside its full box; maxima are exclusive. |
| 1.16.2–1.19.2 | Any boat excludes water. | A non-underwater boat excludes samples within its inclusive vertical bounds. |
| 1.19.3–1.20.4 | Only a non-underwater boat excludes water. | Inclusive vertical bounds, with the sample `0.11111111F` below the eyes. |
| 1.20.5–1.21.11 | Only a non-underwater boat excludes water. | Inclusive vertical bounds at the actual eyes. |
| 26.1–26.3 | Water, lava, eyes and current use the adjusted interaction scan. | Same scan; 26.3 retains camera-specific fluid height. |

The 1.16–1.20.4 eye sample uses the `0.11111111F` offset. Before 26.1, boat water
exclusion does not suppress the separate lava-contact query. The registered
1.8.8 behavior is unchanged; only a 1.8.9 stand-in source is available.

From 26.1, the passenger box is first deflated by `0.001`. An underwater boat
leaves that scan unchanged. Otherwise, a boat covering its top removes the scan;
a partially covering boat raises its bottom to the boat's top. Fluid depth still
uses the original passenger bounding-box bottom. This preserves shallow-current
scaling and avoids deflating the adjusted scan twice. A removed scan clears the
fluid/eye flags and applies no current.

## References and evidence limits

Primary historical sources are under `E:/pr/mc-decompiled`, particularly
`1.19.2/net/minecraft/entity/Entity.java:1075` versus `1.19.3/.../Entity.java:1080`,
and `1.20.4/.../Entity.java:1204` versus `1.20.5/.../Entity.java:1219`.
Modern references are `E:/pr/mc-source/26.3/client/net/minecraft/world/entity/Entity.java:4233`,
`EntityFluidInteraction.java:40`, and `vehicle/boat/AbstractBoat.java:787`.

The 26.1 and 26.1.1 methods were checked in
`D:/projects/mc-zuri/mc-data/extracted_minecraft_data` at commits
`250494672ff7a6ef66607c785c91a56a5c32c9f3` and
`2322018b5712b13eacbbb324b7f9dd359091cdc7`. Existing JDK bytecode inspection of
cached, release-identified jars pins the 1.16.2 eye-boundary change and confirms
the 26.1.2 adjusted-box methods. This is evidence for those methods only.

`D:/minecraft/java/physics-data-generator/mc/26.3/src/main/java/dev/physrec/mc/BridgeImpl.java:483`
shows capture of boat status through its accessor. It is supporting capture
tooling, not a new vanilla execution for this implementation.

Full method excerpts, commands, tool and artifact hashes, embedded version IDs,
and source/Git provenance are retained in
`audit/physics-backlog-20261104-dc4f918/b136-evidence` in the main checkout,
with `b136-reference.txt` and its reproduction script beside it.

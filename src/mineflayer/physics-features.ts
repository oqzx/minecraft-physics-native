// The feature catalog of mineflayer-physics-utils, which is not minecraft-data's: its names ask how vanilla moves and
// tracks an entity in the selected release, and minecraft-data knows none of them. Each answer is a breakpoint read from
// the sources of the releases on both sides (test/physics-features.test.cjs reads them again, release by release), not
// the dependency's features.json, whose lists stop at 1.21 and skip 1.16. The dependency's own engines keep its list.
import { compareReleases } from './blocks';

const before = (release: string, version: string): boolean => compareReleases(release, version) < 0;

/**
 * The index of DATA_LIVING_ENTITY_FLAGS, the first tracked field after Entity's, or null where a release has none:
 * 1.8 has no such field (1.8.8 LivingEntity.java:196-199), 1.9 follows Entity's five (1.9.4 Entity.java), 1.10 added
 * NO_GRAVITY (1.10.2 Entity.java), 1.14 POSE (1.14.4 Entity.java:2825) and 1.17 DATA_TICKS_FROZEN (1.17 Entity.java,
 * 26.3 Entity.java:4268).
 */
export function livingFlagsSlot(release: string): number | null {
    if (before(release, '1.9')) return null;
    if (before(release, '1.10')) return 5;
    if (before(release, '1.14')) return 6;
    if (before(release, '1.17')) return 7;
    return 8;
}

/**
 * Whether an attribute's key on the wire is its registry key: the client packet writes the attribute's name (1.15.2
 * ClientboundUpdateAttributesPacket.java:62), then the resource location (1.16 :66, 1.20.4 :50), then a holder from
 * 1.20.5 that minecraft-data's resource already names. Only the middle one takes a `minecraft:` over that resource.
 */
export function attributesWearNamespace(release: string): boolean {
    return !before(release, '1.16') && before(release, '1.20.5');
}

const CATALOG: ReadonlyMap<string, (release: string) => boolean> = new Map([
    // the constant 0.02 of water and lava (1.12.2 LivingEntity.java:1572,1603)
    ['independentLiquidGravity', release => before(release, '1.13')],
    // gravity / 16 and / 4 (1.13.2 LivingEntity.java:1639,1677, 26.3 LivingEntity.java:2474,2614)
    ['proportionalLiquidGravity', release => !before(release, '1.13')],
    // SoulSandBlock.onEntityCollision multiplies the velocity (1.8.8 SoulsandBlock.java:25-27, 1.14.4 SoulsandBlock.java:26)
    ['velocityBlocksOnCollision', release => before(release, '1.15')],
    // Entity.getBlockSpeedFactor of the block stood on (1.15 Entity.java:634,681, Blocks.java:597)
    ['velocityBlocksOnTop', release => !before(release, '1.15')],
    // horizontalCollision alone (1.13.2 LivingEntity.java:1606), then || jumping (1.14.4 LivingEntity.java:1835)
    ['climbUsingJump', release => !before(release, '1.14')],
    // only where slot 6 is the field: where it is neither 6 nor 8 no answer can be true, see livingFlagsSlot
    ['itemUsageMetadata6', release => livingFlagsSlot(release) === 6],
    // minecraft-data calls no effect by its registry name in any release ('JumpBoost', vanilla 'jump_boost')
    ['effectNamesAreRegistryNames', () => false],
    ['attributesPrefixedByMinecraft', attributesWearNamespace],
]);

/** The names mineflayer-physics-utils asks of an engine. */
export const PHYSICS_FEATURES: readonly string[] = [...CATALOG.keys()];

/** The answer for that release, or undefined for a name of no physics meaning, which is minecraft-data's to answer. */
export function physicsFeature(release: string, feature: string): boolean | undefined {
    return CATALOG.get(feature)?.(release);
}

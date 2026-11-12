'use strict';

// The generated block data against what each release's client reports, and the readers behind it:
// registrations through helpers, collections and reference keys (B031, B032), state definitions
// through superclasses, filters and accessors (B033). A definition that cannot be read stays
// visibly unresolved and cannot number states.

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { describe, it } = require('node:test');
const { literalRegistrations, referenceFields } = require('../scripts/block-references.cjs');
const { stateResolver, stateDefinitionOf } = require('../scripts/block-state-fields.cjs');

const root = path.resolve(__dirname, '..');
const generated = name => JSON.parse(fs.readFileSync(path.join(root, 'generated', name), 'utf8'));
const runtime = generated('runtime-registries.json').releases;
const registries = generated('block-registries.json').versions;
const states = generated('block-state-definitions.json').releases;

/** id -> { className, stateCount, properties } of a release's generated Rust table. */
function moduleBlocks(version) {
    const file = path.join(root, 'native', 'data', 'releases', `v${version.replaceAll('.', '_')}.rs`);
    const text = fs.readFileSync(file, 'utf8');
    const blocks = new Map();
    const row = /GeneratedBlock \{ id: "([^"]+)", class_name: (?:None|Some\("([^"]+)"\)), properties: [A-Z0-9_]+, property_order: [A-Z0-9_]+, state_count: (?:None|Some\((\d+)\))/g;
    for (const match of text.matchAll(row)) blocks.set(match[1], { className: match[2] ?? null, stateCount: match[3] ? Number(match[3]) : null });
    return blocks;
}

describe('the tables against each release\'s client', () => {
    for (const [version, dump] of Object.entries(runtime)) {
        it(`${version} has the client's blocks with its state counts`, () => {
            const blocks = moduleBlocks(version);
            assert.deepEqual([...blocks.keys()].sort(), dump.blocks.map(([id]) => id).sort());
            for (const [id, count] of dump.blocks) assert.equal(blocks.get(id).stateCount, count, id);
        });
    }

    it('covers every modern release in the registry manifest', () => {
        assert.deepEqual(Object.keys(runtime).sort(), Object.keys(registries).sort());
    });

    it('gives every client block a class, 26.1.2\'s among them', () => {
        for (const [version, registry] of Object.entries(registries)) {
            assert.deepEqual(registry.blocks.filter(id => !registry.classes[id]?.className), [], version);
            assert.equal(registry.runtimeOnly, undefined, version);
        }
        assert.equal(registries['26.1.2'].classes.white_bed.className, 'BedBlock');
    });

    it('resolves every state count the client reports from the release\'s own classes', () => {
        const states = JSON.parse(fs.readFileSync(path.join(root, 'generated', 'block-state-definitions.json'), 'utf8')).releases;
        for (const [version, registry] of Object.entries(registries)) {
            for (const [id, count] of runtime[version].blocks) {
                const className = registry.classes[id].className;
                const computed = className === 'Block' ? 1 : states[version].blocks[className]?.stateCount;
                assert.equal(computed, count, `${version} ${id} ${className}`);
            }
            assert.equal(states[version].unresolvedPropertyCount, 0, version);
        }
    });

    it('holds the blocks the first registry reader lost', () => {
        for (const version of ['1.20.3', '1.21.1']) {
            for (const id of ['pumpkin', 'melon', 'melon_stem', 'attached_pumpkin_stem']) {
                assert.ok(registries[version].blocks.includes(id), `${version} ${id}`);
            }
        }
        for (const version of ['1.21.2', '1.21.11', '26.1.1']) {
            for (const id of ['white_bed', 'red_bed', 'oak_stairs', 'cobblestone_wall']) {
                assert.ok(registries[version].blocks.includes(id), `${version} ${id}`);
            }
            assert.equal(registries[version].classes.white_bed.className, 'BedBlock', version);
        }
        assert.ok(registries['1.21.9'].blocks.includes('waxed_oxidized_copper_chain'));
        assert.equal(registries['1.21.9'].blocks.includes('wheat_crop'), false);
    });

    it('has no symbol for a name in 26.2 and 26.3, and their real ids', () => {
        for (const version of ['26.2', '26.3']) {
            for (const spurious of ['wheat_crop', 'melon_crop', 'pumpkin_crop', 'redstone']) {
                assert.equal(registries[version].blocks.includes(spurious), false, `${version} ${spurious}`);
            }
            for (const id of ['wheat', 'melon_stem', 'pumpkin_stem', 'redstone_wire']) {
                assert.ok(registries[version].blocks.includes(id), `${version} ${id}`);
            }
        }
    });

    it('gives every class of a release with sources its client\'s count, or says it could not', () => {
        for (const [version, registry] of Object.entries(registries)) {
            if (!registry.source.match(/^[0-9a-f]{40}:|mc-source/)) continue;
            for (const [id, count] of runtime[version].blocks) {
                const className = registry.classes[id].className;
                const block = states[version].blocks[className];
                if (!block) {
                    assert.equal(count, 1, `${version} ${id} is plain ${className}`);
                } else {
                    assert.equal(block.stateCount, count, `${version} ${id} (${className})`);
                }
            }
        }
    });

    it('gives carrots and potatoes their eight ages', () => {
        for (const version of ['1.14.4', '1.20.4', '1.21.6', '26.1', '26.3']) {
            for (const id of ['carrots', 'potatoes']) {
                assert.equal(moduleBlocks(version).get(id).stateCount, 8, `${version} ${id}`);
            }
        }
    });
});

describe('a run without the sources', () => {
    it('fails unless it says it is keeping cached entries', () => {
        const { spawnSync } = require('node:child_process');
        const os = require('node:os');
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-fresh-'));
        const absent = path.join(base, 'absent');
        const env = { ...process.env, MC_SOURCE_ROOT: absent, MC_SOURCE_CACHE: absent, MC_EXTRACTED_DATA_REPO: absent };
        const copies = { MC_BLOCK_REGISTRY_MANIFEST: 'block-registries.json', MC_BLOCK_STATE_MANIFEST: 'block-state-definitions.json' };
        for (const [variable, name] of Object.entries(copies)) {
            env[variable] = path.join(base, name);
            fs.copyFileSync(path.join(root, 'generated', name), env[variable]);
        }
        for (const script of ['generate-block-registry-manifest.cjs', 'generate-block-state-manifest.cjs']) {
            const run = spawnSync(process.execPath, [path.join(root, 'scripts', script)], { env, encoding: 'utf8' });
            assert.equal(run.status, 1, script);
            assert.match(run.stderr, /pass --cached to keep its entry/, script);
        }
        fs.rmSync(base, { recursive: true, force: true });
    });

    it('leaves the entries the last run with them wrote', () => {
        const { execFileSync } = require('node:child_process');
        const os = require('node:os');
        const base = fs.mkdtempSync(path.join(os.tmpdir(), 'mc-kept-entries-'));
        const absent = path.join(base, 'absent');
        const copies = { MC_BLOCK_REGISTRY_MANIFEST: 'block-registries.json', MC_BLOCK_STATE_MANIFEST: 'block-state-definitions.json' };
        const env = { ...process.env, MC_SOURCE_ROOT: absent, MC_SOURCE_CACHE: absent, MC_EXTRACTED_DATA_REPO: absent };
        for (const [variable, name] of Object.entries(copies)) {
            env[variable] = path.join(base, name);
            fs.copyFileSync(path.join(root, 'generated', name), env[variable]);
        }
        for (const script of ['generate-block-registry-manifest.cjs', 'generate-block-state-manifest.cjs']) {
            execFileSync(process.execPath, [path.join(root, 'scripts', script), '--cached'], { env, stdio: 'pipe' });
        }
        // the entries, not sourceManifest, which is relative to where the copy was written
        const entries = file => {
            const { versions, releases } = JSON.parse(fs.readFileSync(file, 'utf8'));
            return JSON.stringify(versions ?? releases);
        };
        for (const [variable, name] of Object.entries(copies)) {
            assert.equal(entries(env[variable]) === entries(path.join(root, 'generated', name)), true, name);
        }
        fs.rmSync(base, { recursive: true, force: true });
    });
});

describe('reading Blocks.java', () => {
    const keys = { PUMPKIN: 'pumpkin', MELON_STEM: 'melon_stem' };
    const source = `
public class Blocks {
   public static final Block AIR = register((String)"air", new AirBlock(BlockBehaviour.Properties.of()));
   public static final Block WHITE_BED;
   public static final Block OAK_STAIRS;
   public static final Block PUMPKIN;
   public static final Block MELON_STEM;
   public static final WeatheringCopperBlocks COPPER_CHAIN;
   public static final Block STONE;

   private static Block registerBed(String var0, DyeColor var1) {
      return register(var0, (var1x) -> new BedBlock(var1, var1x), Properties.of());
   }

   static {
      WHITE_BED = registerBed("white_bed", DyeColor.WHITE);
      OAK_STAIRS = registerLegacyStair("oak_stairs", OAK_PLANKS);
      PUMPKIN = register((ResourceKey)net.minecraft.references.Blocks.PUMPKIN, PumpkinBlock::new, BlockBehaviour.Properties.of());
      MELON_STEM = register(BlockIds.MELON_STEM, (var0x) -> {
         return new StemBlock(BlockIds.PUMPKIN, var0x);
      }, BlockBehaviour.Properties.of());
      COPPER_CHAIN = WeatheringCopperBlocks.create("copper_chain", Blocks::register, ChainBlock::new, WeatheringCopperChainBlock::new, (var0) -> BlockBehaviour.Properties.of());
      STONE = register("stone", BlockBehaviour.Properties.of());
   }
}
`;

    it('reads strings, casts, helpers, keys, lambdas, and copper collections', () => {
        const classes = literalRegistrations(source, keys);
        assert.equal(classes.air.className, 'AirBlock');
        assert.equal(classes.white_bed.className, 'BedBlock');
        assert.equal(classes.oak_stairs.className, 'StairBlock');
        assert.equal(classes.pumpkin.className, 'PumpkinBlock');
        assert.equal(classes.melon_stem.className, 'StemBlock');
        assert.equal(classes.stone.className, 'Block');
        assert.equal(classes.copper_chain.className, 'WeatheringCopperChainBlock');
        assert.equal(classes.oxidized_copper_chain.className, 'WeatheringCopperChainBlock');
        assert.equal(classes.waxed_copper_chain.className, 'ChainBlock');
        assert.equal(classes.waxed_oxidized_copper_chain.className, 'ChainBlock');
        assert.equal(Object.keys(classes).length, 14);
    });

    it('refuses what it cannot read, naming the field', () => {
        assert.throws(() => literalRegistrations('   public static final Block X = makeBlock("x");\n'), /Unresolved block registration X: makeBlock/);
        assert.throws(() => literalRegistrations('   public static final Block X = register(name(), Properties.of());\n'), /Unresolved block identifier X/);
        assert.throws(() => literalRegistrations('   public static final Block X = register(Blocks.UNKNOWN, Properties.of());\n'), /Unresolved block identifier X/);
        assert.throws(() => literalRegistrations('   public static final Block X = register("x", P.of());\n   public static final Block Y = register("x", P.of());\n'), /Duplicate block registration: x/);
    });

    it('keeps a block id apart from its item id in the references', () => {
        const fields = referenceFields(`
            public static final BlockItemId WHEAT_CROP = BlockItemId.create("wheat", "wheat_seeds");
            public static final BlockItemId MELON_CROP = BlockItemId.create("melon_stem", "melon_seeds");
            public static final BlockItemId REDSTONE = BlockItemId.create("redstone_wire", "redstone");
        `);
        assert.deepEqual(fields.WHEAT_CROP.blocks, ['wheat']);
        assert.deepEqual(fields.MELON_CROP.blocks, ['melon_stem']);
        assert.deepEqual(fields.REDSTONE.blocks, ['redstone_wire']);
    });
});

describe('reading a state definition', () => {
    const classes = {
        Block: 'public class Block extends BlockBehaviour { protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) { } }',
        BlockStateProperties: `
public class BlockStateProperties {
   public static final IntegerProperty AGE_7 = IntegerProperty.create("age", 0, 7);
   public static final IntegerProperty AGE_3 = IntegerProperty.create("age", 0, 3);
   public static final BooleanProperty WATERLOGGED;
   public static final BooleanProperty HAS_BOTTLE_0;
   public static final BooleanProperty HAS_BOTTLE_1;
   public static final EnumProperty<Direction.Axis> HORIZONTAL_AXIS;
   public static final DirectionProperty FACING_HOPPER;
   public static final EnumProperty<RailShape> RAIL_SHAPE_STRAIGHT;
   public static final EnumProperty<Direction> HORIZONTAL_FACING;
   static {
      WATERLOGGED = BooleanProperty.create("waterlogged");
      HAS_BOTTLE_0 = BooleanProperty.create("has_bottle_0");
      HAS_BOTTLE_1 = BooleanProperty.create("has_bottle_1");
      HORIZONTAL_AXIS = EnumProperty.create("axis", Direction.Axis.class, (Enum[])(Direction.Axis.X, Direction.Axis.Z));
      FACING_HOPPER = DirectionProperty.create("facing", (var0) -> {
         return var0 != Direction.UP;
      });
      RAIL_SHAPE_STRAIGHT = EnumProperty.create("shape", RailShape.class, (Predicate)((shape) -> shape != RailShape.NORTH_EAST && shape != RailShape.SOUTH_EAST));
      HORIZONTAL_FACING = EnumProperty.create("facing", Direction.class, Direction.Plane.HORIZONTAL);
   }
}`,
        CropBlock: `public class CropBlock extends Block {
   public static final IntegerProperty AGE = BlockStateProperties.AGE_7;
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(AGE);
   }
   public IntegerProperty getAgeProperty() { return AGE; }
}`,
        CarrotBlock: 'public class CarrotBlock extends CropBlock { }',
        BeetrootBlock: `public class BeetrootBlock extends CropBlock {
   public static final IntegerProperty AGE = BlockStateProperties.AGE_3;
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(AGE);
   }
}`,
        WetCropBlock: `public class WetCropBlock extends CropBlock {
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      super.createBlockStateDefinition(builder);
      builder.add(BlockStateProperties.WATERLOGGED);
   }
}`,
        SegmentedBlock: `public abstract class SegmentedBlock extends Block {
   public IntegerProperty getAmount() { return BlockStateProperties.AGE_7; }
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(this.getAmount());
   }
}`,
        SmallBlock: `public class SmallBlock extends SegmentedBlock {
   public static final IntegerProperty AMOUNT = BlockStateProperties.AGE_3;
   public IntegerProperty getAmount() { return AMOUNT; }
}`,
        StandBlock: `public class StandBlock extends Block {
   public static final BooleanProperty[] HAS_BOTTLE = new BooleanProperty[]{BlockStateProperties.HAS_BOTTLE_0, BlockStateProperties.HAS_BOTTLE_1};
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(HAS_BOTTLE[0], HAS_BOTTLE[1]);
   }
}`,
        RailBlock: `public class RailBlock extends Block {
   public static final EnumProperty<RailShape> SHAPE = BlockStateProperties.RAIL_SHAPE_STRAIGHT;
   public static final BooleanProperty UNUSED = BooleanProperty.create("unused");
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(SHAPE);
   }
}`,
        PortalBlock: `public class PortalBlock extends Block {
   public static final EnumProperty<Direction.Axis> AXIS = BlockStateProperties.HORIZONTAL_AXIS;
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(AXIS);
   }
}`,
        HopperBlock: `public class HopperBlock extends Block {
   public static final DirectionProperty FACING = BlockStateProperties.FACING_HOPPER;
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(FACING);
   }
}`,
        VinesBlock: `public class VinesBlock extends Block implements Vines {
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(BERRIES);
   }
}`,
        Vines: 'public interface Vines {\n   BooleanProperty BERRIES = BooleanProperty.create("berries");\n}',
        StatueBlock: `public class StatueBlock extends Block {
   public static final EnumProperty<Pose> POSE = EnumProperty.create("pose", StatueBlock.Pose.class);
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(POSE);
   }
   public static enum Pose { STANDING, SITTING; }
}`,
        LoopBlock: `public class LoopBlock extends Block {
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      for(Direction direction : DIRECTIONS) {
         builder.add(getFaceProperty(direction));
      }
   }
}`,
        LostBlock: `public class LostBlock extends Block {
   protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
      builder.add(NOWHERE);
   }
}`,
    };
    const enums = {
        Direction: ['DOWN', 'UP', 'NORTH', 'SOUTH', 'WEST', 'EAST'],
        Axis: ['X', 'Y', 'Z'],
        RailShape: ['NORTH_SOUTH', 'EAST_WEST', 'NORTH_EAST', 'SOUTH_EAST'],
        // the same simple name as the block's nested enum, in another package
        Pose: ['STANDING', 'FLYING', 'SWIMMING'],
    };
    const resolver = stateResolver({
        readClass: name => classes[name] ?? null,
        enumConstantsOf: name => enums[name]?.map(constant => ({ name: constant, value: constant.toLowerCase() })) ?? null,
    });
    const shape = name => resolver.definition(name).properties.map(({ name: field, domain }) => [field, domain?.values.length ?? null]);

    it('inherits a builder it does not override', () => assert.deepEqual(shape('CarrotBlock'), [['AGE', 8]]));

    it('takes an override without super as the replacement it is', () => assert.deepEqual(shape('BeetrootBlock'), [['AGE', 4]]));

    it('adds to the parent\'s through super', () => assert.deepEqual(shape('WetCropBlock'), [['AGE', 8], ['WATERLOGGED', 2]]));

    it('follows a virtual accessor to the most derived override', () => assert.deepEqual(shape('SmallBlock'), [['AMOUNT', 4]]));

    it('takes an array element by its element', () => assert.deepEqual(shape('StandBlock'), [['HAS_BOTTLE_0', 2], ['HAS_BOTTLE_1', 2]]));

    it('counts only what the builder adds, filtered', () => {
        assert.deepEqual(shape('RailBlock'), [['SHAPE', 2]]);
        assert.deepEqual(shape('PortalBlock'), [['AXIS', 2]]);
        assert.deepEqual(resolver.definition('HopperBlock').properties[0].domain.values, ['down', 'north', 'south', 'west', 'east']);
    });

    it('finds a field on an interface', () => assert.deepEqual(shape('VinesBlock'), [['BERRIES', 2]]));

    it('takes the enum nested in the class over a namesake', () => {
        assert.deepEqual(resolver.definition('StatueBlock').properties[0].domain.values, ['standing', 'sitting']);
    });

    it('reports a builder it cannot follow, and a field it cannot find', () => {
        assert.equal(resolver.definition('LoopBlock').dynamic, true);
        assert.deepEqual(shape('LostBlock'), [['NOWHERE', null]]);
    });

    it('sees no definition in a class without one', () => assert.equal(stateDefinitionOf(classes.CarrotBlock), null));
});

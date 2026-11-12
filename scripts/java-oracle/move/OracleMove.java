import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Random;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.FenceBlock;
import net.minecraft.world.level.block.SlabBlock;
import net.minecraft.world.level.block.SnowLayerBlock;
import net.minecraft.world.level.block.StairBlock;
import net.minecraft.world.level.block.TrapDoorBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Half;
import net.minecraft.world.level.block.state.properties.SlabType;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Runs the real Entity.move of a jar over worlds of real block states, for native/fixtures/java-oracle-move-RELEASE.txt.
 * The entity and the level are thin subclasses (GenProbe.java writes their abstract methods); every line of Entity.move
 * that runs is the jar's. The entity is a bare Entity with the player's size and 0.6 step height, not a Player: no
 * sneak back-off, no fall damage.
 *
 * `palette id name n box...`; `scene id n (x y z id)...`;
 * `case scene local cause noPhysics pos3 vel3 onGround h v below minor noBlocks support3 fall stuck3 moves n (delta3 nextTick)...
 *  = (pos3 vel3 onGround h v below minor noBlocks support3 fall stuck3)...`, after each move. A support of NaN is none.
 * cause: 0 SELF, 1 PLAYER, 2 PISTON, 3 SHULKER_BOX, 4 SHULKER, the jar's own MoverType. nextTick: the game time goes on
 * before the move, as the piston's limit asks.
 *
 * java -cp "JARS" scripts/java-oracle/move/GenProbe.java GEN
 * javac -cp JARS -d out GEN/ProbeLevel.java GEN/ProbeEntity.java scripts/java-oracle/move/Hooks.java scripts/java-oracle/move/OracleMove.java
 * java -cp "out;JARS" OracleMove > native/fixtures/java-oracle-move-RELEASE.txt
 */
public final class OracleMove {
    private static final double E = 1.0E-7;
    private static final Random RANDOM = new Random(20261108L);
    private static final double[] SIDES = {
        0.0, 0.0, 0.0, 0.0, E, -E, Math.nextDown(E), Math.nextUp(E), -Math.nextDown(E), -Math.nextUp(E), 1.0E-8, -1.0E-8, 5.0E-8, -5.0E-8, 1.0E-6, -1.0E-6, 1.0E-5, -1.0E-5,
    };
    private static final double[] ACROSS = {
        0.0, 0.0, 0.0, 1.0E-8, -1.0E-8, E, -E, Math.nextDown(E), -Math.nextDown(E), Math.nextUp(E), -Math.nextUp(E), 1.0E-6, -1.0E-6, 0.05, -0.05, 0.1, -0.1, 0.3, -0.3, 0.7, -0.7, 1.2, -1.2,
    };
    private static final double[] UP = {
        -0.08, Math.nextUp(-0.08), Math.nextDown(-0.08), 0.0, 0.0, 0.0, 0.0, -0.0, 1.0E-8, -1.0E-8, E, -E, Math.nextDown(E), -Math.nextDown(E), -0.0784000015258789, -0.0784000015258789, -0.5, -1.5, 0.42, 1.2,
    };
    private static final double[] FALLS = {0.0, 0.0, 0.0, 1.5, 10.0};

    /** Bare Entity.move: the authority is the only thing the subclass answers. */
    public static final class Probe extends ProbeEntity {
        Probe(net.minecraft.world.level.Level level) {
            super(entityType("player"), level);
        }

        @Override public void playSound(net.minecraft.sounds.SoundEvent sound, float volume, float pitch) {
            // (honey's fallOn)
        }

        void boots() throws ReflectiveOperationException {
            // (a field before 1.19.4's maxUpStep(), which ProbeEntity overrides where it can)
            for (Field field : net.minecraft.world.entity.Entity.class.getFields()) {
                if (field.getName().equals("maxUpStep")) {
                    field.setFloat(this, 0.6F);
                }
            }
        }

        void stuck(Vec3 multiplier) {
            this.stuckSpeedMultiplier = multiplier;
        }

        Vec3 stuck() {
            return this.stuckSpeedMultiplier;
        }
    }

    /** The registry of a release: BuiltInRegistries from 1.19.3, Registry before. */
    private static Object registry(String field) {
        for (String holder : new String[] {"net.minecraft.core.registries.BuiltInRegistries", "net.minecraft.core.Registry"}) {
            try {
                return Class.forName(holder).getField(field).get(null);
            } catch (ReflectiveOperationException absent) {
                // the other one
            }
        }
        throw new IllegalStateException(field);
    }

    /** By name: 26.3's Blocks and EntityType hold some of them in maps. */
    private static Object byName(String field, String name) {
        try {
            Object registry = registry(field);
            java.lang.reflect.Method keyOf = null;
            for (java.lang.reflect.Method method : net.minecraft.core.Registry.class.getMethods()) {
                if (method.getName().equals("getKey") && method.getParameterCount() == 1 && method.getParameterTypes()[0] == Object.class) {
                    keyOf = method;
                }
            }
            for (Object value : (Iterable<?>) registry) {
                Object key = keyOf.invoke(registry, value);
                if (key.getClass().getMethod("getPath").invoke(key).equals(name)) {
                    return value;
                }
            }
        } catch (ReflectiveOperationException broken) {
            throw new IllegalStateException(broken);
        }
        throw new IllegalArgumentException(name);
    }

    private static BlockState block(String name) {
        return ((Block) byName("BLOCK", name)).defaultBlockState();
    }

    private static EntityType<?> entityType(String name) {
        return (EntityType<?>) byName("ENTITY_TYPE", name);
    }

    private static double pick(double[] values) {
        return values[RANDOM.nextInt(values.length)];
    }

    /** Entity.mainSupportingBlockPos, of 1.20. */
    private static final boolean HAS_SUPPORT = java.util.Arrays.stream(net.minecraft.world.entity.Entity.class.getFields()).anyMatch(f -> f.getName().equals("mainSupportingBlockPos"));

    @SuppressWarnings("unchecked")
    private static Optional<BlockPos> supportOf(Probe probe) throws ReflectiveOperationException {
        return (Optional<BlockPos>) field(probe, "mainSupportingBlockPos", null);
    }

    /** A public flag of the release, false where it has none yet. */
    private static boolean flag(Probe probe, String name) throws ReflectiveOperationException {
        for (Field field : net.minecraft.world.entity.Entity.class.getFields()) {
            if (field.getName().equals(name)) {
                return field.getBoolean(probe);
            }
        }
        return false;
    }

    private static void seed(Probe probe, String name, boolean value) throws ReflectiveOperationException {
        for (Field field : net.minecraft.world.entity.Entity.class.getFields()) {
            if (field.getName().equals(name)) {
                field.setBoolean(probe, value);
            }
        }
    }

    private static Object field(Object owner, String name, Object value) throws ReflectiveOperationException {
        Field field = net.minecraft.world.entity.Entity.class.getDeclaredField(name);
        field.setAccessible(true);
        if (value != null) {
            field.set(owner, value);
        }
        return field.get(owner);
    }

    private static final List<BlockState> PALETTE = new ArrayList<>();
    private static final List<String> NAMES = new ArrayList<>();

    private static int add(String name, BlockState state) {
        PALETTE.add(state);
        NAMES.add(name);
        return PALETTE.size() - 1;
    }

    private static void palette() {
        add("air", Blocks.AIR.defaultBlockState());
        add("stone", Blocks.STONE.defaultBlockState());
        add("slab", Blocks.STONE_SLAB.defaultBlockState().setValue(SlabBlock.TYPE, SlabType.BOTTOM));
        add("topslab", Blocks.STONE_SLAB.defaultBlockState().setValue(SlabBlock.TYPE, SlabType.TOP));
        add("carpet", block("white_carpet"));
        add("snow2", Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS, 2));
        add("snow3", Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS, 3));
        add("snow8", Blocks.SNOW.defaultBlockState().setValue(SnowLayerBlock.LAYERS, 8));
        add("fence", Blocks.OAK_FENCE.defaultBlockState());
        add("fencex", Blocks.OAK_FENCE.defaultBlockState().setValue(FenceBlock.EAST, true).setValue(FenceBlock.WEST, true));
        add("fencel", Blocks.OAK_FENCE.defaultBlockState().setValue(FenceBlock.NORTH, true).setValue(FenceBlock.EAST, true));
        add("stairs", Blocks.OAK_STAIRS.defaultBlockState().setValue(StairBlock.FACING, Direction.EAST));
        add("stairsup", Blocks.OAK_STAIRS.defaultBlockState().setValue(StairBlock.FACING, Direction.NORTH).setValue(StairBlock.HALF, Half.TOP));
        add("trapdoor", Blocks.OAK_TRAPDOOR.defaultBlockState());
        add("trapdoorup", Blocks.OAK_TRAPDOOR.defaultBlockState().setValue(TrapDoorBlock.HALF, Half.TOP));
        add("soulsand", Blocks.SOUL_SAND.defaultBlockState());
        try {
            add("honey", block("honey_block"));
        } catch (IllegalArgumentException before115) {
            add("ice", Blocks.ICE.defaultBlockState());
        }
        add("slime", Blocks.SLIME_BLOCK.defaultBlockState());
        add("bed", block("white_bed"));
        add("ice", Blocks.ICE.defaultBlockState());
        add("chest", Blocks.CHEST.defaultBlockState());
        add("lantern", Blocks.LANTERN.defaultBlockState());
        add("pane", Blocks.GLASS_PANE.defaultBlockState());
        for (int i = 0; i < PALETTE.size(); i++) {
            List<AABB> boxes = PALETTE.get(i).getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).toAabbs();
            StringBuilder line = new StringBuilder("palette ").append(i).append(' ').append(NAMES.get(i)).append(' ').append(boxes.size());
            for (AABB box : boxes) {
                line.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ)
                    .append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ);
            }
            System.out.println(line);
        }
    }

    private record Scene(Map<BlockPos, BlockState> blocks, double[] start) {}

    /** The solids an edge can be tried against, and what to stand on. */
    private static final int[] SOLID = {1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 20, 21, 22};
    private static final int[] FLOOR = {1, 1, 1, 1, 2, 3, 4, 5, 6, 7, 8, 11, 13, 15, 16, 17, 18, 19};

    private static Scene scene(int id) {
        Map<BlockPos, BlockState> blocks = new HashMap<>();
        // a corner of the map by cell: near the origin, either side of it, and far along the axes
        int[][] origins = {{0, 64, 0}, {-8, 64, -8}, {-1, 64, 7}, {40, -1, -41}, {-1024, 100, 1024}, {29999000, 64, -29999000}};
        int[] o = origins[id % origins.length];
        int floor = FLOOR[RANDOM.nextInt(FLOOR.length)];
        boolean mixed = RANDOM.nextBoolean();
        for (int x = -2; x <= 2; x++) {
            for (int z = -2; z <= 2; z++) {
                if (RANDOM.nextInt(6) != 0) {
                    int cell = mixed ? FLOOR[RANDOM.nextInt(FLOOR.length)] : floor;
                    blocks.put(new BlockPos(o[0] + x, o[1] - 1, o[2] + z), PALETTE.get(cell));
                }
            }
        }
        BlockPos under = new BlockPos(o[0], o[1] - 1, o[2]);
        blocks.put(under, PALETTE.get(floor));
        int kind = RANDOM.nextInt(6);
        int wall = SOLID[RANDOM.nextInt(SOLID.length)];
        int[][] sides = {{1, 0}, {-1, 0}, {0, 1}, {0, -1}};
        int[] side = sides[RANDOM.nextInt(sides.length)];
        if (kind == 1 || kind == 2 || kind == 4) {
            blocks.put(new BlockPos(o[0] + side[0], o[1], o[2] + side[1]), PALETTE.get(wall));
            if (RANDOM.nextBoolean()) {
                blocks.put(new BlockPos(o[0] + side[0], o[1] + 1, o[2] + side[1]), PALETTE.get(SOLID[RANDOM.nextInt(SOLID.length)]));
            }
        }
        if (kind == 2) {
            blocks.put(new BlockPos(o[0] + side[1], o[1], o[2] + side[0]), PALETTE.get(SOLID[RANDOM.nextInt(SOLID.length)]));
        }
        if (kind == 3 || kind == 4) {
            int ceiling = RANDOM.nextBoolean() ? 1 : 3;
            for (int x = -1; x <= 1; x++) {
                for (int z = -1; z <= 1; z++) {
                    blocks.put(new BlockPos(o[0] + x, o[1] + 2, o[2] + z), PALETTE.get(ceiling));
                }
            }
        }
        VoxelShape shape = PALETTE.get(floor).getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO);
        double top = shape.isEmpty() ? 0.0 : shape.max(Direction.Axis.Y);
        return new Scene(blocks, new double[] {o[0], o[1] - 1 + top, o[2]});
    }

    private static void describe(int id, Scene scene) {
        StringBuilder line = new StringBuilder("scene ").append(id).append(' ').append(scene.blocks.size());
        scene.blocks.forEach((pos, state) -> line.append(' ').append(pos.getX()).append(' ').append(pos.getY()).append(' ').append(pos.getZ()).append(' ').append(PALETTE.indexOf(state)));
        System.out.println(line);
    }

    private static void state(StringBuilder out, Probe probe) throws ReflectiveOperationException {
        Vec3 v = probe.getDeltaMovement();
        Vec3 s = probe.stuck();
        Optional<BlockPos> support = HAS_SUPPORT ? supportOf(probe) : Optional.empty();
        out.append(' ').append(probe.getX()).append(' ').append(probe.getY()).append(' ').append(probe.getZ());
        out.append(' ').append(v.x).append(' ').append(v.y).append(' ').append(v.z);
        out.append((boolean) field(probe, "onGround", null) ? " 1" : " 0").append(probe.horizontalCollision ? " 1" : " 0").append(probe.verticalCollision ? " 1" : " 0");
        out.append(flag(probe, "verticalCollisionBelow") ? " 1" : " 0").append(flag(probe, "minorHorizontalCollision") ? " 1" : " 0");
        out.append(HAS_SUPPORT && (boolean) field(probe, "onGroundNoBlocks", null) ? " 1" : " 0");
        if (support.isPresent()) {
            BlockPos pos = support.get();
            out.append(' ').append(pos.getX()).append(' ').append(pos.getY()).append(' ').append(pos.getZ());
        } else {
            out.append(" NaN NaN NaN");
        }
        out.append(' ').append((double) probe.fallDistance);
        out.append(' ').append(s.x).append(' ').append(s.y).append(' ').append(s.z);
    }

    private static double nudge(double value, int ulps) {
        for (int i = 0; i < Math.abs(ulps); i++) {
            value = ulps > 0 ? Math.nextUp(value) : Math.nextDown(value);
        }
        return value;
    }

    /**
     * Mth.equal at its call in Entity.move: a move into a wall that falls short of what was asked by the tolerance, ulps
     * either side, on either horizontal axis and way, for native/fixtures/java-oracle-tolerance-RELEASE.txt.
     */
    private static void tolerance(ProbeLevel level, MoverType[] causes) throws ReflectiveOperationException {
        double[] ask = new double[] {(double) 1.0E-5F, 2.0E-5, 5.0E-6};
        int[][] origins = {{0, 64, 0}, {-8, 64, -8}, {40, 70, -41}, {1000000, 64, -1000000}};
        int id = 0;
        for (int[] o : origins) {
            for (int side = 0; side < 4; side++) {
                int[][] ways = {{1, 0}, {-1, 0}, {0, 1}, {0, -1}};
                int[] way = ways[side];
                Map<BlockPos, BlockState> blocks = new HashMap<>();
                for (int x = -2; x <= 2; x++) {
                    for (int z = -2; z <= 2; z++) {
                        blocks.put(new BlockPos(o[0] + x, o[1] - 1, o[2] + z), PALETTE.get(1));
                    }
                }
                blocks.put(new BlockPos(o[0] + way[0], o[1], o[2] + way[1]), PALETTE.get(1));
                blocks.put(new BlockPos(o[0] + way[0], o[1] + 1, o[2] + way[1]), PALETTE.get(1));
                Scene scene = new Scene(blocks, new double[] {o[0], o[1], o[2]});
                describe(id, scene);
                Hooks.blocks = blocks;
                for (double extra : ask) {
                    for (int ulps = -6; ulps <= 6; ulps++) {
                        for (double place : new double[] {0.5, 0.45, 0.625}) {
                            Probe probe = new Probe(level);
                            probe.boots();
                            Hooks.clientSide(level, false);
                            double x = o[0] + place;
                            double z = o[2] + place;
                            probe.setPos(x, o[1], z);
                            net.minecraft.world.phys.AABB box = probe.getBoundingBox();
                            // the gap to the wall, by the box's own faces
                            double gap = way[0] > 0 ? o[0] + 1 - box.maxX : way[0] < 0 ? o[0] - box.minX : way[1] > 0 ? o[2] + 1 - box.maxZ : o[2] - box.minZ;
                            double length = gap + Math.signum(gap) * nudge(extra, ulps);
                            Vec3 delta = new Vec3(way[0] != 0 ? length : 0.0, 0.0, way[1] != 0 ? length : 0.0);
                            StringBuilder line = new StringBuilder("case ").append(id).append(" 1 0 0");
                            state(line, probe);
                            StringBuilder after = new StringBuilder();
                            line.append(" moves 1 ").append(delta.x).append(' ').append(delta.y).append(' ').append(delta.z).append(" 0");
                            probe.move(causes[0], delta);
                            state(after, probe);
                            System.out.println(line.append(" =").append(after));
                        }
                    }
                }
                id++;
            }
        }
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        java.io.PrintStream out = System.out;
        java.io.PrintStream err = System.err;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        try {
            SharedConstants.class.getMethod("tryDetectVersion").invoke(null);
        } catch (NoSuchMethodException older) {
            // (the release has no such step)
        }
        Bootstrap.bootStrap();
        // (Bootstrap takes the two streams over)
        System.setOut(out);
        System.setErr(err);
        palette();
        Hooks.gameTime = 100L;
        ProbeLevel level = Hooks.allocate(ProbeLevel.class);
        MoverType[] causes = MoverType.values();
        if (args[0].equals("tolerance")) {
            tolerance(level, causes);
            return;
        }
        int scenes = Integer.parseInt(args[0]);
        int perScene = Integer.parseInt(args[1]);
        for (int id = 0; id < scenes; id++) {
            Scene scene = scene(id);
            describe(id, scene);
            Hooks.blocks = scene.blocks;
            for (int c = 0; c < perScene; c++) {
                Probe probe = new Probe(level);
                probe.boots();
                boolean local = RANDOM.nextInt(4) != 0;
                probe.local = local;
                // the instance that owns its entity is the server's here, one that does not a client's
                Hooks.clientSide(level, !local);
                int cause = RANDOM.nextInt(4) == 0 ? 1 + RANDOM.nextInt(causes.length - 1) : 0;
                boolean noPhysics = RANDOM.nextInt(16) == 0;
                probe.noPhysics = noPhysics;
                // the box's own faces at the cell's edges, or the middle of it
                double[] across = {0.0, 0.3, 0.7, 0.5, 0.95, 0.05};
                double x = scene.start[0] + across[RANDOM.nextInt(across.length)] + pick(SIDES);
                double y = scene.start[1] + (RANDOM.nextInt(5) == 0 ? 0.25 : 0.0) + pick(SIDES);
                double z = scene.start[2] + across[RANDOM.nextInt(across.length)] + pick(SIDES);
                probe.setPos(x, y, z);
                Vec3 velocity = new Vec3(pick(ACROSS), pick(UP), pick(ACROSS));
                probe.setDeltaMovement(velocity);
                boolean onGround = RANDOM.nextBoolean();
                boolean noBlocks = RANDOM.nextInt(4) == 0;
                field(probe, "onGround", onGround);
                if (HAS_SUPPORT) {
                    field(probe, "onGroundNoBlocks", noBlocks);
                }
                probe.horizontalCollision = RANDOM.nextBoolean();
                probe.verticalCollision = RANDOM.nextBoolean();
                seed(probe, "verticalCollisionBelow", RANDOM.nextBoolean());
                seed(probe, "minorHorizontalCollision", RANDOM.nextBoolean());
                BlockPos seeded = new BlockPos((int) Math.floor(x), (int) Math.floor(y - 0.5), (int) Math.floor(z));
                Optional<BlockPos> support = RANDOM.nextBoolean() ? Optional.of(seeded) : Optional.empty();
                if (HAS_SUPPORT) {
                    field(probe, "mainSupportingBlockPos", support);
                }
                double fall = pick(FALLS);
                Field fallDistance = net.minecraft.world.entity.Entity.class.getField("fallDistance");
                if (fallDistance.getType() == float.class) {
                    fallDistance.setFloat(probe, (float) fall);
                } else {
                    fallDistance.setDouble(probe, fall);
                }
                probe.stuck(RANDOM.nextInt(6) == 0 ? new Vec3(0.25, 0.05F, 0.25) : Vec3.ZERO);
                int moves = RANDOM.nextInt(4) == 0 ? 2 : 1;
                StringBuilder line = new StringBuilder("case ").append(id).append(local ? " 1 " : " 0 ").append(cause).append(noPhysics ? " 1" : " 0");
                state(line, probe);
                StringBuilder after = new StringBuilder();
                line.append(" moves ").append(moves);
                for (int m = 0; m < moves; m++) {
                    Vec3 delta = m == 0 && RANDOM.nextBoolean() ? velocity : new Vec3(pick(ACROSS), pick(UP), pick(ACROSS));
                    boolean nextTick = RANDOM.nextInt(3) == 0;
                    line.append(' ').append(delta.x).append(' ').append(delta.y).append(' ').append(delta.z).append(nextTick ? " 1" : " 0");
                    if (nextTick) {
                        Hooks.gameTime += 1;
                    }
                    probe.move(causes[cause], delta);
                    state(after, probe);
                }
                System.out.println(line.append(" =").append(after));
            }
        }
    }

    private OracleMove() {}
}

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/**
 * Runs a jar's own LivingEntity for native/fixtures/java-oracle-living-RELEASE.txt: the float intermediates of its
 * movement as the real methods compute them, and whole trajectories of its real aiStep. The entity is a zombie-sized
 * LivingEntity with no AI (serverAiStep never runs) over dry floors, so no baseTick: every other line is the jar's.
 *
 * `palette id name friction speedFactor jumpFactor n box...`; `scene id origin cell` (a floor of the cell at y 63, 32 blocks each way of (origin, -origin));
 * `friction speed friction onGround = speed` (getFrictionInfluencedSpeed, widened);
 * `jump yaw sprint vx vy vz = vx vy vz` (jumpFromGround over no block);
 * `dims width height scale x y z = box6` (EntityDimensions over makeBoundingBox);
 * `walk scene x y z vx vy vz speed ticks (strafe forward yaw jump sprint)... = (x y z vx vy vz onGround fall)...`.
 *
 * java -cp "JARS" scripts/java-oracle/move/GenProbe.java GEN
 * javac -cp JARS -d out GEN/*.java scripts/java-oracle/move/Hooks.java scripts/java-oracle/living/OracleLiving.java
 * java -cp "out;JARS" OracleLiving > native/fixtures/java-oracle-living-RELEASE.txt
 */
public final class OracleLiving {
    private static final Random RANDOM = new Random(20261110L);

    public static final class Probe extends ProbeLiving {
        Probe(net.minecraft.world.level.Level level) {
            super(entityType("zombie"), level);
        }

        @Override protected void pushEntities() {
            // (nobody near)
        }

        @Override protected void serverAiStep() {
            // (no AI: the fixture hands the input in)
        }

        void boots() throws ReflectiveOperationException {
            for (Field field : net.minecraft.world.entity.Entity.class.getFields()) {
                if (field.getName().equals("maxUpStep")) {
                    field.setFloat(this, 0.6F);
                }
            }
        }
    }

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

    private static Object byName(String field, String name) {
        try {
            Object registry = registry(field);
            Method keyOf = null;
            for (Method method : net.minecraft.core.Registry.class.getMethods()) {
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

    private static EntityType<?> entityType(String name) {
        return (EntityType<?>) byName("ENTITY_TYPE", name);
    }

    private static BlockState block(String name) {
        return ((Block) byName("BLOCK", name)).defaultBlockState();
    }

    private static Method method(Class<?> owner, String name, Class<?>... types) throws NoSuchMethodException {
        Method method = owner.getDeclaredMethod(name, types);
        method.setAccessible(true);
        return method;
    }

    private static final List<BlockState> PALETTE = new ArrayList<>();

    private static void palette() {
        String[] names = {"stone", "ice", "blue_ice", "slime_block", "soul_sand"};
        for (int i = 0; i < names.length; i++) {
            BlockState state = block(names[i]);
            PALETTE.add(state);
            List<AABB> boxes = state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).toAabbs();
            Block kind = state.getBlock();
            StringBuilder line = new StringBuilder("palette ").append(i).append(' ').append(names[i]).append(' ')
                .append((double) kind.getFriction()).append(' ').append((double) kind.getSpeedFactor()).append(' ').append((double) kind.getJumpFactor()).append(' ').append(boxes.size());
            for (AABB box : boxes) {
                line.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ)
                    .append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ);
            }
            System.out.println(line);
        }
    }

    /** Entity.onGround: a method and a private field from 1.20, a public field before. */
    private static boolean grounded(Probe probe) throws ReflectiveOperationException {
        Field field = net.minecraft.world.entity.Entity.class.getDeclaredField("onGround");
        field.setAccessible(true);
        return field.getBoolean(probe);
    }

    /** Entity.fallDistance: a float before 1.21.5, a double from it. */
    private static double fall(Probe probe) throws ReflectiveOperationException {
        return net.minecraft.world.entity.Entity.class.getField("fallDistance").getDouble(probe);
    }

    private static Probe fresh(ProbeLevel level) throws ReflectiveOperationException {
        Probe probe = new Probe(level);
        probe.boots();
        return probe;
    }

    private static void friction(ProbeLevel level) throws ReflectiveOperationException {
        Method influenced;
        try {
            influenced = method(LivingEntity.class, "getFrictionInfluencedSpeed", float.class);
        } catch (NoSuchMethodException older) {
            return;
        }
        float[] frictions = {0.6F, Math.nextUp(0.6F), Math.nextDown(0.6F), 0.98F, 0.989F, 0.8F, 0.5F, 0.4F, 0.1F, 1.0F, Float.MIN_VALUE, 0.7F};
        float[] speeds = {0.1F, 0.13F, 0.23F, 0.0F, 0.30000001F, 0.7F, 0.1F * 1.3F, 1.0E-10F, 1024.0F};
        for (float friction : frictions) {
            for (float speed : speeds) {
                Probe probe = fresh(level);
                probe.setSpeed(speed);
                Field ground = net.minecraft.world.entity.Entity.class.getDeclaredField("onGround");
                ground.setAccessible(true);
                ground.setBoolean(probe, true);
                float out = (float) influenced.invoke(probe, friction);
                System.out.println("friction " + (double) speed + " " + (double) friction + " 1 = " + (double) out);
            }
        }
    }

    private static void jump(ProbeLevel level) throws ReflectiveOperationException {
        Method jump = method(LivingEntity.class, "jumpFromGround");
        float[] yaws = {0.0F, 90.0F, -90.0F, 180.0F, 45.0F, 33.3F, -179.9F, 359.99F, 1.0E-5F, 1234567.9F, -0.0F};
        double[][] velocities = {{0.0, -0.0784000015258789, 0.0}, {0.05, 0.2, -0.05}, {0.0, 0.41999998688697815, 0.0}, {-0.0, 0.5, 1.0E-9}};
        for (float yaw : yaws) {
            for (int sprint = 0; sprint < 2; sprint++) {
                for (double[] v : velocities) {
                    Probe probe = fresh(level);
                    probe.setPos(0.5, 64.0, 0.5);
                    probe.setYRot(yaw);
                    probe.setSprinting(sprint == 1);
                    probe.setDeltaMovement(v[0], v[1], v[2]);
                    jump.invoke(probe);
                    Vec3 after = probe.getDeltaMovement();
                    System.out.println("jump " + (double) yaw + " " + sprint + " " + v[0] + " " + v[1] + " " + v[2] + " = " + after.x + " " + after.y + " " + after.z);
                }
            }
        }
    }

    private static void dimensions() throws ReflectiveOperationException {
        double[][] places = {{0.0, 64.0, 0.0}, {-0.3, 64.0, 0.3}, {-12345.678, -59.0, 29999000.25}, {0.1, 0.2, 0.3}};
        float[][] sizes = {{0.6F, 1.8F}, {0.6F, 1.5F}, {0.6F, 0.6F}, {0.2F, 0.2F}, {0.0F, 0.0F}, {1.3964844F, 1.6F}};
        float[] scales = {1.0F, 0.0625F, 16.0F, 0.15F, 1.1F, 3.3333333F};
        Class<?> dims = net.minecraft.world.entity.EntityDimensions.class;
        Method box;
        try {
            box = dims.getMethod("makeBoundingBox", double.class, double.class, double.class);
        } catch (NoSuchMethodException older) {
            return;
        }
        Method scalable = dims.getMethod("scalable", float.class, float.class);
        Method scale = dims.getMethod("scale", float.class);
        for (float[] size : sizes) {
            for (float by : scales) {
                for (double[] at : places) {
                    AABB out = (AABB) box.invoke(scale.invoke(scalable.invoke(null, size[0], size[1]), by), at[0], at[1], at[2]);
                    System.out.println("dims " + (double) size[0] + " " + (double) size[1] + " " + (double) by + " " + at[0] + " " + at[1] + " " + at[2]
                        + " = " + out.minX + " " + out.minY + " " + out.minZ + " " + out.maxX + " " + out.maxY + " " + out.maxZ);
                }
            }
        }
    }

    private static Map<BlockPos, BlockState> floor(int origin, int cell) {
        Map<BlockPos, BlockState> blocks = new HashMap<>();
        for (int x = -32; x <= 32; x++) {
            for (int z = -32; z <= 32; z++) {
                blocks.put(new BlockPos(origin + x, 63, -origin + z), PALETTE.get(cell));
            }
        }
        return blocks;
    }

    private static void walks(ProbeLevel level) throws ReflectiveOperationException {
        int[] origins = {0, -1024, 29999000};
        float[] speeds = {0.1F, 0.23F, 0.13F, 0.30000001F};
        int id = 0;
        for (int origin : origins) {
            for (int cell : new int[] {0, 1, 2}) {
                Map<BlockPos, BlockState> blocks = floor(origin, cell);
                System.out.println("scene " + id + " " + origin + " " + cell);
                Hooks.blocks = blocks;
                for (int c = 0; c < 6; c++) {
                    Probe probe = fresh(level);
                    double x = origin + RANDOM.nextDouble() * 4.0 - 2.0;
                    double z = -origin + RANDOM.nextDouble() * 4.0 - 2.0;
                    double y = 64.0 + (c % 2) * 3.25;
                    probe.setPos(x, y, z);
                    float speed = speeds[RANDOM.nextInt(speeds.length)];
                    int ticks = 60;
                    StringBuilder line = new StringBuilder("walk ").append(id).append(' ').append(x).append(' ').append(y).append(' ').append(z).append(" 0.0 0.0 0.0 ").append((double) speed).append(' ').append(ticks);
                    StringBuilder after = new StringBuilder();
                    float yaw = RANDOM.nextFloat() * 720.0F - 360.0F;
                    float forward = 1.0F;
                    float strafe = 0.0F;
                    boolean sprint = false;
                    for (int t = 0; t < ticks; t++) {
                        if (RANDOM.nextInt(8) == 0) {
                            yaw += RANDOM.nextFloat() * 90.0F - 45.0F;
                            forward = new float[] {1.0F, 0.0F, -1.0F, 0.3F}[RANDOM.nextInt(4)];
                            strafe = new float[] {0.0F, 1.0F, -1.0F, 0.3F}[RANDOM.nextInt(4)];
                        }
                        boolean jumping = RANDOM.nextInt(5) == 0;
                        if (RANDOM.nextInt(12) == 0) {
                            sprint = !sprint;
                        }
                        probe.setYRot(yaw);
                        probe.xxa = strafe;
                        probe.zza = forward;
                        probe.setSprinting(sprint);
                        probe.setSpeed(speed);
                        probe.setJumping(jumping);
                        line.append(' ').append((double) strafe).append(' ').append((double) forward).append(' ').append((double) yaw).append(jumping ? " 1" : " 0").append(sprint ? " 1" : " 0");
                        Hooks.gameTime += 1;
                        probe.aiStep();
                        Vec3 v = probe.getDeltaMovement();
                        after.append(' ').append(probe.getX()).append(' ').append(probe.getY()).append(' ').append(probe.getZ());
                        after.append(' ').append(v.x).append(' ').append(v.y).append(' ').append(v.z);
                        after.append(grounded(probe) ? " 1" : " 0").append(' ').append(fall(probe));
                    }
                    System.out.println(line.append(" =").append(after));
                }
                id++;
            }
        }
    }

    /** Motion that is not finite, in by setDeltaMovement: what the real tick makes of it. After walks: their lines stay as they were. */
    private static void notFinite(ProbeLevel level, boolean finiteOnly) throws ReflectiveOperationException {
        Map<BlockPos, BlockState> blocks = floor(0, 0);
        System.out.println("scene 900 0 0");
        Hooks.blocks = blocks;
        double[][] starts = {
            {Double.NaN, 0.0, 0.0}, {0.0, Double.NaN, 0.0}, {0.0, 0.0, Double.NaN},
            {Double.NaN, Double.NaN, Double.NaN},
            {Double.POSITIVE_INFINITY, 0.0, 0.0}, {0.0, Double.NEGATIVE_INFINITY, 0.0},
        };
        // (`no-infinite`: before 1.21.9 or so a jar's collision walk of an infinite move does not end, so only the NaN starts run)
        int count = finiteOnly ? 4 : starts.length;
        for (double y : new double[] {64.0, 67.25}) {
            for (int which = 0; which < count; which++) {
                double[] start = starts[which];
                Probe probe = fresh(level);
                probe.setPos(0.5, y, 0.5);
                probe.setDeltaMovement(start[0], start[1], start[2]);
                int ticks = 4;
                StringBuilder line = new StringBuilder("walk 900 0.5 ").append(y).append(" 0.5 ").append(start[0]).append(' ').append(start[1]).append(' ').append(start[2]).append(" 0.1 ").append(ticks);
                StringBuilder after = new StringBuilder();
                try {
                    for (int t = 0; t < ticks; t++) {
                        probe.setYRot(0.0F);
                        probe.xxa = 0.0F;
                        probe.zza = 1.0F;
                        probe.setSpeed(0.1F);
                        line.append(" 0.0 1.0 0.0 0 0");
                        Hooks.gameTime += 1;
                        probe.aiStep();
                        Vec3 v = probe.getDeltaMovement();
                        after.append(' ').append(probe.getX()).append(' ').append(probe.getY()).append(' ').append(probe.getZ());
                        after.append(' ').append(v.x).append(' ').append(v.y).append(' ').append(v.z);
                        after.append(grounded(probe) ? " 1" : " 0").append(' ').append(fall(probe));
                    }
                    System.out.println(line.append(" =").append(after));
                } catch (RuntimeException thrown) {
                    System.out.println("# threw " + start[0] + " " + start[1] + " " + start[2] + " at " + y + ": " + thrown);
                }
            }
        }
    }

    /** jumpFromGround on each side of its 1.0E-5F, by the JUMP_STRENGTH attribute: `jumpat strength vy = vy`. */
    private static void jumpRejection(ProbeLevel level) throws ReflectiveOperationException {
        Field strength;
        try {
            strength = net.minecraft.world.entity.ai.attributes.Attributes.class.getField("JUMP_STRENGTH");
        } catch (NoSuchFieldException before) {
            return;
        }
        Object attribute = strength.get(null);
        Method instance = null;
        for (Method method : LivingEntity.class.getMethods()) {
            if (method.getName().equals("getAttribute") && method.getParameterTypes()[0].isInstance(attribute)) {
                instance = method;
            }
        }
        Method jump = method(LivingEntity.class, "jumpFromGround");
        double rejected = (double) 1.0E-5F;
        double[] strengths = {0.0, rejected, Math.nextDown(rejected), Math.nextUp(rejected), (double) Math.nextUp(1.0E-5F), (double) Math.nextDown(1.0E-5F), 1.1E-5, 9.0E-6, 0.41999998688697815, 1.0};
        for (double value : strengths) {
            for (double vy : new double[] {-0.0784000015258789, 0.5, 0.0}) {
                Probe probe = fresh(level);
                probe.setPos(0.5, 64.0, 0.5);
                probe.setDeltaMovement(0.0, vy, 0.0);
                Object held = instance.invoke(probe, attribute);
                if (held == null) {
                    return; // (before 1.20.5 the attribute is a horse's)
                }
                held.getClass().getMethod("setBaseValue", double.class).invoke(held, value);
                jump.invoke(probe);
                System.out.println("jumpat " + value + " " + vy + " = " + probe.getDeltaMovement().y);
            }
        }
    }

    /** A player's sizes by pose, which Entity.refreshDimensions fits the box to: `pose name width height`. */
    private static void poses() throws ReflectiveOperationException {
        for (String holder : new String[] {"net.minecraft.world.entity.Avatar", "net.minecraft.world.entity.player.Player"}) {
            Field field;
            try {
                field = Class.forName(holder).getDeclaredField("POSES");
            } catch (ClassNotFoundException | NoSuchFieldException elsewhere) {
                continue;
            }
            field.setAccessible(true);
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) field.get(null)).entrySet()) {
                Object dimensions = entry.getValue();
                Object width;
                Object height;
                try {
                    width = dimensions.getClass().getMethod("width").invoke(dimensions);
                    height = dimensions.getClass().getMethod("height").invoke(dimensions);
                } catch (NoSuchMethodException fields) {
                    width = dimensions.getClass().getField("width").get(dimensions);
                    height = dimensions.getClass().getField("height").get(dimensions);
                }
                System.out.println("pose " + entry.getKey() + " " + ((Float) width).doubleValue() + " " + ((Float) height).doubleValue());
            }
            return;
        }
    }

    public static void main(String[] args) throws Exception {
        java.io.PrintStream out = System.out;
        java.io.PrintStream err = System.err;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        try {
            SharedConstants.class.getMethod("tryDetectVersion").invoke(null);
        } catch (NoSuchMethodException older) {
            // (the release has no such step)
        }
        Bootstrap.bootStrap();
        System.setOut(out);
        System.setErr(err);
        palette();
        Hooks.gameTime = 100L;
        ProbeLevel level = Hooks.allocate(ProbeLevel.class);
        Hooks.clientSide(level, false);
        if (args.length > 0 && args[0].equals("knockback")) {
            knockback(level);
            return;
        }
        friction(level);
        jump(level);
        dimensions();
        walks(level);
        notFinite(level, args.length > 0 && args[0].equals("no-infinite"));
        jumpRejection(level);
        poses();
    }

    /**
     * `knockback strength x z resistance ground vx vy vz = vx vy vz` of the jar's own LivingEntity.knockback, the release's
     * signature found by its arguments, for native/fixtures/java-oracle-knockback.txt: `java ... OracleLiving knockback`. A
     * release that rolls the resistance (to 1.15) is asked at 0 alone.
     */
    private static void knockback(ProbeLevel level) throws ReflectiveOperationException {
        Method taken = null;
        for (Method m : LivingEntity.class.getDeclaredMethods()) {
            if (m.getName().equals("knockback") && m.getReturnType() == void.class && m.getParameterCount() >= 3 && m.getParameterCount() <= 6
                && (taken == null || m.getParameterCount() > taken.getParameterCount())) taken = m;
        }
        taken.setAccessible(true);
        double[] strengths = {0.4000000059604645, 0.5, 0.9, 0.0};
        double[][] directions = {{1.0, 0.0}, {0.0, 1.0}, {-0.6, 0.8}, {0.31, -0.97}, {2.5, 1.25}, {1.0E-3, 1.0E-3}, {0.00317, 0.0}, {3.3E-3, 3.3E-3}, {3.2E-3, -3.0E-5}};
        double[] resistances = {0.0, 0.25, 0.5, 0.6, 1.0, 1.25};
        double[][] velocities = {{0.0, -0.0784000015258789, 0.0}, {0.2, 0.3, -0.1}, {1.0E-4, 0.79, 2.0}};
        boolean rolls = taken.getParameterTypes()[0] != float.class && taken.getParameterTypes()[0] != double.class;
        for (double strength : strengths) {
            for (double[] direction : directions) {
                for (double resistance : resistances) {
                    if (rolls && resistance != 0.0) continue;
                    for (double[] v : velocities) {
                        for (int ground = 0; ground < 2; ground++) {
                            Probe probe = fresh(level);
                            Field onGround = net.minecraft.world.entity.Entity.class.getDeclaredField("onGround");
                            onGround.setAccessible(true);
                            onGround.setBoolean(probe, ground == 1);
                            probe.setDeltaMovement(new Vec3(v[0], v[1], v[2]));
                            probe.getAttribute(net.minecraft.world.entity.ai.attributes.Attributes.KNOCKBACK_RESISTANCE).setBaseValue(resistance);
                            Class<?>[] types = taken.getParameterTypes();
                            Object[] arguments = new Object[types.length];
                            int at = rolls ? 1 : 0;
                            arguments[at] = types[at] == float.class ? (Object) (float) strength : (Object) strength;
                            arguments[at + 1] = direction[0];
                            arguments[at + 2] = direction[1];
                            for (int i = at + 3; i < types.length; i++) arguments[i] = types[i] == float.class ? (Object) 0.0F : types[i] == boolean.class ? (Object) false : null;
                            taken.invoke(probe, arguments);
                            Vec3 out = probe.getDeltaMovement();
                            System.out.println("knockback " + strength + " " + direction[0] + " " + direction[1] + " " + resistance + " " + ground + " " + v[0] + " " + v[1] + " " + v[2] + " = " + out.x + " " + out.y + " " + out.z);
                        }
                    }
                }
            }
        }
    }

    private OracleLiving() {}
}

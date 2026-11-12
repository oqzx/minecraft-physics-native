import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * The composite operations Oracle.java leaves out, from the real classes of a jar, for
 * native/fixtures/java-oracle-geometry-RELEASE.txt, in its `op inputs... = outputs...` lines: the face and hit of a clip,
 * the ordered boxes and the movement of a union before and after VoxelShape.optimize, sequences of vector operations
 * that reassociation would change, Entity.getInputVector with its casts, the table lookups at very large and non-finite
 * angles, and Math.sin and Math.cos.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleGeometry.java
 * java -cp "out;JARS" OracleGeometry > native/fixtures/java-oracle-geometry-RELEASE.txt
 *
 * JARS is the Mojang-named jar with fastutil and guava (and its other libraries for the Entity lines).
 */
public final class OracleGeometry {
    private static final Random RANDOM = new Random(20261109L);
    private static final double E = 1.0E-7;
    private static final double[] LINES = {0.0, 0.125, 0.25, 0.375, 0.5, 0.75, 0.875, 1.0};
    private static final double[] SIDES = {0.0, 0.0, 0.0, 0.0, 5.0E-8, -5.0E-8, E, -E, Math.nextUp(E), Math.nextDown(E), 1.5E-7, -1.5E-7, 3.0E-7};
    private static final double[] MOVES = {0.0, 0.0, 0.25, -0.25, 1.0 / 3.0, -0.1, 16.0, -32.0, 1.0E-7, 3.0E7, 0.1 + 0.2};

    private static double pick(double[] values) {
        return values[RANDOM.nextInt(values.length)];
    }

    private static String text(double[] values) {
        StringBuilder out = new StringBuilder();
        for (double value : values) {
            out.append(' ').append(value);
        }
        return out.toString();
    }

    private static void line(String op, double[] in, double... result) {
        System.out.println(op + text(in) + " =" + text(result));
    }

    private static double[] join(double[]... parts) {
        int size = 0;
        for (double[] part : parts) {
            size += part.length;
        }
        double[] out = new double[size];
        int at = 0;
        for (double[] part : parts) {
            System.arraycopy(part, 0, out, at, part.length);
            at += part.length;
        }
        return out;
    }

    private static double[] of(AABB b) {
        return new double[] {b.minX, b.minY, b.minZ, b.maxX, b.maxY, b.maxZ};
    }

    private static double[] of(Vec3 v) {
        return new double[] {v.x, v.y, v.z};
    }

    private static double[] boxes(VoxelShape shape) {
        List<double[]> out = new ArrayList<>();
        for (AABB box : shape.toAabbs()) {
            out.add(of(box));
        }
        return join(out.toArray(new double[0][]));
    }

    private static double[] randomBox() {
        double[] box = new double[6];
        for (int axis = 0; axis < 3; axis++) {
            double low = LINES[RANDOM.nextInt(LINES.length - 1)] + pick(SIDES);
            double high = low + 0.125 * (1 + RANDOM.nextInt(4)) + pick(SIDES);
            box[axis] = Math.max(0.0, low);
            box[axis + 3] = Math.min(1.0, high);
            if (box[axis + 3] <= box[axis]) {
                box[axis] = 0.0;
                box[axis + 3] = 1.0;
            }
        }
        return box;
    }

    /** n, each box's 6 and its move's 3. */
    private static void union() {
        for (int i = 0; i < 260; i++) {
            int n = 1 + RANDOM.nextInt(5);
            double[] in = {n};
            VoxelShape[] shapes = new VoxelShape[n];
            for (int s = 0; s < n; s++) {
                double[] box = s > 0 && RANDOM.nextInt(5) == 0 ? java.util.Arrays.copyOfRange(in, 1, 7) : randomBox();
                double[] move = RANDOM.nextInt(4) == 0 ? new double[] {pick(MOVES), pick(MOVES), pick(MOVES)} : new double[3];
                in = join(in, box, move);
                shapes[s] = Shapes.create(new AABB(box[0], box[1], box[2], box[3], box[4], box[5])).move(move[0], move[1], move[2]);
            }
            VoxelShape first = shapes[0];
            VoxelShape[] rest = java.util.Arrays.copyOfRange(shapes, 1, n);
            VoxelShape joined = Shapes.or(first, rest);
            line("union", in, boxes(joined));
            // a box against the result, from outside and from inside
            AABB bounds = joined.isEmpty() ? new AABB(0, 0, 0, 1, 1, 1) : joined.bounds();
            for (int probe = 0; probe < 2; probe++) {
                Direction.Axis axis = Direction.Axis.values()[RANDOM.nextInt(3)];
                double[] moving = {
                    bounds.minX - 0.3 + RANDOM.nextDouble() * 1.2, bounds.minY - 0.3 + RANDOM.nextDouble() * 1.2, bounds.minZ - 0.3 + RANDOM.nextDouble() * 1.2, 0, 0, 0
                };
                moving[3] = moving[0] + 0.1 + RANDOM.nextDouble() * 0.6;
                moving[4] = moving[1] + 0.1 + RANDOM.nextDouble() * 0.6;
                moving[5] = moving[2] + 0.1 + RANDOM.nextDouble() * 0.6;
                double delta = (RANDOM.nextDouble() - 0.5) * 3.0;
                AABB box = new AABB(moving[0], moving[1], moving[2], moving[3], moving[4], moving[5]);
                line("unioncollide", join(in, new double[] {axis.ordinal()}, moving, new double[] {delta}), joined.collide(axis, box, delta));
                if (n == 2) {
                    // the same query before the optimization of Shapes.or
                    VoxelShape raw = Shapes.joinUnoptimized(first, rest[0], BooleanOp.OR);
                    line("rawcollide", join(in, new double[] {axis.ordinal()}, moving, new double[] {delta}), raw.collide(axis, box, delta));
                }
            }
        }
    }

    private static double ordinal(Direction direction) {
        return direction.ordinal();
    }

    /** box, from, to: the hit and its Direction (0 down, 1 up, 2 north, 3 south, 4 west, 5 east), nothing for a miss. */
    private static void clips() {
        double[][] shapes = {
            {0, 0, 0, 1, 1, 1},
            {0, 0, 0, 1, 0.5, 1},
            {0.25, 0, 0.25, 0.75, 1, 0.75},
            {0, 0.75, 0, 1, 1, 1},
        };
        double[] ends = {-1.0, -E, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0 + E, 2.0, Math.nextUp(1.0), Math.nextDown(0.0)};
        for (double[] b : shapes) {
            AABB box = new AABB(b[0], b[1], b[2], b[3], b[4], b[5]);
            for (int i = 0; i < 100; i++) {
                double[] from = {pick(ends), pick(ends), pick(ends)};
                double[] to = {pick(ends), pick(ends), pick(ends)};
                if (i % 4 == 0) {
                    to[i % 3] = from[i % 3];
                }
                Vec3 a = new Vec3(from[0], from[1], from[2]);
                Vec3 c = new Vec3(to[0], to[1], to[2]);
                BlockHitResult hit = AABB.clip(List.of(box), a, c, BlockPos.ZERO);
                line("clipface", join(b, from, to), hit == null ? new double[0] : join(of(hit.getLocation()), new double[] {ordinal(hit.getDirection())}));
            }
        }
        // VoxelShape.clip over a block of two boxes at a position of the map: ties, a start inside, the 0.001 step
        double[][] positions = {{0, 64, 0}, {-3, 70, 5}, {1024, 5, -4097}};
        for (double[] p : positions) {
            VoxelShape shape = Shapes.or(Shapes.box(0, 0, 0, 1, 0.5, 1), Shapes.box(0.5, 0.5, 0, 1, 1, 1));
            BlockPos pos = new BlockPos((int) p[0], (int) p[1], (int) p[2]);
            for (int i = 0; i < 120; i++) {
                double[] from = {p[0] + pick(ends), p[1] + pick(ends), p[2] + pick(ends)};
                double[] to = {p[0] + pick(ends), p[1] + pick(ends), p[2] + pick(ends)};
                BlockHitResult hit = shape.clip(new Vec3(from[0], from[1], from[2]), new Vec3(to[0], to[1], to[2]), pos);
                line("shapeclip", join(p, from, to), hit == null ? new double[0] : join(of(hit.getLocation()), new double[] {ordinal(hit.getDirection()), hit.isInside() ? 1.0 : 0.0}));
            }
        }
    }

    private static Vec3 vector(double[] a, int at) {
        return new Vec3(a[at], a[at + 1], a[at + 2]);
    }

    /** Where the order of the operations shows: huge and tiny together, values that cancel, signed zeros. */
    private static void vectors() throws ReflectiveOperationException {
        Method projectedOn = null;
        for (Method method : Vec3.class.getMethods()) {
            if (method.getName().equals("projectedOn")) {
                projectedOn = method;
            }
        }
        double[] values = {1.0E16, -1.0E16, 1.0, -1.0, 0.1, 0.2, 0.3, 1.0E-16, 3.0E7, -3.0E7, 0.0, -0.0, 1.0E-300, 1.0E300, 0.05000000074505806};
        for (int i = 0; i < 120; i++) {
            double[] in = new double[12];
            for (int k = 0; k < 12; k++) {
                in[k] = pick(values);
            }
            Vec3 a = vector(in, 0);
            Vec3 b = vector(in, 3);
            Vec3 c = vector(in, 6);
            double t = in[9];
            line("vassoc", in, join(of(a.add(b).add(c)), of(a.add(b.add(c)))));
            line("vcancel", in, of(a.add(b).subtract(b)));
            line("vscaleadd", in, of(a.add(b).scale(t)));
            line("vdist", in, of(a.subtract(b).multiply(c)));
            if (projectedOn != null) {
                line("vproject2", in, of((Vec3) projectedOn.invoke((Vec3) projectedOn.invoke(a, b), c)));
            }
        }
    }

    /** Entity.getInputVector: the input, the speed and yaw as floats, by the table of the release. */
    private static void inputs() throws ReflectiveOperationException {
        java.io.PrintStream out = System.out;
        java.io.PrintStream err = System.err;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        net.minecraft.SharedConstants.class.getMethod("tryDetectVersion").invoke(null);
        net.minecraft.server.Bootstrap.bootStrap();
        System.setOut(out);
        System.setErr(err);
        Method method = Entity.class.getDeclaredMethod("getInputVector", Vec3.class, float.class, float.class);
        method.setAccessible(true);
        double[] parts = {0.0, 1.0, -1.0, 0.98F, -0.98F, 0.3F, 0.7071F, 1.0E-4, 3.2E-4, 1.5};
        float[] speeds = {0.1F, 0.02F, 0.13F, 0.06F, 0.2806F, 0.026000002F};
        for (int i = 0; i < 300; i++) {
            double[] in = {pick(parts), pick(parts), pick(parts)};
            float speed = speeds[RANDOM.nextInt(speeds.length)];
            float yaw = i % 5 == 0 ? (float) (RANDOM.nextInt(16) * 45) : (RANDOM.nextFloat() - 0.5F) * 720.0F;
            Vec3 moved = (Vec3) method.invoke(null, new Vec3(in[0], in[1], in[2]), speed, yaw);
            line("inputvec", join(in, new double[] {speed, yaw}), of(moved));
        }
    }

    private static double table(String name, double radians) throws ReflectiveOperationException {
        Class<?> mth = net.minecraft.util.Mth.class;
        try {
            return (float) mth.getMethod(name, double.class).invoke(null, radians);
        } catch (NoSuchMethodException floats) {
            return (float) mth.getMethod(name, float.class).invoke(null, (float) radians);
        }
    }

    private static void trig() throws ReflectiveOperationException {
        double quarter = Math.PI / 2.0;
        List<Double> angles = new ArrayList<>(List.of(0.0, -0.0, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.MAX_VALUE, -Double.MAX_VALUE, Double.MIN_VALUE));
        for (int turn = -8; turn <= 8; turn++) {
            double angle = turn * quarter;
            angles.add(angle);
            angles.add(Math.nextUp(angle));
            angles.add(Math.nextDown(angle));
        }
        for (double scale : new double[] {1.0E6, 1.0E7, 1.0E8, 1.0E9, 1.0E10, 2.147483647E9, 2.147483648E9, 1.0E12, 1.0E15, 1.0E16, 1.0E19, 1.0E30, 1.0E300}) {
            angles.add(scale);
            angles.add(-scale);
            angles.add(scale * 1.2345678);
        }
        // lookups on a table index and the half index, where the conversion of the argument decides
        for (int i = 0; i < 60; i++) {
            double index = RANDOM.nextInt(65536) + (i % 2 == 0 ? 0.0 : 0.5);
            angles.add(index / 10430.378350470453);
            angles.add(-index / 10430.378350470453);
        }
        for (double angle : angles) {
            line("sin", new double[] {angle}, table("sin", angle));
            line("cos", new double[] {angle}, table("cos", angle));
        }
        // Math.sin and Math.cos, which trig.rs answers rounded once: within its range of about a million radians
        for (int i = 0; i < 200; i++) {
            double angle = (RANDOM.nextDouble() - 0.5) * Math.pow(10.0, RANDOM.nextInt(7) - 1);
            line("jsin", new double[] {angle}, Math.sin(angle));
            line("jcos", new double[] {angle}, Math.cos(angle));
        }
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        vectors();
        union();
        clips();
        trig();
        try {
            inputs();
        } catch (LinkageError | NoSuchMethodException absent) {
            // (a jar run without its libraries, or one before this signature)
        }
    }

    private OracleGeometry() {}
}

import java.util.Random;
import net.minecraft.core.Direction;
import net.minecraft.util.Mth;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Runs the real AABB, Vec3, Mth and Shapes classes of a vanilla jar over a fixed seed and prints each operation
 * with its inputs and result, for native/fixtures/java-oracle-*.txt. Each line is `op inputs... = outputs...`,
 * doubles in Double.toString's shortest form, which Rust parses back to the same bits. A method the jar does not
 * have yet leaves its lines out.
 *
 * javac -cp JARS -d out scripts/java-oracle/Oracle.java
 * java -cp "out;JARS" Oracle > native/fixtures/java-oracle-RELEASE.txt
 *
 * JARS is the release's jar with Mojang's names (fabric-loom's minecraft-SERVER-only.jar for 26.x, its
 * minecraft-merged jar before) and the libraries of the release's version json.
 */
public final class Oracle {
    private static final Random RANDOM = new Random(20261105L);
    private static final double E = 1.0E-7;
    /** Faces and the sides of them: the values around 0, 1, the box and the collision epsilon. */
    private static final double[] EDGES = {
        -2.5, -1.0, -0.5, -E, -Math.nextUp(-E), -0.0, 0.0, E, Math.nextDown(E), Math.nextUp(E), 0.25, 0.5, 1.0,
        Math.nextUp(1.0), Math.nextDown(1.0), 1.5, 2.0, 1.0E-5, (double) 1.0E-5F, 0.001, 0.1, 3.0E7
    };

    private static double edge() {
        return EDGES[RANDOM.nextInt(EDGES.length)];
    }

    private static double mixed() {
        return switch (RANDOM.nextInt(3)) {
            case 0 -> edge();
            case 1 -> (RANDOM.nextDouble() - 0.5) * 8.0;
            default -> edge() + (RANDOM.nextDouble() - 0.5) * 1.0E-6;
        };
    }

    /** An operation of the jar: a method this release does not have yet leaves the operation out. */
    private interface Result {
        double[] get();
    }

    private static void line(String op, double[] in, Result result) {
        try {
            line(op, in, result.get());
        } catch (LinkageError absent) {
            // (older jars)
        }
    }

    private static void line(String op, double[] in, double... out) {
        StringBuilder text = new StringBuilder(op);
        for (double value : in) {
            text.append(' ').append(value);
        }
        text.append(" =");
        for (double value : out) {
            text.append(' ').append(value);
        }
        System.out.println(text);
    }

    private static double[] six() {
        double[] values = new double[6];
        for (int i = 0; i < 6; i++) {
            values[i] = mixed();
        }
        return values;
    }

    private static double[] join(double[] a, double... b) {
        double[] out = new double[a.length + b.length];
        System.arraycopy(a, 0, out, 0, a.length);
        System.arraycopy(b, 0, out, a.length, b.length);
        return out;
    }

    private static AABB box(double[] a) {
        return new AABB(a[0], a[1], a[2], a[3], a[4], a[5]);
    }

    private static double[] of(AABB b) {
        return new double[] {b.minX, b.minY, b.minZ, b.maxX, b.maxY, b.maxZ};
    }

    static double[] of(Vec3 v) {
        return new double[] {v.x, v.y, v.z};
    }

    private static double bool(boolean value) {
        return value ? 1.0 : 0.0;
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        boxes();
        vectors();
        mth();
        trig();
        try {
            collide();
        } catch (LinkageError streams) {
            // (Shapes.collide takes a Stream before 1.17)
        }
    }

    /** Mth.sin by the signature of the jar: a float until 1.21.11, a double after. */
    private static double sin(String name, double radians) throws ReflectiveOperationException {
        try {
            return (float) Mth.class.getMethod(name, double.class).invoke(null, radians);
        } catch (NoSuchMethodException floats) {
            return (float) Mth.class.getMethod(name, float.class).invoke(null, (float) radians);
        }
    }

    private static void trig() throws ReflectiveOperationException {
        long sum = 0L;
        for (int i = 0; i < 65536; i++) {
            // each table entry through its own lookup, its index exact in either signature
            sum = sum * 31L + Float.floatToRawIntBits((float) sin("sin", (i + 0.5) / 10430.378350470453));
        }
        line("sintable", new double[0], (double) (sum >>> 11));
        for (int i = 0; i < 80; i++) {
            double value = (RANDOM.nextDouble() - 0.5) * Math.pow(10.0, RANDOM.nextInt(6) - 1);
            double other = i % 4 == 0 ? edge() : (RANDOM.nextDouble() - 0.5) * 8.0;
            line("sin", new double[] {value}, sin("sin", value));
            line("cos", new double[] {value}, sin("cos", value));
            line("atan2", new double[] {value, other}, Mth.atan2(value, other));
        }
    }

    private static void boxes() {
        for (int i = 0; i < 30; i++) {
            double[] a = six();
            double[] b = six();
            double[] sorted = of(box(a));
            double x = mixed();
            double y = mixed();
            double z = mixed();
            line("ctor", a, sorted);
            line("inflate", join(sorted, x, y, z), of(box(a).inflate(x, y, z)));
            line("deflate", join(sorted, x, y, z), () -> of(box(a).deflate(x, y, z)));
            line("contract", join(sorted, x, y, z), of(box(a).contract(x, y, z)));
            line("expand", join(sorted, x, y, z), of(box(a).expandTowards(x, y, z)));
            line("move", join(sorted, x, y, z), of(box(a).move(x, y, z)));
            line("intersect", join(sorted, of(box(b))), of(box(a).intersect(box(b))));
            line("minmax", join(sorted, of(box(b))), of(box(a).minmax(box(b))));
            line("intersects", join(sorted, of(box(b))), bool(box(a).intersects(box(b))));
            line("contains", join(sorted, x, y, z), () -> new double[] {bool(box(a).contains(x, y, z))});
            line("distance", join(sorted, x, y, z), () -> new double[] {box(a).distanceToSqr(new Vec3(x, y, z))});
            line("chain", join(sorted, x, y, z), () -> of(box(a).move(x, y, z).deflate(z, x, y).move(-y, -z, -x)));
            double[] from = {mixed(), mixed(), mixed()};
            double[] to = i % 3 == 0 ? new double[] {from[0], mixed(), mixed()} : new double[] {mixed(), mixed(), mixed()};
            line("clip", join(sorted, join(from, to)), box(a).clip(new Vec3(from[0], from[1], from[2]), new Vec3(to[0], to[1], to[2])).map(Oracle::of).orElse(new double[0]));
        }
        // rays straight at the faces, edges and corners of a box, from inside and out
        double[] unit = {-1.0, -1.0, -1.0, 2.0, 2.0, 2.0};
        double[] rayEnds = {-3.0, -1.0, -1.0 - E, 0.5, 2.0, 2.0 + E, 4.0};
        for (double ax : rayEnds) {
            for (double ay : rayEnds) {
                for (double bx : new double[] {-3.0, 0.5, 4.0}) {
                    double[] from = {ax, ay, 0.5};
                    double[] to = {bx, 0.5, 2.0};
                    line("clip", join(unit, join(from, to)), box(unit).clip(new Vec3(ax, ay, 0.5), new Vec3(bx, 0.5, 2.0)).map(Oracle::of).orElse(new double[0]));
                }
            }
        }
        // the faces themselves: on, just inside, just outside each
        for (int axis = 0; axis < 3; axis++) {
            for (double value : new double[] {-1.0, Math.nextDown(-1.0), Math.nextUp(-1.0), 2.0, Math.nextDown(2.0), Math.nextUp(2.0)}) {
                double[] point = {0.5, 0.5, 0.5};
                point[axis] = value;
                line("contains", join(unit, point), bool(box(unit).contains(point[0], point[1], point[2])));
                double[] thin = {value, value, value, 5.0, 5.0, 5.0};
                line("intersects", join(unit, thin), bool(box(unit).intersects(box(thin))));
            }
        }
    }

    private static void vectors() {
        for (int i = 0; i < 30; i++) {
            double[] a = {mixed(), mixed(), mixed()};
            double[] b = {mixed(), mixed(), mixed()};
            double t = mixed();
            Vec3 u = new Vec3(a[0], a[1], a[2]);
            Vec3 v = new Vec3(b[0], b[1], b[2]);
            line("vadd", join(a, b), of(u.add(v)));
            line("vsub", join(a, b), of(u.subtract(v)));
            line("vmul", join(a, b), of(u.multiply(v)));
            line("vscale", join(a, t), of(u.scale(t)));
            line("vlerp", join(join(a, b), t), () -> of(u.lerp(v, t)));
            line("vcross", join(a, b), of(u.cross(v)));
            line("vdot", join(a, b), u.dot(v));
            line("vlength", a, u.length());
            line("vhlength", a, () -> new double[] {u.horizontalDistance()});
            line("vnormalize", a, of(u.normalize()));
            line("vproject", join(a, b), () -> of(u.projectedOn(v)));
            line("vreverse", a, of(u.reverse()));
        }
    }

    private static void mth() {
        for (int i = 0; i < 60; i++) {
            final double value = i % 3 == 0 ? edge() : (RANDOM.nextDouble() - 0.5) * Math.pow(10.0, RANDOM.nextInt(14) - 4);
            float single = (float) value;
            final double low = mixed();
            double high = mixed();
            line("floor", new double[] {value}, Mth.floor(value));
            line("lfloor", new double[] {value}, () -> new double[] {Mth.lfloor(value)});
            line("ceil", new double[] {value}, Mth.ceil(value));
            line("floorf", new double[] {single}, Mth.floor(single));
            line("ceilf", new double[] {single}, Mth.ceil(single));
            line("frac", new double[] {value}, Mth.frac(value));
            line("clamp", new double[] {value, low, high}, Mth.clamp(value, low, high));
            line("clampf", new double[] {single, (float) low, (float) high}, Mth.clamp(single, (float) low, (float) high));
            line("absmax", new double[] {value, low}, () -> new double[] {Mth.absMax(value, low)});
            line("wrapdegrees", new double[] {value}, Mth.wrapDegrees(value));
            line("wrapdegreesf", new double[] {single}, Mth.wrapDegrees(single));
            line("lerp", new double[] {low, high, value}, Mth.lerp(value, low, high));
        }
        // past the int range the casts saturate before the comparison
        for (double value : new double[] {2.147483647E9, 2.147483648E9, -2.147483648E9, -2.1474836485E9, 1.0E19, -1.0E19, 9.223372036854775E18}) {
            line("floor", new double[] {value}, Mth.floor(value));
            line("lfloor", new double[] {value}, () -> new double[] {Mth.lfloor(value)});
            line("ceil", new double[] {value}, Mth.ceil(value));
        }
        for (double value : new double[] {2.147483647E9, 2.147483648E9, -2.147483648E9, -2.1474836485E9, 3.0E9, -3.0E9, 1.0E19, -1.0E19, 9.223372036854775E18, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY}) {
            float single = (float) value;
            line("frac", new double[] {value}, Mth.frac(value));
            line("floorf", new double[] {single}, Mth.floor(single));
            line("ceilf", new double[] {single}, Mth.ceil(single));
        }
        // Math.round and the (int) of it, which the engine's Rust round must not stand in for
        for (double value : new double[] {0.49999999999999994, 0.5, -0.5, -1.5, -2.5, 1.5, 2.5, 4503599627370497.0, -4503599627370497.0, 3.0E9, 1.0E30, -1.0E30, Double.NaN}) {
            line("round", new double[] {value}, new double[] {Math.round(value), (int) Math.round(value)});
        }
    }

    /** Shapes.collide over partial and full shapes, around the 1.0E-7 of a tiny move. */
    private static void collide() {
        double[][] shapes = {
            {0, 0, 0, 1, 1, 1},
            {0, 0, 0, 1, 0.5, 1},
            {0.25, 0, 0.25, 0.75, 1, 0.75},
            {0, 0.75, 0, 1, 1, 1},
        };
        double[] sides = {0.0, E, -E, Math.nextDown(E), 1.0E-8, 2.0E-7};
        double[] moves = {0.5, -0.5, 1.0E-8, E, -E, Math.nextDown(E), 2.0E-7};
        for (int s = 0; s < shapes.length; s++) {
            double[] b = shapes[s];
            VoxelShape shape = Shapes.box(b[0], b[1], b[2], b[3], b[4], b[5]);
            for (Direction.Axis axis : Direction.Axis.values()) {
                int along = axis.ordinal();
                for (double side : sides) {
                    for (double move : moves) {
                        // a 0.5 wide box resting on the shape's far face along the axis, or at its near face
                        double[] box = {0.25, 0.25, 0.25, 0.75, 0.75, 0.75};
                        box[along] = b[along + 3] + side;
                        box[along + 3] = box[along] + 0.5;
                        AABB moving = box(box);
                        double moved = Shapes.collide(axis, moving, java.util.List.of(shape), move);
                        line("collide", join(join(b, along), join(of(moving), move)), moved);
                        line("join", join(b, of(moving)), bool(Shapes.joinIsNotEmpty(shape, Shapes.create(moving), BooleanOp.AND)));
                        box[along + 3] = b[along] - side;
                        box[along] = box[along + 3] - 0.5;
                        moving = box(box);
                        moved = Shapes.collide(axis, moving, java.util.List.of(shape), move);
                        line("collide", join(join(b, along), join(of(moving), move)), moved);
                    }
                }
            }
        }
    }

    private Oracle() {}
}

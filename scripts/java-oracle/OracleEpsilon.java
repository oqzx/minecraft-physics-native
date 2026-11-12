import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.core.BlockPos;
import net.minecraft.util.Mth;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/**
 * The 1.0E-5F tolerances at their call sites, through a jar's own classes, for native/fixtures/java-oracle-epsilon-RELEASE.txt:
 * Mth.equal below, on and above the tolerance, and BlockGetter's walk of a move on each side of the square its call
 * compares with: Mth.square(1.0E-5F) from 1.21.9, Mth.square(0.99999F) up to 1.21.8 (both compare as doubles).
 *
 * `mthequal a b = 0|1`; `mthequalf a b = 0|1`; `traverse from3 to3 box6 = (x y z step)...`.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleEpsilon.java
 * java -cp "out;JARS" OracleEpsilon > native/fixtures/java-oracle-epsilon-RELEASE.txt
 */
public final class OracleEpsilon {
    private static final double T = (double) 1.0E-5F;

    private static double nudge(double value, int ulps) {
        for (int i = 0; i < Math.abs(ulps); i++) {
            value = ulps > 0 ? Math.nextUp(value) : Math.nextDown(value);
        }
        return value;
    }

    private static void equals() {
        double[] bases = {0.0, 1.0, -1.0, 0.25, 16.0, -1024.5, 29999000.0};
        for (double base : bases) {
            for (int ulps = -3; ulps <= 3; ulps++) {
                for (double sign : new double[] {1.0, -1.0}) {
                    double other = base + sign * nudge(T, ulps);
                    System.out.println("mthequal " + base + " " + other + " = " + (Mth.equal(base, other) ? "1" : "0"));
                }
            }
        }
        System.out.println("mthequal -0.0 0.0 = " + (Mth.equal(-0.0, 0.0) ? "1" : "0"));
        System.out.println("mthequal NaN 0.0 = " + (Mth.equal(Double.NaN, 0.0) ? "1" : "0"));
        System.out.println("mthequal Infinity Infinity = " + (Mth.equal(Double.POSITIVE_INFINITY, Double.POSITIVE_INFINITY) ? "1" : "0"));
        float[] floats = {0.0F, 1.0F, -1.0F, 0.25F, 16.0F};
        for (float base : floats) {
            float other = base + 1.0E-5F;
            for (float value : new float[] {other, Math.nextUp(other), Math.nextDown(other), Math.nextUp(Math.nextUp(other)), Math.nextDown(Math.nextDown(other))}) {
                System.out.println("mthequalf " + (double) base + " " + (double) value + " = " + (Mth.equal(base, value) ? "1" : "0"));
            }
        }
    }

    /** The walk of a jar: a visitor from 1.21.5, an Iterable before; null where the release has neither. */
    private static List<int[]> walk(Vec3 from, Vec3 to, AABB box) throws ReflectiveOperationException {
        List<int[]> seen = new ArrayList<>();
        for (Method method : BlockGetter.class.getMethods()) {
            if (method.getName().equals("forEachBlockIntersectedBetween")) {
                Class<?> visitor = method.getParameterTypes()[3];
                InvocationHandler record = (proxy, called, args) -> {
                    if (called.getDeclaringClass() == Object.class) {
                        return called.getName().equals("hashCode") ? Integer.valueOf(1) : Boolean.FALSE;
                    }
                    BlockPos pos = (BlockPos) args[0];
                    seen.add(new int[] {pos.getX(), pos.getY(), pos.getZ(), (int) args[1]});
                    return Boolean.TRUE;
                };
                method.invoke(null, from, to, box, Proxy.newProxyInstance(visitor.getClassLoader(), new Class<?>[] {visitor}, record));
                return seen;
            }
            if (method.getName().equals("boxTraverseBlocks")) {
                for (Object pos : (Iterable<?>) method.invoke(null, from, to, box)) {
                    BlockPos at = (BlockPos) pos;
                    seen.add(new int[] {at.getX(), at.getY(), at.getZ(), 0});
                }
                return seen;
            }
        }
        return null;
    }

    private static void traversals() throws ReflectiveOperationException {
        float[] thresholds = {1.0E-5F, 0.99999F};
        double[][] starts = {{0.5, 64.5, 0.5}, {-0.3, 64.2, 7.9}, {1000.25, 5.5, -3.75}, {0.999, 64.999, 0.001}};
        double[][] directions = {{1.0, 0.0, 0.0}, {0.0, -1.0, 0.0}, {0.0, 0.0, -1.0}, {1.0, 1.0, 0.0}, {-1.0, 1.0, 1.0}};
        for (float threshold : thresholds) {
            double length = Math.sqrt((double) Mth.square(threshold));
            for (int ulps = -3; ulps <= 3; ulps++) {
                double size = nudge(length, ulps);
                for (double[] start : starts) {
                    for (double[] direction : directions) {
                        double norm = Math.sqrt(direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]);
                        Vec3 from = new Vec3(start[0], start[1], start[2]);
                        Vec3 to = new Vec3(start[0] + direction[0] / norm * size, start[1] + direction[1] / norm * size, start[2] + direction[2] / norm * size);
                        AABB box = new AABB(to.x - 0.3, to.y, to.z - 0.3, to.x + 0.3, to.y + 1.8, to.z + 0.3);
                        List<int[]> seen = walk(from, to, box);
                        if (seen == null) {
                            return;
                        }
                        StringBuilder line = new StringBuilder("traverse ");
                        line.append(from.x).append(' ').append(from.y).append(' ').append(from.z).append(' ').append(to.x).append(' ').append(to.y).append(' ').append(to.z);
                        line.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ).append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ).append(" =");
                        for (int[] cell : seen) {
                            line.append(' ').append(cell[0]).append(' ').append(cell[1]).append(' ').append(cell[2]).append(' ').append(cell[3]);
                        }
                        System.out.println(line);
                    }
                }
            }
        }
    }

    /**
     * Travels from the origin whose squared length is the threshold to the last bit, found by a seeded search: ulps of the
     * root do not land on it, and only such a travel tells `<` from `<=`. Their walk is the release's long one.
     */
    private static void exactly() throws ReflectiveOperationException {
        java.util.Random random = new java.util.Random(20261111L);
        for (float threshold : new float[] {1.0E-5F, 0.99999F}) {
            double square = (double) Mth.square(threshold);
            int found = 0;
            for (int tries = 0; tries < 40000000 && found < 6; tries++) {
                double x = random.nextDouble() * Math.sqrt(square);
                double rest = square - x * x;
                if (rest <= 0.0) {
                    continue;
                }
                double y = Math.sqrt(rest);
                for (int ulps = -2; ulps <= 2; ulps++) {
                    double candidate = nudge(y, ulps);
                    Vec3 to = new Vec3(x, candidate, 0.0);
                    if (to.lengthSqr() == square) {
                        AABB box = new AABB(to.x - 0.3, to.y, to.z - 0.3, to.x + 0.3, to.y + 1.8, to.z + 0.3);
                        List<int[]> seen = walk(Vec3.ZERO, to, box);
                        if (seen == null) {
                            return;
                        }
                        StringBuilder line = new StringBuilder("traverse 0.0 0.0 0.0 ");
                        line.append(to.x).append(' ').append(to.y).append(' ').append(to.z);
                        line.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ).append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ).append(" =");
                        for (int[] cell : seen) {
                            line.append(' ').append(cell[0]).append(' ').append(cell[1]).append(' ').append(cell[2]).append(' ').append(cell[3]);
                        }
                        System.out.println(line);
                        found++;
                        break;
                    }
                }
            }
            if (found == 0) {
                throw new IllegalStateException("no travel of length " + threshold);
            }
        }
    }

    /**
     * Diagonal travels whose leading corner crosses the grid at its corners, found by a seeded search in the engine: the
     * walk's x and y steps tie, and the travel recomputed as toCorner - fromCorner rounds them apart. From 1.21.9 the
     * walk takes its signs and steps from the travel itself (BlockGetter.addCollisionsAlongTravel(deltaMove)).
     */
    private static void cornerTies() throws ReflectiveOperationException {
        double[][] travels = {
            {9.409375, 35.109375, 3.409375, 7.409375, 33.109375, 1.409375},
            {9.659375, 63.359375, 2.659375, 7.659375, 61.359375, 0.659375},
            {8.64375, 152.34375, 2.64375, 7.64375, 151.34375, 1.64375},
            {8.659375, 50.359375, 3.659375, 6.659375, 48.359375, 1.659375},
            {9.190625, 112.890625, 2.190625, 7.690625, 111.390625, 0.690625},
            {8.003125, 191.703125, 3.003125, 6.503125, 190.203125, 1.503125},
            {8.425, 10.125, 2.425, 7.675, 9.375, 1.675},
            {8.159375, 167.859375, 2.159375, 6.659375, 166.359375, 0.659375},
            {9.4875, 187.1875, 2.4875, 7.4875, 185.1875, 0.4875},
            {9.065625, 58.765625, 3.065625, 7.565625, 57.265625, 1.565625},
            {8.971875, 93.671875, 2.971875, 7.471875, 92.171875, 1.471875},
            {8.58125, 110.28125, 2.58125, 7.58125, 109.28125, 1.58125},
        };
        for (double[] travel : travels) {
            Vec3 from = new Vec3(travel[0], travel[1], travel[2]);
            Vec3 to = new Vec3(travel[3], travel[4], travel[5]);
            AABB box = new AABB(to.x - 0.3, to.y, to.z - 0.3, to.x + 0.3, to.y + 1.8, to.z + 0.3);
            List<int[]> seen = walk(from, to, box);
            if (seen == null) {
                return;
            }
            StringBuilder line = new StringBuilder("traverse ");
            line.append(from.x).append(' ').append(from.y).append(' ').append(from.z).append(' ').append(to.x).append(' ').append(to.y).append(' ').append(to.z);
            line.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ).append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ).append(" =");
            for (int[] cell : seen) {
                line.append(' ').append(cell[0]).append(' ').append(cell[1]).append(' ').append(cell[2]).append(' ').append(cell[3]);
            }
            System.out.println(line);
        }
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        equals();
        traversals();
        exactly();
        cornerTies();
    }

    private OracleEpsilon() {}
}

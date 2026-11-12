import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.Random;

/**
 * The six-face ray clip of the jars before 1.13, for native/fixtures/java-oracle-clip-legacy.txt: `legacyclip RELEASE box6
 * from3 to3 = hit3 direction`, nothing after the `=` for a miss, the Direction's ordinal (0 down, 1 up, 2 north, 3 south,
 * 4 west, 5 east). No Mojang names exist for these jars at compile time, so the jar is read by reflection on its yarn
 * names: Box.method_585 (1.12.2 Box.java:294) and the fields of BlockHitResult.
 *
 * javac -d out scripts/java-oracle/OracleLegacyClip.java
 * java -cp "out;JARS" OracleLegacyClip RELEASE >> native/fixtures/java-oracle-clip-legacy.txt
 *
 * JARS is the release's yarn-named jar with guava.
 */
public final class OracleLegacyClip {
    private static final double E = 1.0E-7;

    public static void main(String[] args) throws ReflectiveOperationException {
        Random random = new Random(20261110L);
        Class<?> vec = Class.forName("net.minecraft.util.math.Vec3d");
        Class<?> boxType = Class.forName("net.minecraft.util.math.Box");
        Constructor<?> newVec = vec.getConstructor(double.class, double.class, double.class);
        Constructor<?> newBox = boxType.getConstructor(double.class, double.class, double.class, double.class, double.class, double.class);
        Method clip = boxType.getMethod("method_585", vec, vec);
        double[][] boxes = {
            {0, 0, 0, 1, 1, 1},
            {-1, -1, -1, 2, 2, 2},
            {0, 0, 0, 1, 0.5, 1},
            {0.25, 0, 0.25, 0.75, 1, 0.75},
        };
        double[] ends = {-2.0, -1.0, -E, 0.0, 0.25, 0.5, 0.75, 1.0, 1.0 + E, 2.0, 3.0, Math.nextUp(1.0), Math.nextDown(0.0)};
        for (double[] b : boxes) {
            Object box = newBox.newInstance(b[0], b[1], b[2], b[3], b[4], b[5]);
            for (int i = 0; i < 400; i++) {
                double[] from = {pick(random, ends), pick(random, ends), pick(random, ends)};
                double[] to = {pick(random, ends), pick(random, ends), pick(random, ends)};
                if (i % 5 == 0) {
                    to[i % 3] = from[i % 3];
                }
                Object hit = clip.invoke(box, newVec.newInstance(from[0], from[1], from[2]), newVec.newInstance(to[0], to[1], to[2]));
                StringBuilder line = new StringBuilder("legacyclip ").append(args[0]);
                for (double value : b) {
                    line.append(' ').append(value);
                }
                for (double value : from) {
                    line.append(' ').append(value);
                }
                for (double value : to) {
                    line.append(' ').append(value);
                }
                line.append(" =");
                if (hit != null) {
                    Object pos = hit.getClass().getField("pos").get(hit);
                    Object direction = hit.getClass().getField("direction").get(hit);
                    for (String axis : new String[] {"x", "y", "z"}) {
                        line.append(' ').append(vec.getField(axis).getDouble(pos));
                    }
                    line.append(' ').append(((Enum<?>) direction).ordinal());
                }
                System.out.println(line);
            }
        }
    }

    private static double pick(Random random, double[] values) {
        return values[random.nextInt(values.length)];
    }

    private OracleLegacyClip() {}
}

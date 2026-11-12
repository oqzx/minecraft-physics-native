import net.minecraft.util.Mth;

/**
 * The numeric edges Oracle.java's seeded values do not reach, through the real Mth of a vanilla jar, in its line format:
 * signed zeros, the smallest subnormals, the values around the cuts, the legal world limit and the float and int limits on
 * both sides, and the look's wrap and the sine table's index on both sides of each boundary.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleEdges.java
 * java -cp "out;JARS" OracleEdges > native/fixtures/java-oracle-edges-RELEASE.txt
 */
public final class OracleEdges {
    private static final double LIMIT = 2.9999984E7;
    private static final double[] VALUES = {
        0.0, -0.0, Double.MIN_VALUE, -Double.MIN_VALUE, Double.MIN_NORMAL, -Double.MIN_NORMAL, Math.nextDown(Double.MIN_NORMAL),
        1.0E-7, Math.nextUp(1.0E-7), Math.nextDown(1.0E-7), -1.0E-7, 0.003, Math.nextUp(0.003), Math.nextDown(0.003), -0.003,
        0.005, Math.nextDown(0.005), 1.0, Math.nextDown(1.0), Math.nextUp(1.0), -1.0, Math.nextDown(-1.0), Math.nextUp(-1.0),
        -0.5, -1.0E-300, 1.0E-300, LIMIT, Math.nextUp(LIMIT), Math.nextDown(LIMIT), -LIMIT, Math.nextUp(-LIMIT), Math.nextDown(-LIMIT),
        LIMIT + 0.5, -LIMIT - 0.5, 3.0E7, -3.0E7, 16.0, Math.nextDown(16.0), -16.0, Math.nextDown(-16.0), Math.nextUp(-16.0),
        2147483647.0, -2147483648.0, 2147483648.0, -2147483649.0, 9.007199254740992E15, -9.007199254740992E15,
    };
    private static final double[] ANGLES = {
        180.0, Math.nextUp(180.0), Math.nextDown(180.0), -180.0, Math.nextUp(-180.0), Math.nextDown(-180.0), 179.99, 180.01, -179.99, -180.01,
        360.0, -360.0, 540.0, -540.0, 0.0, -0.0, 0.01, -0.01, 90.0, -90.0, 89.99, -89.99, 720.5, -720.5, 1.0E7, -1.0E7,
    };

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

    public static void main(String[] args) {
        for (double value : VALUES) {
            line("floor", new double[] {value}, Mth.floor(value));
            line("lfloor", new double[] {value}, Mth.lfloor(value));
            line("ceil", new double[] {value}, Mth.ceil(value));
            line("frac", new double[] {value}, Mth.frac(value));
            float single = (float) value;
            line("floorf", new double[] {single}, Mth.floor(single));
        }
        for (double angle : ANGLES) {
            line("wrapdegrees", new double[] {angle}, Mth.wrapDegrees(angle));
            float single = (float) angle;
            line("wrapdegreesf", new double[] {single}, Mth.wrapDegrees(single));
            double radians = (double) (single * 0.017453292F);
            line("sin", new double[] {radians}, Mth.sin(radians));
            line("cos", new double[] {radians}, Mth.cos(radians));
        }
    }
}

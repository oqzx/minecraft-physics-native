import net.minecraft.util.Mth;

/**
 * Mth.sin and Mth.cos, and Math.sin and Math.cos, at what the other oracles leave out: very large arguments, the ends of the
 * casts, signed zero and what is not finite, for native/fixtures/java-oracle-trig-large.txt:
 * `RELEASE sin|cos|jsin|jcos x = bits` (the raw long bits of the double, a float's widened).
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleTrigLarge.java
 * java -cp "out;JARS" OracleTrigLarge RELEASE >> native/fixtures/java-oracle-trig-large.txt
 */
public final class OracleTrigLarge {
    private static final double[] VALUES = {
        0.0, -0.0, Math.PI, -Math.PI, 1.0E6, -1.0E6, 2.0E8, 2.147483648E9, -2.147483648E9, 2.0E9, 1.0E10, 2.1E14, 9.2E14, 8.846E14, 8.846E14 + 1.0, 1.0E15, 1.0E18, -1.0E18,
        9.223372036854775E18, 1.0E19, 1.0E22, -1.0E22, 1.0E100, 1.7976931348623157E308, -1.7976931348623157E308, Double.MIN_VALUE, 1.0E-300,
        Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY, Double.NaN,
    };

    private static double table(boolean sine, double x) throws ReflectiveOperationException {
        for (String kind : new String[] {sine ? "sin" : "cos"}) {
            try {
                return ((Number) Mth.class.getMethod(kind, double.class).invoke(null, x)).doubleValue();
            } catch (NoSuchMethodException older) {
                return ((Number) Mth.class.getMethod(kind, float.class).invoke(null, (float) x)).doubleValue();
            }
        }
        throw new IllegalStateException();
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        for (double x : VALUES) {
            String at = Long.toString(Double.doubleToRawLongBits(x));
            System.out.println(args[0] + " sin " + at + " = " + Double.doubleToRawLongBits(table(true, x)));
            System.out.println(args[0] + " cos " + at + " = " + Double.doubleToRawLongBits(table(false, x)));
            System.out.println(args[0] + " jsin " + at + " = " + Double.doubleToRawLongBits(Math.sin(x)));
            System.out.println(args[0] + " jcos " + at + " = " + Double.doubleToRawLongBits(Math.cos(x)));
        }
    }
}

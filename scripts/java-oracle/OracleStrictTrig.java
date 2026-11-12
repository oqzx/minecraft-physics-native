import java.math.BigDecimal;
import java.math.MathContext;
import java.math.RoundingMode;
import java.util.LinkedHashSet;
import java.util.Random;
import java.util.Set;

/**
 * StrictMath.sin and StrictMath.cos (fdlibm 5.3, what Java 8's Math.sin and Math.cos are past pi/4) and Math.sin and
 * Math.cos (HotSpot's Intel LIBM stubs on x86_64) of arguments that take every way through the argument reduction:
 * pi/4 to 3pi/4 either side of pi/2, the medium reduction's second and third iterations (the doubles nearest to n * pi/2),
 * k_rem_pio2's every q0 (powers of two from 2^20 to 2^1023), the worst case of Muller's Elementary Functions and what is
 * not finite. For native/fixtures/java-oracle-strict-trig-bits.txt, one line per argument:
 * `x StrictMath.sin StrictMath.cos Math.sin Math.cos`, each the raw bits in hex. Java 21.0.11 and 25.0.3 on x86_64 give
 * the same file.
 *
 * javac -d out scripts/java-oracle/OracleStrictTrig.java
 * java -cp out OracleStrictTrig > native/fixtures/java-oracle-strict-trig-bits.txt
 */
public final class OracleStrictTrig {
    static final MathContext MC = new MathContext(120);

    static BigDecimal atanInv(int n) {
        BigDecimal x = BigDecimal.ONE.divide(BigDecimal.valueOf(n), MC), x2 = x.multiply(x, MC), term = x, sum = x;
        for (int k = 1; k < 400; k++) {
            term = term.multiply(x2, MC);
            BigDecimal t = term.divide(BigDecimal.valueOf(2L * k + 1), MC);
            sum = (k % 2 == 1) ? sum.subtract(t, MC) : sum.add(t, MC);
        }
        return sum;
    }

    public static void main(String[] args) {
        BigDecimal pi = atanInv(5).multiply(BigDecimal.valueOf(16)).subtract(atanInv(239).multiply(BigDecimal.valueOf(4)), MC);
        BigDecimal halfPi = pi.divide(BigDecimal.valueOf(2), MC);
        Set<Double> xs = new LinkedHashSet<>();
        double[] fixed = {0.5, 0.7853981633974483, 0.78539816339744839, 0.8, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 10.0, 100.0, 1000.0, 1.0E5, 1.0E6, 1.0E7, 1.0E9, 1.0E12, 1.0E15, 1.0E18, 1.0E22, 1.0E50, 1.0E100, 1.0E200, 1.0E300,
            1.7976931348623157E308, Math.PI, Math.PI / 2, 3 * Math.PI / 4, 1.0E-10, 1.0E-9, 3.0E-9, 2.0E-8, 1.0E-300, Double.MIN_VALUE, 0.0, 0.1, 0.25, 0.3, 0.6, 0.65};
        for (double x : fixed) { xs.add(x); xs.add(-x); }
        for (int e = 20; e <= 1023; e++) {
            xs.add(Math.scalb(1.0, e));
            xs.add(-Math.scalb(1.625, e));
        }
        // the nearest doubles to n * pi/2, and their neighbours, below 2^19 * pi/2 the closest few in units of their last place
        java.util.List<double[]> close = new java.util.ArrayList<>();
        for (int n = 1; n <= (1 << 19); n++) {
            BigDecimal exact = halfPi.multiply(BigDecimal.valueOf(n), MC);
            double x = exact.doubleValue();
            double gap = exact.subtract(new BigDecimal(x), MC).abs().doubleValue() / Math.ulp(x);
            close.add(new double[] {gap, x});
            if (n <= 16 || n % 65537 == 0) { xs.add(x); xs.add(-x); xs.add(Math.nextUp(x)); xs.add(-Math.nextDown(x)); }
        }
        close.sort((a, b) -> Double.compare(a[0], b[0]));
        for (int i = 0; i < 24; i++) { xs.add(close.get(i)[1]); xs.add(-close.get(i)[1]); }
        // nearest doubles to n * pi/2 for large n: 2^k * pi/2 and odd multiples
        for (int k = 19; k <= 1000; k += 7) {
            BigDecimal exact = halfPi.multiply(new BigDecimal(java.math.BigInteger.ONE.shiftLeft(k)), MC);
            double x = exact.doubleValue();
            if (Double.isFinite(x)) { xs.add(x); xs.add(-x); }
        }
        // the worst case for argument reduction (Muller, Elementary Functions): 6381956970095103 * 2^797
        xs.add(Math.scalb(6381956970095103.0, 797)); xs.add(-Math.scalb(6381956970095103.0, 797));
        xs.add(Math.scalb(5261692873635770.0, 499));
        xs.add(Double.NaN); xs.add(Double.longBitsToDouble(0x7ff0_0000_0000_1234L)); xs.add(Double.longBitsToDouble(0xfff8_0000_0000_0042L));
        xs.add(Double.POSITIVE_INFINITY); xs.add(Double.NEGATIVE_INFINITY);
        Random random = new Random(20261111L);
        for (int i = 0; i < 300; i++) {
            int e = 20 + random.nextInt(1004);
            double x = Math.scalb(1.0 + random.nextDouble(), e);
            if (Double.isFinite(x)) xs.add(random.nextBoolean() ? x : -x);
        }
        for (int i = 0; i < 100; i++) {
            double x = (random.nextDouble() * 2 - 1) * 40.0;
            xs.add(x);
        }
        for (double x : xs) {
            System.out.printf("%016x %016x %016x %016x %016x%n", Double.doubleToRawLongBits(x), Double.doubleToRawLongBits(StrictMath.sin(x)), Double.doubleToRawLongBits(StrictMath.cos(x)),
                Double.doubleToRawLongBits(Math.sin(x)), Double.doubleToRawLongBits(Math.cos(x)));
        }
    }
}

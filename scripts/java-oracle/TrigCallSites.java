import java.io.BufferedInputStream;
import java.io.BufferedOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.math.BigDecimal;
import java.math.MathContext;
import java.math.RoundingMode;
import java.util.ArrayList;
import java.util.List;

/**
 * B315: java.lang.Math at the direct call sites that reach simulated state in releases whose version json declares Java 8,
 * over the inputs physics feeds them, written as records of raw 64-bit words so that two JVMs' files can be compared value
 * by value. Inputs come from a fixed SplitMix64 stream, the same on every JVM; nothing here takes a jar.
 *
 * javac --release 8 -d out scripts/java-oracle/TrigCallSites.java
 * java -cp out TrigCallSites write DIR      one file per site in DIR, from this JVM's Math and StrictMath
 * java -cp out TrigCallSites compare A B    per site and column: values differing, by how many ulps, and against the
 *                                           correctly rounded value where the argument is within pi/4 (run it on 21+,
 *                                           whose BigDecimal.doubleValue rounds correctly)
 * java -cp out TrigCallSites x87 DIR        within pi/4, DIR's Math against the true value rounded once, and rounded to a
 *                                           64-bit significand and then to a double (what trig::java8 computes there)
 * java -cp out TrigCallSites arc PATH OUT   Math.atan2 of TrigDigest's arc pairs, as records; arc-compare A B lists those
 *                                           that differ
 * scripts/java-oracle/wasm's trig-digest reads the same files: TRIG_JVM=8 trig-digest records SITE DIR/SITE.bin
 *
 * Sites (record words):
 *  cart-188    1.8.8 AbstractMinecart.java:446-447 and Boat.java:266-267: float r = yaw * (float)Math.PI / 180.0F, the yaws of
 *              a rider and of a boat rider, the yaws in turn with strafe 0, +0.98F and -0.98F (90.0F * 0.98F). [x, sin, cos, strict sin, strict cos]
 *  cart-194    1.9.4-1.13.2 AbstractMinecart (1.12.2 :476-477, 1.13.2 :471-472): float r = yaw * (float)(Math.PI / 180.0). Same words.
 *  seat-188    1.8.8 Boat.java:372-373 positionRider: double x = yaw * Math.PI / 180.0. Same words.
 *  costab      Mth's COS_TAB and ASIN_TAB (1.8.8 Mth.java:404-410): [i, asin, cos, strict cos]
 *  atan2-cart  Mth.atan2(dz, dx) of a cart's move, (float)(a * 180.0 / Math.PI) (1.8.8 AbstractMinecart rotation, 26.3
 *              OldMinecartBehavior.java:67-84): [dz, dx, a, degrees as float bits]
 *  atan2-dragon Mth.atan2(dx, dz) of the dragon's steering (1.8.8-1.18.1 EnderDragon): [dx, dz, a, 180 - a * 57.2957763671875]
 */
public final class TrigCallSites {
    private static long state = 0x5eed_b315L;

    private static long next() {
        long z = (state += 0x9e3779b97f4a7c15L);
        z = (z ^ (z >>> 30)) * 0xbf58476d1ce4e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d049bb133111ebL;
        return z ^ (z >>> 31);
    }

    /** Uniform in [-limit, limit), as a double of 53 random bits. */
    private static double uniform(double limit) {
        return ((next() >>> 11) * 0x1.0p-53 * 2.0 - 1.0) * limit;
    }

    /** The float yaws: every hundredth of a degree in [-720, 720], 2,000,000 random floats in [-360, 360) and 500,000 in [-3600, 3600). */
    private static List<Float> yaws() {
        List<Float> out = new ArrayList<>();
        for (int k = -72000; k <= 72000; k++) out.add((float) (k / 100.0));
        for (int i = 0; i < 2_000_000; i++) out.add((float) uniform(360.0));
        for (int i = 0; i < 500_000; i++) out.add((float) uniform(3600.0));
        return out;
    }

    private static DataOutputStream open(String dir, String name) throws IOException {
        return new DataOutputStream(new BufferedOutputStream(new FileOutputStream(dir + "/" + name + ".bin"), 1 << 20));
    }

    private static void trig(DataOutputStream out, double x) throws IOException {
        out.writeLong(Double.doubleToRawLongBits(x));
        out.writeLong(Double.doubleToRawLongBits(Math.sin(x)));
        out.writeLong(Double.doubleToRawLongBits(Math.cos(x)));
        out.writeLong(Double.doubleToRawLongBits(StrictMath.sin(x)));
        out.writeLong(Double.doubleToRawLongBits(StrictMath.cos(x)));
    }

    private static final double FRAC_BIAS = Double.longBitsToDouble(4805340802404319232L);
    private static final double[] ASIN_TAB = new double[257];
    private static final double[] COS_TAB = new double[257];

    static {
        for (int i = 0; i < 257; i++) {
            double e = Math.asin(i / 256.0);
            COS_TAB[i] = Math.cos(e);
            ASIN_TAB[i] = e;
        }
    }

    private static double fastInvSqrt(double d) {
        double e = 0.5 * d;
        long l = Double.doubleToRawLongBits(d);
        l = 6910469410427058090L - (l >> 1);
        d = Double.longBitsToDouble(l);
        return d * (1.5 - e * d * d);
    }

    /** 1.8.8 Mth.java:336-379, the same through 26.3. */
    private static double atan2(double d, double e) {
        double f = e * e + d * d;
        if (Double.isNaN(f)) return Double.NaN;
        boolean bl = d < 0.0;
        if (bl) d = -d;
        boolean bl2 = e < 0.0;
        if (bl2) e = -e;
        boolean bl3 = d > e;
        if (bl3) {
            double g = e;
            e = d;
            d = g;
        }
        double g = fastInvSqrt(f);
        e *= g;
        d *= g;
        double h = FRAC_BIAS + d;
        int i = (int) Double.doubleToRawLongBits(h);
        double j = ASIN_TAB[i];
        double k = COS_TAB[i];
        double l = h - FRAC_BIAS;
        double m = d * k - e * l;
        double n = (6.0 + m * m) * m * 0.16666666666666666;
        double o = j + n;
        if (bl3) o = (Math.PI / 2) - o;
        if (bl2) o = Math.PI - o;
        if (bl) o = -o;
        return o;
    }

    private static void write(String dir) throws IOException {
        List<Float> yaws = yaws();
        float strafe = 90.0F * 0.98F;
        try (DataOutputStream out = open(dir, "cart-188")) {
            int k = 0;
            for (float yaw : yaws) {
                float s = k % 3 == 0 ? yaw : k % 3 == 1 ? yaw + -strafe : yaw + strafe;
                k++;
                trig(out, s * (float) Math.PI / 180.0F);
            }
        }
        try (DataOutputStream out = open(dir, "cart-194")) {
            for (float yaw : yaws) trig(out, yaw * (float) (Math.PI / 180.0));
        }
        try (DataOutputStream out = open(dir, "seat-188")) {
            for (float yaw : yaws) trig(out, yaw * Math.PI / 180.0);
        }
        try (DataOutputStream out = open(dir, "costab")) {
            for (int i = 0; i < 257; i++) {
                out.writeLong(i);
                out.writeLong(Double.doubleToRawLongBits(ASIN_TAB[i]));
                out.writeLong(Double.doubleToRawLongBits(COS_TAB[i]));
                out.writeLong(Double.doubleToRawLongBits(StrictMath.cos(ASIN_TAB[i])));
            }
        }
        try (DataOutputStream out = open(dir, "atan2-cart")) {
            for (int i = 0; i < 4_000_000; i++) {
                double dx;
                double dz;
                switch (i & 3) {
                    case 0: dx = uniform(0.5); dz = uniform(0.5); break;
                    case 1: dx = uniform(8.0); dz = uniform(8.0); break;
                    case 2: dx = uniform(0.4); dz = (next() < 0 ? -dx : dx); break; // a diagonal rail, |dx| == |dz|
                    default: dx = uniform(0.4); dz = (next() < 0 ? -dx : dx) * (1.0 + uniform(1.0e-6)); break;
                }
                if (dx * dx + dz * dz <= 0.001) continue;
                double a = atan2(dz, dx);
                out.writeLong(Double.doubleToRawLongBits(dz));
                out.writeLong(Double.doubleToRawLongBits(dx));
                out.writeLong(Double.doubleToRawLongBits(a));
                out.writeLong(Float.floatToRawIntBits((float) (a * 180.0 / Math.PI)) & 0xffffffffL);
            }
        }
        try (DataOutputStream out = open(dir, "atan2-dragon")) {
            for (int i = 0; i < 1_000_000; i++) {
                double dx = uniform(150.0);
                double dz = uniform(150.0);
                double a = atan2(dx, dz);
                out.writeLong(Double.doubleToRawLongBits(dx));
                out.writeLong(Double.doubleToRawLongBits(dz));
                out.writeLong(Double.doubleToRawLongBits(a));
                out.writeLong(Double.doubleToRawLongBits(180.0 - a * 57.2957763671875));
            }
        }
        System.out.println("written " + System.getProperty("java.runtime.version"));
    }

    private static long[] read(String path, int width) throws IOException {
        long length = new java.io.File(path).length();
        long[] flat = new long[(int) (length / 8 / width * width)];
        try (DataInputStream in = new DataInputStream(new BufferedInputStream(new FileInputStream(path), 1 << 20))) {
            for (int i = 0; i < flat.length; i++) flat[i] = in.readLong();
        }
        return flat;
    }

    private static final MathContext MC = new MathContext(80, RoundingMode.HALF_EVEN);

    /** sin (odd) or cos (even) by the Taylor series at 80 digits, for |x| <= pi/4, rounded once to the nearest double. */
    private static double exact(double x, boolean sine) {
        BigDecimal v = new BigDecimal(x);
        BigDecimal v2 = v.multiply(v, MC);
        BigDecimal term = sine ? v : BigDecimal.ONE;
        BigDecimal sum = term;
        for (int n = sine ? 1 : 0; n < 80; n += 1) {
            int a = sine ? 2 * n : 2 * n + 1;
            int b = sine ? 2 * n + 1 : 2 * n + 2;
            term = term.multiply(v2, MC).divide(BigDecimal.valueOf(-(long) a * b), MC);
            sum = sum.add(term, MC);
            if (term.signum() == 0 || term.abs().compareTo(BigDecimal.ONE.movePointLeft(70)) < 0) break;
        }
        return sum.doubleValue();
    }

    private static long ulps(long a, long b, boolean isFloat) {
        if (isFloat) return Math.abs((long) (int) a - (long) (int) b);
        if ((a < 0) != (b < 0)) return Long.MAX_VALUE;
        return Math.abs(a - b);
    }

    private static void compare(String a, String b) throws IOException {
        String[] sites = {"cart-188", "cart-194", "seat-188", "costab", "atan2-cart", "atan2-dragon"};
        for (String site : sites) {
            boolean trig = site.startsWith("cart") || site.startsWith("seat");
            int width = trig ? 5 : 4;
            long[] x = read(a + "/" + site + ".bin", width);
            long[] y = read(b + "/" + site + ".bin", width);
            int rows = x.length / width;
            if (y.length != x.length) throw new IllegalStateException(site + " lengths differ");
            System.out.println("== " + site + " rows " + rows);
            int[] outputs = trig ? new int[] {1, 2} : site.equals("costab") ? new int[] {1, 2} : new int[] {2, 3};
            for (int c : outputs) {
                boolean isFloat = site.equals("atan2-cart") && c == 3;
                long differ = 0;
                long max = 0;
                long[] histogram = new long[4];
                long inputsDiffer = 0;
                int shown = 0;
                for (int r = 0; r < rows; r++) {
                    if (x[r * width] != y[r * width]) inputsDiffer++;
                    long p = x[r * width + c];
                    long q = y[r * width + c];
                    if (p == q) continue;
                    differ++;
                    long u = ulps(p, q, isFloat);
                    max = Math.max(max, u);
                    histogram[(int) Math.min(u, 3)]++;
                    if (shown++ < 3) {
                        System.out.println("   e.g. row " + r + " in " + Long.toHexString(x[r * width]) + " "
                            + Long.toHexString(p) + " vs " + Long.toHexString(q));
                    }
                }
                System.out.println(" col " + c + ": differ " + differ + " of " + rows + " (" + String.format("%.4f", 100.0 * differ / rows)
                    + "%), max ulps " + max + ", 1ulp " + histogram[1] + ", 2ulp " + histogram[2] + ", 3+ " + histogram[3]
                    + (inputsDiffer > 0 ? " INPUTS DIFFER " + inputsDiffer : ""));
            }
            if (!trig) continue;
            // the first file's Math against its own StrictMath past pi/4, and against the correctly rounded value within it
            for (int c : new int[] {1, 2}) {
                long small = 0, smallCr = 0, smallNotCr = 0, smallSecondCr = 0, large = 0, largeStrict = 0, largeSecondStrict = 0;
                for (int r = 0; r < rows; r++) {
                    double arg = Double.longBitsToDouble(x[r * width]);
                    long p = x[r * width + c];
                    long q = y[r * width + c];
                    if (Math.abs(arg) <= Math.PI / 4) {
                        small++;
                        long cr = Double.doubleToRawLongBits(exact(arg, c == 1));
                        if (p == cr) smallCr++; else smallNotCr++;
                        if (q == cr) smallSecondCr++;
                    } else {
                        large++;
                        if (p == x[r * width + c + 2]) largeStrict++;
                        if (q == y[r * width + c + 2]) largeSecondStrict++;
                    }
                }
                System.out.println(" " + (c == 1 ? "sin" : "cos") + ": |x|<=pi/4 " + small + ", first file correctly rounded " + smallCr
                    + " (not " + smallNotCr + "), second " + smallSecondCr + "; |x|>pi/4 " + large + ", first == its StrictMath " + largeStrict
                    + ", second == its StrictMath " + largeSecondStrict);
            }
        }
        long[] cos = read(a + "/costab.bin", 4);
        long[] cos2 = read(b + "/costab.bin", 4);
        StringBuilder entries = new StringBuilder();
        for (int i = 0; i < 257; i++) {
            if (cos[i * 4 + 2] != cos2[i * 4 + 2]) {
                double angle = Double.longBitsToDouble(cos[i * 4 + 1]);
                long cr = angle <= Math.PI / 4 ? Double.doubleToRawLongBits(exact(angle, false)) : 0;
                entries.append(' ').append(i).append(angle <= Math.PI / 4 ? (cos[i * 4 + 2] == cr ? "(first CR)" : cos2[i * 4 + 2] == cr ? "(second CR)" : "(neither CR)") : "");
            }
        }
        System.out.println("COS_TAB entries differing:" + entries);
    }

    /** TrigDigest's arc pairs: Math.atan2(word i, word i + 1) of a file of little-endian doubles, as [x, y, atan2]. */
    private static void arc(String path, String outPath) throws IOException {
        java.nio.ByteBuffer bytes = java.nio.ByteBuffer.wrap(java.nio.file.Files.readAllBytes(java.nio.file.Paths.get(path)))
            .order(java.nio.ByteOrder.LITTLE_ENDIAN);
        int count = bytes.remaining() / 8;
        try (DataOutputStream out = new DataOutputStream(new BufferedOutputStream(new FileOutputStream(outPath), 1 << 20))) {
            for (int i = 0; i + 1 < count; i++) {
                double x = bytes.getDouble(i * 8);
                double y = bytes.getDouble(i * 8 + 8);
                out.writeLong(Double.doubleToRawLongBits(x));
                out.writeLong(Double.doubleToRawLongBits(y));
                out.writeLong(Double.doubleToRawLongBits(Math.atan2(x, y)));
            }
        }
    }

    private static void arcCompare(String a, String b) throws IOException {
        long[] x = read(a, 3);
        long[] y = read(b, 3);
        for (int r = 0; r < x.length / 3; r++) {
            if (x[r * 3 + 2] == y[r * 3 + 2]) continue;
            double p = Double.longBitsToDouble(x[r * 3]);
            double q = Double.longBitsToDouble(x[r * 3 + 1]);
            System.out.println("atan2(" + p + ", " + q + ") " + Long.toHexString(x[r * 3 + 2]) + " vs " + Long.toHexString(y[r * 3 + 2])
                + " |y/x| " + Math.abs(p / q));
        }
    }

    private static BigDecimal pow2(int k) {
        return k >= 0 ? new BigDecimal(java.math.BigInteger.ONE.shiftLeft(k)) : BigDecimal.ONE.divide(new BigDecimal(java.math.BigInteger.ONE.shiftLeft(-k)));
    }

    /** The true value rounded to a 64-bit significand, then to a double, both half-even: an x87 result stored as a double. */
    private static double twice(double x, boolean sine) {
        BigDecimal v = new BigDecimal(x);
        BigDecimal v2 = v.multiply(v, MC);
        BigDecimal term = sine ? v : BigDecimal.ONE;
        BigDecimal sum = term;
        for (int n = sine ? 1 : 0; n < 80; n += 1) {
            int a = sine ? 2 * n : 2 * n + 1;
            int b = sine ? 2 * n + 1 : 2 * n + 2;
            term = term.multiply(v2, MC).divide(BigDecimal.valueOf(-(long) a * b), MC);
            sum = sum.add(term, MC);
            if (term.signum() == 0 || term.abs().compareTo(BigDecimal.ONE.movePointLeft(70)) < 0) break;
        }
        if (sum.signum() == 0) return sine ? x : 1.0;
        double guess = sum.doubleValue();
        int e = Math.getExponent(guess);
        BigDecimal scaled = sum.multiply(pow2(63 - e));
        if (scaled.abs().compareTo(pow2(64)) >= 0) { e++; scaled = sum.multiply(pow2(63 - e)); }
        if (scaled.abs().compareTo(pow2(63)) < 0) { e--; scaled = sum.multiply(pow2(63 - e)); }
        java.math.BigInteger m64 = scaled.setScale(0, RoundingMode.HALF_EVEN).toBigIntegerExact();
        java.math.BigInteger m53 = new BigDecimal(m64).divide(pow2(11)).setScale(0, RoundingMode.HALF_EVEN).toBigIntegerExact();
        return new BigDecimal(m53).multiply(pow2(e - 52)).doubleValue();
    }

    /** Within pi/4, the first file's Math against the correctly rounded value and against the value rounded twice. */
    private static void x87(String dir) throws IOException {
        for (String site : new String[] {"cart-188", "cart-194", "seat-188"}) {
            long[] x = read(dir + "/" + site + ".bin", 5);
            long small = 0, cr = 0, two = 0, neither = 0;
            for (int r = 0; r < x.length / 5; r++) {
                double arg = Double.longBitsToDouble(x[r * 5]);
                if (Math.abs(arg) > Math.PI / 4) continue;
                for (int c : new int[] {1, 2}) {
                    small++;
                    long v = x[r * 5 + c];
                    boolean isCr = v == Double.doubleToRawLongBits(exact(arg, c == 1));
                    boolean isTwo = v == Double.doubleToRawLongBits(twice(arg, c == 1));
                    if (isCr) cr++;
                    if (isTwo) two++;
                    if (!isCr && !isTwo) { neither++; if (neither < 5) System.out.println("  neither: " + site + " x " + Long.toHexString(x[r * 5]) + " col " + c); }
                }
            }
            System.out.println(site + ": values within pi/4 " + small + ", correctly rounded " + cr + ", rounded twice (64 then 53 bits) " + two + ", neither " + neither);
        }
        long[] t = read(dir + "/costab.bin", 4);
        long ok = 0;
        for (int i = 0; i < 257; i++) {
            double angle = Double.longBitsToDouble(t[i * 4 + 1]);
            if (angle > Math.PI / 4) continue;
            if (t[i * 4 + 2] == Double.doubleToRawLongBits(twice(angle, false))) ok++; else System.out.println("  costab " + i + " not rounded twice");
        }
        System.out.println("costab within pi/4: rounded twice " + ok);
    }

    public static void main(String[] args) throws IOException {
        switch (args[0]) {
            case "write": write(args[1]); break;
            case "compare": compare(args[1], args[2]); break;
            case "arc": arc(args[1], args[2]); break;
            case "x87": x87(args[1]); break;
            case "arc-compare": arcCompare(args[1], args[2]); break;
            default: throw new IllegalArgumentException(args[0]);
        }
    }
}

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.LongBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Paths;
import java.nio.file.StandardOpenOption;

/**
 * java.lang.Math.sin and cos over whole sets, as digests of the raw bits, on the JVM that runs it. Its twin is
 * scripts/java-oracle/wasm/src/trig_digest.rs, which prints the same lines for native/src/trig.rs on the host and in wasm32-wasip1:
 * equal output is equal bits. A digest is FNV-1a over 64-bit words, each step a bijection, so one differing result always shows.
 * Writes out the java.runtime.version on stderr. Nothing here takes a jar: only the JVM's Math is under test.
 *
 * javac --release 8 -d out scripts/java-oracle/TrigDigest.java
 * java -cp out TrigDigest floats          every finite float, by sign and exponent
 * java -cp out TrigDigest tables          the two Mth tables, as (float) bits, with the Java formulas
 * java -cp out TrigDigest atan-tables    Mth.atan2's two tables, with the Java formulas
 * java -cp out TrigDigest kick SEED COUNT PrimedTnt's kick, angles drawn as LegacyRandomSource does
 * java -cp out TrigDigest file NAME PATH  little-endian doubles from scripts/java-oracle/trig-vectors.cjs
 * java -cp out TrigDigest arc NAME PATH   Math.atan2, acos and asin over the same words
 * java -cp out TrigDigest bits PATH       one line `x sin cos` of hex bits per double of PATH
 */
public final class TrigDigest {
    private static final long BASIS = 0xcbf29ce484222325L;
    private static final long PRIME = 0x100000001b3L;

    private static long mix(long hash, long word) {
        return (hash ^ word) * PRIME;
    }

    private static void floats() {
        for (int top = 0; top < 512; top++) {
            if ((top & 255) == 255) continue;
            long sin = BASIS;
            long cos = BASIS;
            long count = 0;
            int first = top << 23;
            for (int low = 0; low < 1 << 23; low++) {
                double x = Float.intBitsToFloat(first | low);
                sin = mix(sin, Double.doubleToRawLongBits(Math.sin(x)));
                cos = mix(cos, Double.doubleToRawLongBits(Math.cos(x)));
                count++;
            }
            print("floats " + top, count, sin, cos);
        }
    }

    private static void tables() {
        long modern = BASIS;
        long legacy = BASIS;
        for (int i = 0; i < 65536; i++) {
            modern = mix(modern, Float.floatToRawIntBits((float) Math.sin(i / 10430.378350470453)) & 0xffffffffL);
            legacy = mix(legacy, Float.floatToRawIntBits((float) Math.sin(i * Math.PI * 2.0 / 65536.0)) & 0xffffffffL);
        }
        print("tables", 65536, modern, legacy);
    }

    /** Mth's table for atan2: 26.3 Mth, ASIN_TAB and COS_TAB, of 257 entries. */
    private static void atanTables() {
        long asin = BASIS;
        long cos = BASIS;
        for (int i = 0; i < 257; i++) {
            double angle = Math.asin(i / 256.0);
            asin = mix(asin, Double.doubleToRawLongBits(angle));
            cos = mix(cos, Double.doubleToRawLongBits(Math.cos(angle)));
        }
        print("atan-tables", 257, asin, cos);
    }

    /** 26.3 LegacyRandomSource.next, BitRandomSource.nextDouble and PrimedTnt's constructor. */
    private static void kick(long seed, int count) {
        long state = (seed ^ 25214903917L) & 281474976710655L;
        long x = BASIS;
        long z = BASIS;
        for (int i = 0; i < count; i++) {
            state = state * 25214903917L + 11L & 281474976710655L;
            int upper = (int) (state >> 48 - 26);
            state = state * 25214903917L + 11L & 281474976710655L;
            int lower = (int) (state >> 48 - 27);
            double unit = (double) (((long) upper << 27) + (long) lower) * 1.1102230246251565E-16;
            double rot = unit * 6.2831854820251465;
            x = mix(x, Double.doubleToRawLongBits(-Math.sin(rot) * 0.02));
            z = mix(z, Double.doubleToRawLongBits(-Math.cos(rot) * 0.02));
        }
        print("kick " + seed, count, x, z);
    }

    private static LongBuffer read(String path) throws IOException {
        try (FileChannel channel = FileChannel.open(Paths.get(path), StandardOpenOption.READ)) {
            ByteBuffer bytes = channel.map(FileChannel.MapMode.READ_ONLY, 0, channel.size()).order(ByteOrder.LITTLE_ENDIAN);
            return bytes.asLongBuffer();
        }
    }

    private static void file(String name, String path) throws IOException {
        LongBuffer in = read(path);
        long sin = BASIS;
        long cos = BASIS;
        long count = in.remaining();
        for (int i = 0; i < count; i++) {
            double x = Double.longBitsToDouble(in.get(i));
            sin = mix(sin, Double.doubleToRawLongBits(Math.sin(x)));
            cos = mix(cos, Double.doubleToRawLongBits(Math.cos(x)));
        }
        print("file " + name, count, sin, cos);
    }

    /** Math.atan2 of each pair, Math.acos and Math.asin of each number, all fdlibm's on HotSpot: trig::atan2 and the libm crate. */
    private static void arc(String name, String path) throws IOException {
        LongBuffer in = read(path);
        long atan2 = BASIS;
        long inverse = BASIS;
        long count = in.remaining() - 1;
        for (int i = 0; i < count; i++) {
            double x = Double.longBitsToDouble(in.get(i));
            double y = Double.longBitsToDouble(in.get(i + 1));
            atan2 = mix(atan2, Double.doubleToRawLongBits(Math.atan2(x, y)));
            double unit = x / (Math.abs(x) + Math.abs(y));
            inverse = mix(mix(inverse, Double.doubleToRawLongBits(Math.acos(unit))), Double.doubleToRawLongBits(Math.asin(unit)));
        }
        print("arc " + name, count, atan2, inverse);
    }

    private static void bits(String path) throws IOException {
        LongBuffer in = read(path);
        StringBuilder out = new StringBuilder();
        for (int i = 0; i < in.remaining(); i++) {
            double x = Double.longBitsToDouble(in.get(i));
            out.append(Long.toHexString(in.get(i))).append(' ').append(Long.toHexString(Double.doubleToRawLongBits(Math.sin(x))))
                .append(' ').append(Long.toHexString(Double.doubleToRawLongBits(Math.cos(x)))).append('\n');
        }
        System.out.print(out);
    }

    private static void print(String name, long count, long a, long b) {
        System.out.println(name + " " + count + " " + Long.toHexString(a) + " " + Long.toHexString(b));
    }

    public static void main(String[] args) throws IOException {
        System.err.println(System.getProperty("java.runtime.version"));
        switch (args[0]) {
            case "floats": floats(); break;
            case "tables": tables(); break;
            case "atan-tables": atanTables(); break;
            case "kick": kick(Long.parseLong(args[1]), Integer.parseInt(args[2])); break;
            case "file": file(args[1], args[2]); break;
            case "arc": arc(args[1], args[2]); break;
            case "bits": bits(args[1]); break;
            default: throw new IllegalArgumentException(args[0]);
        }
    }
}

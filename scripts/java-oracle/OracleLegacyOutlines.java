import java.lang.reflect.Array;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;

/**
 * The outline box of every block state of a jar before 1.13, alone in the world, for src/mineflayer/legacy-outlines.json: what
 * World.rayTrace(start, end) meets, canCollide(state, false) and Block.rayTrace's box (1.12.2 World.java rayTrace, Block.java:
 * 378, 414). `RELEASE ID META OFFSET minX minY minZ maxX maxY maxZ` a line, OFFSET 1 when the box moves with the position
 * (a flower's Block.OffsetType), and then the box is that of the first position. No Mojang names exist for these jars at
 * compile time, so the jar is read by reflection on its yarn names, as OracleLegacyClip.
 *
 * javac -d out scripts/java-oracle/OracleLegacyOutlines.java
 * java -cp "out;JARS" OracleLegacyOutlines RELEASE
 *
 * JARS is the release's yarn-named jar and its libraries.
 */
public final class OracleLegacyOutlines {
    public static void main(String[] args) throws ReflectiveOperationException {
        Class<?> block = Class.forName("net.minecraft.block.Block");
        Class<?> state = Class.forName("net.minecraft.block.BlockState");
        Class<?> view = Class.forName("net.minecraft.world.BlockView");
        Class<?> pos = Class.forName("net.minecraft.util.math.BlockPos");
        Class.forName("net.minecraft.Bootstrap").getMethod("initialize").invoke(null);
        Method getById = block.getMethod("getById", int.class);
        Method fromData = block.getMethod("stateFromData", int.class);
        Method canCollide = block.getMethod("canCollide", state, boolean.class);
        Method getId = block.getMethod("getIdByBlock", block);
        Method byCollision = null;
        for (Method m : block.getMethods()) {
            if (m.getName().equals("getCollisionBox") && m.getParameterCount() == 3 && m.getParameterTypes()[1] == view) {
                byCollision = m;
            }
        }
        Method setBounds = byCollision != null ? null : block.getMethod("setBoundingBox", view, pos);
        Object air = fromData.invoke(getById.invoke(null, 0), 0);
        Object first = pos.getConstructor(int.class, int.class, int.class).newInstance(0, 64, 0);
        Object second = pos.getConstructor(int.class, int.class, int.class).newInstance(123, 64, -77);
        for (int id = 1; id < 256; id++) {
            Object b = getById.invoke(null, id);
            if (((Integer) getId.invoke(null, b)) != id) {
                continue;
            }
            for (int meta = 0; meta < 16; meta++) {
                Object s;
                try {
                    s = fromData.invoke(b, meta);
                } catch (ReflectiveOperationException e) {
                    continue;
                }
                if (!(Boolean) canCollide.invoke(b, s, false)) {
                    continue;
                }
                double[] one = outline(b, s, air, first, view, byCollision, setBounds);
                double[] two = outline(b, s, air, second, view, byCollision, setBounds);
                if (one == null) {
                    continue;
                }
                boolean offset = two == null || one[0] != two[0] || one[2] != two[2];
                StringBuilder line = new StringBuilder(args[0]).append(' ').append(id).append(' ').append(meta).append(' ').append(offset ? 1 : 0);
                for (double value : one) {
                    line.append(' ').append(value);
                }
                System.out.println(line);
            }
        }
    }

    private static double[] outline(Object b, Object s, Object air, Object at, Class<?> view, Method byCollision, Method setBounds) throws ReflectiveOperationException {
        Object stub = Proxy.newProxyInstance(view.getClassLoader(), new Class<?>[] {view}, (proxy, method, args) -> {
            if (method.getName().equals("getBlockState")) {
                return args[0].equals(at) ? s : air;
            }
            if (method.getName().equals("isAir")) {
                return !args[0].equals(at);
            }
            Class<?> type = method.getReturnType();
            if (type == boolean.class) {
                return false;
            }
            if (type == int.class) {
                return 0;
            }
            return null;
        });
        try {
            if (byCollision != null) {
                Object box = byCollision.invoke(b, s, stub, at);
                if (box == null) {
                    return null;
                }
                Class<?> c = box.getClass();
                String[] fields = {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"};
                double[] out = new double[6];
                for (int i = 0; i < 6; i++) {
                    out[i] = c.getField(fields[i]).getDouble(box);
                }
                return out;
            }
            setBounds.invoke(b, stub, at);
            String[] getters = {"getMinX", "getMinY", "getMinZ", "getMaxX", "getMaxY", "getMaxZ"};
            double[] out = new double[6];
            for (int i = 0; i < 6; i++) {
                out[i] = (Double) b.getClass().getMethod(getters[i]).invoke(b);
            }
            return out;
        } catch (ReflectiveOperationException | RuntimeException e) {
            return null;
        }
    }

    private OracleLegacyOutlines() {}
}

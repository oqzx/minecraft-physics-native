import java.util.TreeMap;
import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Prints the collision shape of every block state of a jar with its boxes and grid lines, from EmptyBlockGetter at
 * BlockPos.ZERO: `state NAME PROPS flags boxes... x-lines y-lines z-lines`, each list a count and its numbers, boxes in
 * toAabbs order, then isFaceSturdy of the six faces. flags: 2 the shape is the Shapes.block() object.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleStates.java
 * java -cp "out;JARS" OracleStates > states.txt
 *
 * JARS is the Mojang-named jar and the libraries of its version json. scripts/java-oracle/compare-states.cjs
 * compares the lines with the shapes the adapter gives and writes the distinct shapes for the native test.
 */
public final class OracleStates {
    private static <T extends Comparable<T>> String valueName(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        java.io.PrintStream out = System.out;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        try {
            SharedConstants.class.getMethod("tryDetectVersion").invoke(null);
        } catch (NoSuchMethodException older) {
            // (the release has no such step)
        }
        Bootstrap.bootStrap();
        System.setOut(out);
        java.lang.reflect.Method coords = VoxelShape.class.getDeclaredMethod("getCoords", Direction.Axis.class);
        coords.setAccessible(true);
        java.lang.reflect.Method sturdy = null;
        for (java.lang.reflect.Method method : BlockState.class.getMethods()) {
            if (method.getName().equals("isFaceSturdy") && method.getParameterCount() == 3) {
                sturdy = method;
            }
        }
        VoxelShape whole = net.minecraft.world.phys.shapes.Shapes.block();
        StringBuilder all = new StringBuilder();
        // BuiltInRegistries from 1.19.3, Registry before
        Object registry = null;
        for (String holder : new String[] {"net.minecraft.core.registries.BuiltInRegistries", "net.minecraft.core.Registry"}) {
            try {
                registry = Class.forName(holder).getField("BLOCK").get(null);
                break;
            } catch (ReflectiveOperationException absent) {
                // the other one
            }
        }
        java.lang.reflect.Method keyOf = null;
        for (java.lang.reflect.Method method : net.minecraft.core.Registry.class.getMethods()) {
            if (method.getName().equals("getKey") && method.getParameterCount() == 1 && method.getParameterTypes()[0] == Object.class) {
                keyOf = method;
            }
        }
        for (Object value : (Iterable<?>) registry) {
            Block block = (Block) value;
            Object key = keyOf.invoke(registry, block);
            String name = (String) key.getClass().getMethod("getPath").invoke(key);
            for (BlockState state : block.getStateDefinition().getPossibleStates()) {
                VoxelShape shape = state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO);
                TreeMap<String, String> props = new TreeMap<>();
                state.getProperties().forEach(property -> props.put(property.getName(), valueName(state, property)));
                String text = props.entrySet().stream().map(entry -> entry.getKey() + "=" + entry.getValue()).collect(Collectors.joining(","));
                int flags = shape == whole ? 2 : 0;
                all.append("state ").append(name).append(' ').append(text.isEmpty() ? "-" : text).append(' ').append(flags);
                var boxes = shape.toAabbs();
                all.append(' ').append(boxes.size());
                for (AABB box : boxes) {
                    all.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ)
                        .append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ);
                }
                for (Direction.Axis axis : new Direction.Axis[] {Direction.Axis.X, Direction.Axis.Y, Direction.Axis.Z}) {
                    java.util.List<?> lines = (java.util.List<?>) coords.invoke(shape, axis);
                    all.append(' ').append(lines.size());
                    lines.forEach(line -> all.append(' ').append(line));
                }
                // isFaceSturdy of each Direction, 0 down to 5 east, from 1.15; a release without it says nothing
                if (sturdy != null) {
                    for (Direction direction : Direction.values()) {
                        all.append((boolean) sturdy.invoke(state, EmptyBlockGetter.INSTANCE, BlockPos.ZERO, direction) ? " 1" : " 0");
                    }
                }
                all.append('\n');
            }
        }
        System.out.print(all);
    }

    private OracleStates() {}
}

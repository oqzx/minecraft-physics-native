import java.util.TreeMap;
import java.util.stream.Collectors;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Prints BlockState.isFaceSturdy(DOWN) of every block state of a jar, and Block.isFaceFull(DOWN) of its collision shape
 * with that shape's boxes: `state NAME PROPS sturdy full n box...` (PROPS `-` for none). Shapes come from
 * EmptyBlockGetter at BlockPos.ZERO. The distinct shapes of it are native/fixtures/java-oracle-bottom-face-26.3.txt.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleSturdy.java
 * java -cp "out;JARS" OracleSturdy
 *
 * JARS is the jar and the libraries of its version json.
 */
public final class OracleSturdy {
    private static String flag(boolean value) {
        return value ? "1" : "0";
    }

    public static void main(String[] args) {
        java.io.PrintStream out = System.out;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(out);
        for (Block block : BuiltInRegistries.BLOCK) {
            String name = BuiltInRegistries.BLOCK.getKey(block).getPath();
            for (BlockState state : block.getStateDefinition().getPossibleStates()) {
                VoxelShape shape = state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO);
                boolean sturdy = state.isFaceSturdy(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, Direction.DOWN);
                boolean full = Block.isFaceFull(shape, Direction.DOWN);
                TreeMap<String, String> props = new TreeMap<>();
                state.getValues().forEach(value -> props.put(value.property().getName(), value.valueName()));
                String text = props.entrySet().stream().map(entry -> entry.getKey() + "=" + entry.getValue()).collect(Collectors.joining(","));
                StringBuilder line = new StringBuilder("state ").append(name).append(' ').append(text.isEmpty() ? "-" : text)
                    .append(' ').append(flag(sturdy)).append(' ').append(flag(full));
                var boxes = shape.toAabbs();
                line.append(' ').append(boxes.size());
                for (AABB box : boxes) {
                    line.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ)
                        .append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ);
                }
                System.out.println(line);
            }
        }
    }

    private OracleSturdy() {}
}

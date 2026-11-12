import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Random;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.border.WorldBorder;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Runs 1.13.2's own collision query over a world of real block states, for
 * native/fixtures/java-oracle-collisions-1.13.2.txt: LevelReader.method_16365 (the box swept and grown by 1.0E-7, less
 * the box moved back along the motion, the blocks the box is already in left out, the full cubes joined) and the
 * Shapes.collide axis order of Entity.move. The level is a proxy over the interface, so the default methods that run
 * are the jar's.
 *
 * `palette id n box...`, the collision boxes of a state in its block; then
 * `case box6 move3 cells n (x y z id)... shapes n (bounds6)... moved x y z`.
 *
 * javac -cp JARS -d out scripts/java-oracle/Oracle113.java
 * java -cp "out;JARS" Oracle113 > native/fixtures/java-oracle-collisions-1.13.2.txt
 *
 * JARS is the Mojang-named 1.13.2 jar and the libraries of its version json.
 */
public final class Oracle113 {
    private static final double E = 1.0E-7;
    private static final Random RANDOM = new Random(20261106L);
    private static final double[] SIDES = {
        0.0, 0.0, 0.0, E, -E, Math.nextDown(E), Math.nextUp(E), -Math.nextDown(E), -Math.nextUp(E), 5.0E-8, -5.0E-8, 1.5E-7, -1.5E-7, 2.0E-7, -2.0E-7, 1.0E-6, -1.0E-6,
    };
    private static final double[] MOVES = {
        0.0, 0.0, 0.0, 5.0E-8, -5.0E-8, E, -E, Math.nextDown(E), -Math.nextDown(E), 2.0E-7, -2.0E-7, 1.0E-5, -1.0E-5, 0.0784000015258789, -0.0784000015258789, 0.3, -0.3, 1.1, -1.1,
    };

    private static double pick(double[] values) {
        return values[RANDOM.nextInt(values.length)];
    }

    private static LevelReader level(Map<BlockPos, BlockState> blocks) {
        WorldBorder border = new WorldBorder();
        InvocationHandler handler = (proxy, method, args) -> {
            if (method.isDefault()) {
                return InvocationHandler.invokeDefault(proxy, method, args);
            }
            return switch (method.getName()) {
                case "getBlockState" -> blocks.getOrDefault(args[0], Blocks.AIR.defaultBlockState());
                case "getWorldBorder" -> border;
                case "method_8555", "hasChunk" -> true;
                case "hashCode" -> 0;
                default -> throw new UnsupportedOperationException(method.getName());
            };
        };
        return (LevelReader) Proxy.newProxyInstance(Oracle113.class.getClassLoader(), new Class<?>[] {LevelReader.class}, handler);
    }

    private static void append(StringBuilder text, AABB box) {
        text.append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ)
            .append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ);
    }

    private static List<VoxelShape> query(LevelReader level, AABB box, double x, double y, double z) {
        List<VoxelShape> shapes = new ArrayList<>();
        Iterator<VoxelShape> stream = level.method_16365(null, box, x, y, z).iterator();
        while (stream.hasNext()) {
            shapes.add(stream.next());
        }
        return shapes;
    }

    public static void main(String[] args) {
        java.io.PrintStream out = System.out;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        Bootstrap.bootStrap();
        System.setOut(out);
        List<BlockState> palette = new ArrayList<>();
        for (var block : List.of(
            Blocks.STONE, Blocks.STONE_SLAB, Blocks.OAK_FENCE, Blocks.COBBLESTONE_WALL, Blocks.WHITE_CARPET, Blocks.OAK_STAIRS, Blocks.SNOW,
            Blocks.CHEST, Blocks.SOUL_SAND, Blocks.FARMLAND, Blocks.CACTUS, Blocks.HOPPER, Blocks.ENCHANTING_TABLE, Blocks.END_PORTAL_FRAME,
            Blocks.LILY_PAD, Blocks.BREWING_STAND)) {
            palette.add(block.defaultBlockState());
        }
        LevelReader empty = level(Map.of());
        for (int id = 0; id < palette.size(); id++) {
            List<AABB> boxes = palette.get(id).getCollisionShape(empty, BlockPos.ZERO).toAabbs();
            StringBuilder text = new StringBuilder("palette ").append(id).append(' ').append(boxes.size());
            boxes.forEach(box -> append(text, box));
            System.out.println(text);
        }
        int count = args.length > 0 ? Integer.parseInt(args[0]) : 3000;
        for (int i = 0; i < count; i++) {
            scene(palette, i % 2 == 0 ? new int[] {0, 0, 0} : new int[] {5, 64, -3});
        }
    }

    private static void scene(List<BlockState> palette, int[] anchor) {
        Map<BlockPos, BlockState> blocks = new HashMap<>();
        List<int[]> cells = new ArrayList<>();
        int targetId = RANDOM.nextInt(palette.size());
        place(blocks, cells, palette, anchor, targetId);
        for (int n = RANDOM.nextInt(6); n > 0; n--) {
            int[] at = {anchor[0] + RANDOM.nextInt(5) - 2, anchor[1] + RANDOM.nextInt(4) - 2, anchor[2] + RANDOM.nextInt(5) - 2};
            if (blocks.keySet().stream().noneMatch(pos -> pos.getX() == at[0] && pos.getY() == at[1] && pos.getZ() == at[2])) {
                place(blocks, cells, palette, at, RANDOM.nextInt(palette.size()));
            }
        }
        LevelReader level = level(blocks);
        AABB target = palette.get(targetId).getCollisionShape(level, new BlockPos(anchor[0], anchor[1], anchor[2])).toAabbs().stream().findFirst()
            .orElse(new AABB(0, 0, 0, 1, 1, 1)).move(anchor[0], anchor[1], anchor[2]);
        double width = RANDOM.nextBoolean() ? 0.6000000238418579 : 0.25;
        double height = RANDOM.nextBoolean() ? 1.8 : 0.5625;
        // beside a face of the target block, by a side around the tolerance, or over it
        int axis = RANDOM.nextInt(3);
        boolean high = RANDOM.nextBoolean();
        double[] min = {
            target.minX + RANDOM.nextDouble() * (target.maxX - target.minX) - width / 2,
            target.minY + RANDOM.nextDouble() * (target.maxY - target.minY) - height / 2,
            target.minZ + RANDOM.nextDouble() * (target.maxZ - target.minZ) - width / 2,
        };
        double[] size = {width, height, width};
        double[] low = {target.minX, target.minY, target.minZ};
        double[] top = {target.maxX, target.maxY, target.maxZ};
        min[axis] = (high ? top[axis] : low[axis] - size[axis]) + pick(SIDES);
        if (RANDOM.nextBoolean()) {
            // and flush with a second face, where a touching shape is met or not by the merger alone
            int other = (axis + 1 + RANDOM.nextInt(2)) % 3;
            min[other] = (RANDOM.nextBoolean() ? top[other] : low[other] - size[other]) + pick(SIDES);
        }
        AABB box = new AABB(min[0], min[1], min[2], min[0] + width, min[1] + height, min[2] + width);
        double x = pick(MOVES);
        double y = pick(MOVES);
        double z = pick(MOVES);
        if (x == 0.0 && y == 0.0 && z == 0.0) {
            x = pick(new double[] {5.0E-8, -5.0E-8, 0.3});
        }
        List<VoxelShape> shapes = query(level, box, x, y, z);
        StringBuilder text = new StringBuilder("case");
        append(text, box);
        text.append(' ').append(x).append(' ').append(y).append(' ').append(z).append(" cells ").append(cells.size());
        for (int[] cell : cells) {
            text.append(' ').append(cell[0]).append(' ').append(cell[1]).append(' ').append(cell[2]).append(' ').append(cell[3]);
        }
        text.append(" shapes ").append(shapes.size());
        shapes.forEach(shape -> append(text, shape.bounds()));
        // Entity.move (1.13.2 Entity.java:672-692)
        AABB moving = box;
        if (y != 0.0) {
            y = Shapes.collide(Direction.Axis.Y, moving, shapes.stream(), y);
            moving = moving.move(0.0, y, 0.0);
        }
        if (x != 0.0) {
            x = Shapes.collide(Direction.Axis.X, moving, shapes.stream(), x);
            if (x != 0.0) {
                moving = moving.move(x, 0.0, 0.0);
            }
        }
        if (z != 0.0) {
            z = Shapes.collide(Direction.Axis.Z, moving, shapes.stream(), z);
        }
        text.append(" moved ").append(x).append(' ').append(y).append(' ').append(z);
        System.out.println(text);
    }

    private static void place(Map<BlockPos, BlockState> blocks, List<int[]> cells, List<BlockState> palette, int[] at, int id) {
        blocks.put(new BlockPos(at[0], at[1], at[2]), palette.get(id));
        cells.add(new int[] {at[0], at[1], at[2], id});
    }

    private Oracle113() {}
}

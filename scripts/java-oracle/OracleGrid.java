import java.lang.reflect.Method;
import java.util.Random;
import net.minecraft.core.Direction;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * Prints the grid lines Shapes.create gives boxes on and around a 1/2^n grid, whose test (Shapes.findBits) changes at
 * 1.17, for native/fixtures/java-oracle-grid.txt: `grid RELEASE box6 = n lines... n lines... n lines...`, the lines of
 * X, Y and Z.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleGrid.java
 * java -cp "out;JARS" OracleGrid RELEASE >> native/fixtures/java-oracle-grid.txt
 *
 * JARS is the release's Mojang-named jar with fastutil and guava.
 */
public final class OracleGrid {
    private static final double[] LINES = {0.0, 0.125, 0.25, 0.375, 0.5, 0.75, 1.0};
    private static final double[] SIDES = {
        0.0, 0.0, 1.0E-8, -1.0E-8, 5.0E-8, -5.0E-8, 9.0E-8, -9.0E-8, 1.0E-7, -1.0E-7, 1.2E-7, -1.2E-7, 2.0E-7, -2.0E-7, 3.0E-7, -3.0E-7, 7.0E-7, -7.0E-7,
    };

    public static void main(String[] args) throws ReflectiveOperationException {
        Random random = new Random(20261107L);
        Method coords = VoxelShape.class.getDeclaredMethod("getCoords", Direction.Axis.class);
        coords.setAccessible(true);
        for (int i = 0; i < 800; i++) {
            double[] box = new double[6];
            for (int axis = 0; axis < 3; axis++) {
                double low = LINES[random.nextInt(LINES.length - 1)] + (i % 3 == axis ? SIDES[random.nextInt(SIDES.length)] : 0.0);
                double high = LINES[1 + random.nextInt(LINES.length - 1)] + (i % 3 == axis ? SIDES[random.nextInt(SIDES.length)] : 0.0);
                box[axis] = Math.min(low, high);
                box[axis + 3] = Math.max(low, high);
                if (box[axis] == box[axis + 3]) {
                    box[axis + 3] += 0.25;
                }
            }
            VoxelShape shape = Shapes.create(new AABB(box[0], box[1], box[2], box[3], box[4], box[5]));
            if (shape.isEmpty()) {
                continue;
            }
            StringBuilder text = new StringBuilder("grid ").append(args[0]);
            for (double value : box) {
                text.append(' ').append(value);
            }
            text.append(" =");
            for (Direction.Axis axis : new Direction.Axis[] {Direction.Axis.X, Direction.Axis.Y, Direction.Axis.Z}) {
                java.util.List<?> lines = (java.util.List<?>) coords.invoke(shape, axis);
                text.append(' ').append(lines.size());
                lines.forEach(line -> text.append(' ').append(line));
            }
            System.out.println(text);
        }
    }

    private OracleGrid() {}
}

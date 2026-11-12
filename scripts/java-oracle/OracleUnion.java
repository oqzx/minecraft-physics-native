import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import java.util.Random;
import net.minecraft.core.Direction;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/**
 * What Shapes.or makes of a list of boxes, through a jar's own classes, for native/fixtures/java-oracle-union.txt:
 * `union RELEASE n (ox oy oz box6)... = m box6...` is the ordered toAabbs of the reduce, each input a Shapes.box moved by
 * (ox oy oz); `join` is the same without optimize (Shapes.joinUnoptimized over the inputs); `collide RELEASE join|union axis
 * entity6 delta = result` asks each of them, so a move before and after the optimization point is compared.
 *
 * The inputs are one shape, duplicates, overlaps and the same boxes reordered, on lines of 1/8 and on lines beside them by
 * less than, exactly and more than 1.0E-7, and moved by a whole or a fraction of a block.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleUnion.java
 * java -cp "out;JARS" OracleUnion RELEASE >> native/fixtures/java-oracle-union.txt
 */
public final class OracleUnion {
    private static final double[] SIDES = {0.0, 0.0, 0.0, 5.0E-8, -5.0E-8, 1.0E-7, -1.0E-7, 1.2E-7, -1.2E-7, 3.0E-7, -3.0E-7};
    private static final Direction.Axis[] AXES = {Direction.Axis.X, Direction.Axis.Y, Direction.Axis.Z};
    private static String release;

    private record Input(double ox, double oy, double oz, double[] box) {
        VoxelShape shape() throws ReflectiveOperationException {
            VoxelShape shape = Shapes.box(box[0], box[1], box[2], box[3], box[4], box[5]);
            return ox == 0.0 && oy == 0.0 && oz == 0.0 ? shape : moved(shape, ox, oy, oz);
        }

        @Override
        public String toString() {
            StringBuilder out = new StringBuilder(ox + " " + oy + " " + oz);
            for (double v : box) out.append(' ').append(v);
            return out.toString();
        }
    }

    private static VoxelShape moved(VoxelShape shape, double x, double y, double z) throws ReflectiveOperationException {
        for (Method m : VoxelShape.class.getMethods()) {
            if (!m.getName().equals("move")) continue;
            if (m.getParameterCount() == 3) return (VoxelShape) m.invoke(shape, x, y, z);
            if (m.getParameterTypes()[0] != net.minecraft.world.phys.Vec3.class) continue;
            return (VoxelShape) m.invoke(shape, new net.minecraft.world.phys.Vec3(x, y, z));
        }
        throw new NoSuchMethodException("VoxelShape.move");
    }

    private static String boxes(VoxelShape shape) {
        List<AABB> list = shape.toAabbs();
        StringBuilder out = new StringBuilder().append(list.size());
        for (AABB b : list) {
            out.append(' ').append(b.minX).append(' ').append(b.minY).append(' ').append(b.minZ)
                .append(' ').append(b.maxX).append(' ').append(b.maxY).append(' ').append(b.maxZ);
        }
        return out.toString();
    }

    private static String line(List<Input> inputs) {
        StringBuilder out = new StringBuilder().append(inputs.size());
        for (Input input : inputs) out.append(' ').append(input);
        return out.toString();
    }

    private static void ask(String name, VoxelShape shape, Random random) {
        for (int i = 0; i < 4; i++) {
            Direction.Axis axis = AXES[random.nextInt(3)];
            double[] e = new double[6];
            for (int a = 0; a < 3; a++) {
                double low = random.nextInt(9) / 8.0 - 0.5 + SIDES[random.nextInt(SIDES.length)];
                e[a] = low;
                e[a + 3] = low + 0.25 + random.nextInt(3) / 8.0;
            }
            double delta = (random.nextBoolean() ? 1.0 : -1.0) * (0.05 + random.nextInt(8) / 8.0);
            AABB entity = new AABB(e[0], e[1], e[2], e[3], e[4], e[5]);
            System.out.println("collide " + release + " " + name + " " + axis.ordinal() + " " + e[0] + " " + e[1] + " " + e[2] + " " + e[3] + " " + e[4] + " " + e[5]
                + " " + delta + " = " + shape.collide(axis, entity, delta));
        }
    }

    private static void run(List<Input> inputs, Random random) throws ReflectiveOperationException {
        VoxelShape first = inputs.get(0).shape();
        VoxelShape[] tail = new VoxelShape[inputs.size() - 1];
        for (int i = 1; i < inputs.size(); i++) tail[i - 1] = inputs.get(i).shape();
        VoxelShape union = Shapes.or(first, tail);
        System.out.println("union " + release + " " + line(inputs) + " = " + boxes(union));
        ask("union", union, random);
        if (tail.length == 1) {
            VoxelShape join = Shapes.joinUnoptimized(first, tail[0], BooleanOp.OR);
            System.out.println("join " + release + " " + line(inputs) + " = " + boxes(join));
            ask("join", join, random);
        }
    }

    private static double[] box(Random random, boolean nearby) {
        double[] box = new double[6];
        for (int a = 0; a < 3; a++) {
            int low = random.nextInt(7);
            int high = low + 1 + random.nextInt(8 - low);
            box[a] = low / 8.0 + (nearby ? SIDES[random.nextInt(SIDES.length)] : 0.0);
            box[a + 3] = high / 8.0 + (nearby ? SIDES[random.nextInt(SIDES.length)] : 0.0);
        }
        return box;
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        release = args[0];
        Random random = new Random(20261112L);
        for (int i = 0; i < 150; i++) {
            boolean nearby = i % 2 == 1;
            boolean movedInputs = i % 4 >= 2;
            int count = 1 + random.nextInt(4);
            List<Input> inputs = new ArrayList<>();
            for (int n = 0; n < count; n++) {
                double[] o = {0.0, 0.0, 0.0};
                if (movedInputs) for (int a = 0; a < 3; a++) o[a] = random.nextInt(3) == 0 ? random.nextInt(3) * (random.nextBoolean() ? 1.0 : 0.0625) : 0.0;
                inputs.add(new Input(o[0], o[1], o[2], box(random, nearby)));
            }
            run(inputs, random);
            // the same again: duplicates, and the list turned round
            if (i % 8 == 0) {
                List<Input> twice = new ArrayList<>(inputs);
                twice.addAll(inputs);
                run(twice, random);
            }
            if (count > 1) {
                List<Input> reversed = new ArrayList<>(inputs);
                java.util.Collections.reverse(reversed);
                run(reversed, random);
            }
        }
    }
}

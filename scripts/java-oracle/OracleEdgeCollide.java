import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.io.OutputStream;
import java.io.PrintStream;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;

/**
 * Collision at the edges the seeded oracles leave out, through a jar's own classes, for native/fixtures/java-oracle-edge-collide.txt:
 * planes that touch, gaps either side of the 1.0E-7 of a tiny move, cells at negative coordinates, and a diagonal whose axis order
 * decides where it ends. Each scene runs at the origin, at negative cells and at a fraction of a block.
 *
 *   edge RELEASE box6 delta3 n (ox oy oz box6)... = x y z   the whole move: Entity.collideWithShapes from 1.18, Entity.collideBoundingBox
 *                                                           over a RewindableStream from 1.14 (collideBoundingBoxLegacy: the path of a move on two axes or more), each collider a Shapes.create moved to its cell
 *   axis RELEASE axis box6 delta n (ox oy oz box6)... = d   Shapes.collide of one axis on 1.13.2
 *   offset RELEASE axis box6 other6 delta = d               AABB's clip of one axis before 1.13 (1.12.2 AABB.java:202, 222, 242)
 *
 * No class of a jar is compiled against, so one source runs on all of them.
 *
 * javac -d out scripts/java-oracle/OracleEdgeCollide.java
 * java -cp "out;JARS" OracleEdgeCollide RELEASE >> native/fixtures/java-oracle-edge-collide.txt
 */
public final class OracleEdgeCollide {
    private static final double E = 1.0E-7;
    private static final double[] GAPS = {0.0, E, Math.nextDown(E), Math.nextUp(E), 5.0E-8, -1.0E-8};
    private static final double[] STEPS = {0.25, E, Math.nextDown(E), Math.nextUp(E)};
    private static final double[] SIDES = {0.0, E, -E};
    private static final double[][] ORIGINS = {{-3, 64, -5}, {-1, -1, -1}};
    private static final double[][] LOCAL = {{0, 0, 0, 1, 1, 1}, {0.25, 0, 0.25, 0.75, 0.5, 0.75}};
    private static final double[][] DIAGONALS = {
        {0.5, 0, 0.3}, {0.3, 0, 0.5}, {-0.3, 0, -0.5}, {0.5, -0.5, 0.3}, {0.3, 0.4, -0.5}, {-0.5, -0.5, -0.5},
    };
    private static Constructor<?> aabb;
    private static Constructor<?> vec3;
    private static Method create;
    private static Method move;
    private static Method collideWithShapes;
    private static Method collideBoundingBox;
    private static Method shapesCollide;
    private static Constructor<?> rewindable;
    private static Object[] axes;
    private static Method[] offsets;
    private static String release;
    private static StringBuilder text = new StringBuilder();

    private static Class<?> type(String... names) {
        for (String name : names) {
            try {
                return Class.forName(name, false, OracleEdgeCollide.class.getClassLoader());
            } catch (ClassNotFoundException | LinkageError absent) {
                // the next name
            }
        }
        return null;
    }

    private static Object box(double[] b) throws Exception {
        return aabb.newInstance(b[0], b[1], b[2], b[3], b[4], b[5]);
    }

    private static String join(double[] values) {
        StringBuilder out = new StringBuilder();
        for (double value : values) {
            out.append(out.length() == 0 ? "" : " ").append(value);
        }
        return out.toString();
    }

    private static double[] moved(double[] b, double x, double y, double z) {
        return new double[] {b[0] + x, b[1] + y, b[2] + z, b[3] + x, b[4] + y, b[5] + z};
    }

    private static void whole(double[] entity, double[] delta, double[][] cells, double[] local) throws Exception {
        if (axes != null && collideWithShapes == null && collideBoundingBox == null && shapesCollide == null && offsets == null) {
            return;
        }
        if (offsets != null) {
            for (int axis = 0; axis < 3; axis++) {
                if (delta[axis] == 0.0) {
                    continue;
                }
                for (double[] cell : cells) {
                    double[] other = moved(local, cell[0], cell[1], cell[2]);
                    double answer = (Double) offsets[axis].invoke(box(other), box(entity), delta[axis]);
                    text.append("offset ").append(release).append(' ').append("xyz".charAt(axis)).append(' ').append(join(entity)).append(' ').append(join(other))
                        .append(' ').append(delta[axis]).append(" = ").append(answer).append('\n');
                }
            }
            return;
        }
        // (a shape the sweep does not reach is no collider: the walk of the blocks would not offer it)
        List<double[]> reached = new ArrayList<>();
        for (double[] cell : cells) {
            double[] at = moved(local, cell[0], cell[1], cell[2]);
            boolean meets = true;
            for (int i = 0; i < 3; i++) {
                double low = entity[i] + Math.min(delta[i], 0.0);
                double high = entity[i + 3] + Math.max(delta[i], 0.0);
                meets &= low < at[i + 3] && high > at[i];
            }
            if (meets) {
                reached.add(cell);
            }
        }
        if (reached.isEmpty()) {
            return;
        }
        StringBuilder listed = new StringBuilder().append(reached.size());
        List<Object> shapes = new ArrayList<>();
        for (double[] cell : reached) {
            Object shape = create.invoke(null, box(local));
            shapes.add(move.invoke(shape, cell[0], cell[1], cell[2]));
            listed.append(' ').append(join(cell)).append(' ').append(join(local));
        }
        if (shapesCollide != null) {
            for (int axis = 0; axis < 3; axis++) {
                if (delta[axis] == 0.0) {
                    continue;
                }
                double answer = (Double) shapesCollide.invoke(null, axes[axis], box(entity), shapes.stream(), delta[axis]);
                text.append("axis ").append(release).append(' ').append("xyz".charAt(axis)).append(' ').append(join(entity)).append(' ').append(delta[axis]).append(' ')
                    .append(listed).append(" = ").append(answer).append('\n');
            }
            return;
        }
        Object vector = vec3.newInstance(delta[0], delta[1], delta[2]);
        Object answer = collideWithShapes != null
            ? collideWithShapes.invoke(null, vector, box(entity), shapes)
            : collideBoundingBox.invoke(null, vector, box(entity), rewindable.newInstance(shapes.stream()));
        double[] result = new double[3];
        String[] names = {"x", "y", "z"};
        for (int i = 0; i < 3; i++) {
            result[i] = answer.getClass().getField(names[i]).getDouble(answer);
        }
        text.append("edge ").append(release).append(' ').append(join(entity)).append(' ').append(join(delta)).append(' ').append(listed).append(" = ").append(join(result)).append('\n');
    }

    /** A box beside a cell along each axis at each gap, moving toward it, touching or just off its side planes. */
    private static void planes() throws Exception {
        for (double[] origin : ORIGINS) {
            for (double[] local : LOCAL) {
                double[][] cell = {origin};
                for (int axis = 0; axis < 3; axis++) {
                    int one = (axis + 1) % 3;
                    int other = (axis + 2) % 3;
                    double[] size = {0.6, 1.8, 0.6};
                    for (int sign = -1; sign <= 1; sign += 2) {
                        double face = origin[axis] + (sign > 0 ? local[axis] : local[axis + 3]);
                        for (double gap : GAPS) {
                            for (double step : STEPS) {
                                for (double side : SIDES) {
                                    double[] min = new double[3];
                                    min[axis] = sign > 0 ? face - gap - size[axis] : face + gap;
                                    min[one] = origin[one] + local[one] - size[one] + side;
                                    min[other] = origin[other] + 0.2;
                                    double[] delta = new double[3];
                                    delta[axis] = sign * step;
                                    whole(new double[] {min[0], min[1], min[2], min[0] + size[0], min[1] + size[1], min[2] + size[2]}, delta, cell, local);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /** Around the corners of a block and an L of blocks, a diagonal that one order of the axes lets by and another does not. */
    private static void diagonals() throws Exception {
        for (double[] origin : ORIGINS) {
            double[][][] scenes = {
                {origin},
                {origin, {origin[0] + 1, origin[1], origin[2]}, {origin[0], origin[1], origin[2] + 1}},
                {origin, {origin[0], origin[1] + 1, origin[2]}, {origin[0] + 1, origin[1] + 1, origin[2] + 1}},
            };
            for (double[][] scene : scenes) {
                for (double[] delta : DIAGONALS) {
                    for (double dx : new double[] {-0.9, -0.61, -0.6, 0.4, 1.1}) {
                        for (double dz : new double[] {-0.9, -0.61, -0.6, 0.4, 1.1}) {
                            for (double dy : new double[] {0.25}) {
                                double x = origin[0] + dx;
                                double y = origin[1] + dy;
                                double z = origin[2] + dz;
                                whole(new double[] {x, y, z, x + 0.6, y + 1.8, z + 0.6}, delta, scene, LOCAL[0]);
                            }
                        }
                    }
                }
            }
        }
    }

    public static void main(String[] args) throws Exception {
        release = args[0];
        PrintStream real = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        Class<?> detected = type("net.minecraft.SharedConstants");
        for (Method candidate : detected == null ? new Method[0] : detected.getMethods()) {
            if (candidate.getName().equals("tryDetectVersion")) {
                candidate.invoke(null);
            }
        }
        type("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Class<?> boxes = type("net.minecraft.world.phys.AABB");
        aabb = boxes.getConstructor(double.class, double.class, double.class, double.class, double.class, double.class);
        Class<?> shapes = type("net.minecraft.world.phys.shapes.Shapes");
        if (shapes == null) {
            offsets = new Method[] {
                boxes.getMethod("method_583", boxes, double.class), boxes.getMethod("method_589", boxes, double.class), boxes.getMethod("method_594", boxes, double.class),
            };
        } else {
            Class<?> vectors = type("net.minecraft.world.phys.Vec3");
            vec3 = vectors.getConstructor(double.class, double.class, double.class);
            create = shapes.getMethod("create", boxes);
            move = type("net.minecraft.world.phys.shapes.VoxelShape").getMethod("move", double.class, double.class, double.class);
            Class<?> entity = type("net.minecraft.world.entity.Entity");
            Class<?> axis = type("net.minecraft.core.Direction$Axis");
            axes = axis.getEnumConstants();
            Class<?> stream = type("net.minecraft.util.RewindableStream");
            for (Method candidate : entity.getDeclaredMethods()) {
                if (candidate.getName().equals("collideWithShapes") && Modifier.isStatic(candidate.getModifiers()) && candidate.getParameterCount() == 3) {
                    candidate.setAccessible(true);
                    collideWithShapes = candidate;
                }
                if (stream != null && candidate.getName().startsWith("collideBoundingBox") && candidate.getParameterCount() >= 3 && candidate.getParameterTypes()[candidate.getParameterCount() - 1] == stream
                    && candidate.getParameterCount() == 3) {
                    candidate.setAccessible(true);
                    collideBoundingBox = candidate;
                }
            }
            if (stream != null) {
                rewindable = stream.getConstructor(Stream.class);
            }
            if (collideWithShapes == null && collideBoundingBox == null) {
                for (Method candidate : shapes.getMethods()) {
                    if (candidate.getName().equals("collide") && candidate.getParameterCount() == 4 && candidate.getParameterTypes()[2] == Stream.class) {
                        shapesCollide = candidate;
                    }
                }
            }
        }
        planes();
        if (offsets == null && shapesCollide == null) {
            diagonals();
        }
        System.setOut(real);
        real.print(text);
        real.flush();
        System.exit(0);
    }
}

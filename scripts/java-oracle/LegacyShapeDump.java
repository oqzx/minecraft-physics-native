import java.io.BufferedWriter;
import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.reflect.Array;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;

/**
 * A block state's geometry in a release before 1.14, from the release's own code in a level of its own. A shape there
 * needs a world: `appendCollisionBoxes` takes a Level, and a block may read itself back from it. So each state is put in
 * a real LevelChunkSection of a blank chunk at POS, in a client level with no constructor run and a chunk source that
 * hands that chunk out, and every other cell of the world is air. That is the case this answers: a block alone. What its
 * neighbours do to it (a fence's arms) is not here. The one exception is the half of a door, which is never alone: an
 * upper half reads its facing from the lower half below it, so that one is put there.
 *
 * The jar is the one the recorder runs: Mojang's names of 1.14.4 mapped on by Calamus ids (the recorder's mod/legacy-mappings.py).
 *
 * Lines, tab separated, boxes as in ShapeDump.java, relative to the block:
 *   state NAME ID PROPS collision outline full
 *   refused NAME ID PROPS channel reason        a query the blank level could not answer: a block entity, as a rule
 *
 * ID is Block.getId(state): (block id << 4) | metadata before 1.13, the state id from it. `-` is a channel the release
 * has no method for. `full` is World.isBlockFullCube's answer for the state alone before 1.13: its
 * getCollisionBoundingBox (Block.getCollisionShape returning an AABB, 1.12.2 Block.java:405-407) is there and its
 * average edge is at least 1 (1.12.2 World.java:1835-1838, 1.8.9 World.java isBlockFullCube): 1 or 0, `-` from 1.13.
 *
 * java -XX:+UnlockDiagnosticVMOptions -XX:-BytecodeVerificationRemote -cp "out;JARS" LegacyShapeDump OUTPUT_FILE
 *
 * (The remapped jars of these releases do not pass the verifier of a modern JVM: a protected field read across packages.)
 */
public final class LegacyShapeDump {
    private static final String ABSENT = "-";
    private static final int X = 0;
    private static final int Y = 65;
    private static final int Z = 0;
    private static Object origin;
    private static Object level;
    private static Object chunk;
    private static Object section;
    private static Field sectionsField;
    private static Object air;

    private static Class<?> type(String... names) {
        for (String name : names) {
            try {
                return Class.forName(name);
            } catch (ClassNotFoundException | LinkageError absent) {
                // the next name
            }
        }
        return null;
    }

    private static Method method(Class<?> owner, String name, int parameters) {
        for (Class<?> at = owner; at != null; at = at.getSuperclass()) {
            for (Method candidate : at.getDeclaredMethods()) {
                if (candidate.getName().equals(name) && candidate.getParameterCount() == parameters && !java.lang.reflect.Modifier.isStatic(candidate.getModifiers())) {
                    candidate.setAccessible(true);
                    return candidate;
                }
            }
        }
        // (BlockState is an interface of default methods in 1.13)
        for (Method candidate : owner.getMethods()) {
            if (candidate.getName().equals(name) && candidate.getParameterCount() == parameters && !java.lang.reflect.Modifier.isStatic(candidate.getModifiers())) {
                candidate.setAccessible(true);
                return candidate;
            }
        }
        return null;
    }

    private static Field field(Class<?> owner, String name) {
        for (Class<?> at = owner; at != null; at = at.getSuperclass()) {
            for (Field candidate : at.getDeclaredFields()) {
                if (candidate.getName().equals(name)) {
                    candidate.setAccessible(true);
                    return candidate;
                }
            }
        }
        return null;
    }

    private static sun.misc.Unsafe unsafe() throws Exception {
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        return (sun.misc.Unsafe) theUnsafe.get(null);
    }

    private static Object blankOf(Class<?> owner) throws Exception {
        if (owner == null) {
            throw new IllegalStateException("A class this oracle needs is not in the jar");
        }
        return unsafe().allocateInstance(owner);
    }

    /** The blank level: its chunk source hands out one chunk, with one real section at POS. */
    private static void prepareLevel() throws Exception {
        Class<?> sectionClass = type("net.minecraft.world.level.chunk.LevelChunkSection");
        Constructor<?> make = sectionClass.getConstructor(int.class, boolean.class);
        section = make.newInstance(Y & ~15, true);
        Class<?> chunkClass = type("net.minecraft.world.level.chunk.LevelChunk");
        chunk = blankOf(chunkClass);
        sectionsField = field(chunkClass, "sections");
        Object sections = Array.newInstance(sectionClass, 16);
        Array.set(sections, Y >> 4, section);
        sectionsField.set(chunk, sections);
        Class<?> sourceClass = type("net.minecraft.world.level.chunk.ChunkSource");
        Object source = Proxy.newProxyInstance(sourceClass.getClassLoader(), new Class<?>[] {sourceClass}, (proxy, called, arguments) -> {
            Class<?> result = called.getReturnType();
            if (result == boolean.class) {
                return true;
            }
            if (result.isInstance(chunk)) {
                return chunk;
            }
            return null;
        });
        Class<?> levelClass = type("net.minecraft.client.multiplayer.MultiPlayerLevel", "net.minecraft.client.multiplayer.ClientLevel");
        level = blankOf(levelClass);
        field(levelClass, "chunkSource").set(level, source);
        Field client = field(levelClass, "isClientSide");
        if (client != null) {
            client.setBoolean(level, true);
        }
        // (LevelChunk.getBlockState asks its level whether this is a debug world)
        Field owner = field(chunkClass, "level");
        if (owner != null) {
            owner.set(chunk, level);
        }
        Field data = field(levelClass, "levelData");
        Class<?> levelData = type("net.minecraft.world.level.storage.LevelData");
        Class<?> levelType = type("net.minecraft.world.level.LevelType");
        if (data != null && levelData != null) {
            Object blank = blankOf(levelData);
            Field generator = field(levelData, "generator");
            if (generator != null && levelType != null) {
                Field normal2 = field(levelType, "NORMAL");
                if (normal2 != null) {
                    generator.set(blank, normal2.get(null));
                }
            }
            data.set(level, blank);
        }
        Field dimension = field(levelClass, "dimension");
        Class<?> normal = type("net.minecraft.world.level.dimension.NormalDimension", "net.minecraft.world.level.dimension.OverworldDimension");
        if (dimension != null && normal != null) {
            dimension.set(level, blankOf(normal));
        }
        Class<?> border = type("net.minecraft.world.level.border.WorldBorder");
        Field borderField = field(levelClass, "worldBorder");
        if (borderField != null && border != null) {
            borderField.set(level, border.getConstructor().newInstance());
        }
    }

    private static void place(Object state) throws Exception {
        method(section.getClass(), "setBlockState", 4).invoke(section, X & 15, Y & 15, Z & 15, state);
    }

    /** What is under the cell: the block's own default state beneath the upper half of a door, else air. */
    private static void placeBelow(Object state) throws Exception {
        method(section.getClass(), "setBlockState", 4).invoke(section, X & 15, (Y & 15) - 1, Z & 15, state);
    }

    private static String boxes(Iterable<?> list, boolean absolute) throws Exception {
        StringBuilder text = new StringBuilder();
        for (Object box : list) {
            if (text.length() > 0) {
                text.append(';');
            }
            String[] names = {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"};
            for (int i = 0; i < 6; i++) {
                double value = box.getClass().getField(names[i]).getDouble(box) - (absolute && (i == 1 || i == 4) ? Y : 0);
                text.append(i == 0 ? "" : ",").append(value);
            }
        }
        return text.toString();
    }

    private static Object[] arguments(Method found, Object state, Object query, List<Object> list) {
        Object[] values = new Object[found.getParameterCount()];
        Class<?>[] kinds = found.getParameterTypes();
        for (int i = 0; i < values.length; i++) {
            String name = kinds[i].getSimpleName();
            if (name.equals("BlockState")) {
                values[i] = state;
            } else if (name.equals("BlockPos")) {
                values[i] = origin;
            } else if (name.equals("AABB")) {
                values[i] = query;
            } else if (List.class.isAssignableFrom(kinds[i])) {
                values[i] = list;
            } else if (kinds[i] == boolean.class) {
                values[i] = false;
            } else if (name.equals("Entity")) {
                values[i] = null;
            } else {
                values[i] = level;
            }
        }
        return values;
    }

    private static String collision(Object state, Object block, Object query) throws Exception {
        Method shape = method(state.getClass(), "getCollisionShape", 2);
        if (shape != null && !java.lang.reflect.Modifier.isStatic(shape.getModifiers()) && shape.getReturnType().getSimpleName().equals("VoxelShape")) {
            Object answer = shape.invoke(state, level, origin);
            return boxes((List<?>) method(answer.getClass(), "toAabbs", 0).invoke(answer), false);
        }
        Method append = null;
        for (Class<?> at = block.getClass(); at != null && append == null; at = at.getSuperclass()) {
            for (Method candidate : at.getDeclaredMethods()) {
                if (candidate.getName().equals("appendCollisionBoxes") && candidate.getParameterCount() >= 6) {
                    append = candidate;
                }
            }
        }
        append.setAccessible(true);
        List<Object> list = new ArrayList<>();
        append.invoke(block, arguments(append, state, query, list));
        return boxes(list, true);
    }

    private static String outline(Object state, Object block) throws Exception {
        Method onState = method(state.getClass(), "getShape", 2);
        if (onState != null && onState.getReturnType().getSimpleName().equals("VoxelShape")) {
            Object answer = onState.invoke(state, level, origin);
            return boxes((List<?>) method(answer.getClass(), "toAabbs", 0).invoke(answer), false);
        }
        Method onBlock = method(block.getClass(), "getShape", 3);
        if (onBlock == null) {
            return ABSENT;
        }
        return boxes(List.of(onBlock.invoke(block, arguments(onBlock, state, null, null))), false);
    }

    private static String full(Object state, Object block) throws Exception {
        Method box = null;
        for (Method candidate : block.getClass().getMethods()) {
            if (candidate.getName().equals("getCollisionShape") && candidate.getParameterCount() == 3
                && candidate.getReturnType().getSimpleName().equals("AABB")) {
                box = candidate;
            }
        }
        if (box == null) {
            return ABSENT;
        }
        Object answer = box.invoke(block, arguments(box, state, null, null));
        if (answer == null) {
            return "0";
        }
        String[] names = {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"};
        double[] v = new double[6];
        for (int i = 0; i < 6; i++) {
            v[i] = answer.getClass().getField(names[i]).getDouble(answer);
        }
        double average = (v[3] - v[0] + v[4] - v[1] + v[5] - v[2]) / 3.0;
        return average >= 1.0 ? "1" : "0";
    }

    private static String reason(Throwable failed) {
        Throwable at = failed;
        while (at instanceof InvocationTargetException && at.getCause() != null) {
            at = at.getCause();
        }
        StackTraceElement[] where = at.getStackTrace();
        return at.getClass().getSimpleName() + (where.length > 0 ? "@" + where[0].getClassName().substring(where[0].getClassName().lastIndexOf('.') + 1) + "." + where[0].getMethodName() : "");
    }

    private static void dump(String output) throws Exception {
        PrintStream real = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        Method bootstrap = type("net.minecraft.server.Bootstrap").getMethod("bootStrap");
        bootstrap.invoke(null);
        Class<?> blockClass = type("net.minecraft.world.level.block.Block");
        Class<?> posClass = type("net.minecraft.core.BlockPos");
        origin = posClass.getConstructor(int.class, int.class, int.class).newInstance(X, Y, Z);
        prepareLevel();
        air = method(section.getClass(), "getBlockState", 3).invoke(section, 0, 0, 0);
        Object registry;
        Field held = field(blockClass, "REGISTRY");
        registry = held != null ? held.get(null) : type("net.minecraft.core.Registry").getField("BLOCK").get(null);
        Method keyOf = null;
        for (Method candidate : registry.getClass().getMethods()) {
            if (candidate.getName().equals("getKey") && candidate.getParameterCount() == 1) {
                keyOf = candidate;
            }
        }
        keyOf.setAccessible(true);
        Method idOf = null;
        for (Method candidate : blockClass.getMethods()) {
            if (candidate.getName().equals("getId") && candidate.getParameterCount() == 1 && candidate.getParameterTypes()[0].getSimpleName().equals("BlockState")) {
                idOf = candidate;
            }
        }
        Class<?> aabb = type("net.minecraft.world.phys.AABB");
        Object query = aabb.getConstructor(double.class, double.class, double.class, double.class, double.class, double.class).newInstance(X - 1.0, Y - 1.0, Z - 1.0, X + 2.0, Y + 3.0, Z + 2.0);
        Class<?> propertyClass = type("net.minecraft.world.level.block.state.properties.Property");
        Method propertyName = null;
        Method valueName = null;
        for (Method candidate : propertyClass.getMethods()) {
            if (candidate.getName().equals("getName")) {
                if (candidate.getParameterCount() == 0) {
                    propertyName = candidate;
                } else {
                    valueName = candidate;
                }
            }
        }
        try (BufferedWriter out = Files.newBufferedWriter(Path.of(output), StandardCharsets.UTF_8)) {
            for (Object block : (Iterable<?>) registry) {
                String name = keyOf.invoke(registry, block).toString().replaceFirst("^minecraft:", "");
                Object definition = method(block.getClass(), "getStateDefinition", 0).invoke(block);
                Collection<?> properties = (Collection<?>) method(definition.getClass(), "getProperties", 0).invoke(definition);
                for (Object state : (List<?>) method(definition.getClass(), "getPossibleStates", 0).invoke(definition)) {
                    List<String> props = new ArrayList<>();
                    Method value = null;
                    for (Method candidate : state.getClass().getMethods()) {
                        if ((candidate.getName().equals("getValue") || candidate.getName().equals("get")) && candidate.getParameterCount() == 1
                            && candidate.getParameterTypes()[0] == propertyClass) {
                            value = candidate;
                        }
                    }
                    value.setAccessible(true);
                    for (Object property : properties) {
                        props.add(propertyName.invoke(property) + "=" + valueName.invoke(property, value.invoke(state, property)));
                    }
                    props.sort(null);
                    String id = idOf.invoke(null, state).toString();
                    String joined = String.join(",", props);
                    place(state);
                    boolean upperDoor = name.endsWith("_door") && joined.contains("half=upper");
                    if (upperDoor) {
                        placeBelow(method(block.getClass(), "defaultBlockState", 0).invoke(block));
                    }
                    String collision = ABSENT;
                    String outline = ABSENT;
                    try {
                        collision = collision(state, block, query);
                    } catch (ReflectiveOperationException | RuntimeException refused) {
                        out.write(String.join("\t", "refused", name, id, joined, "collision", reason(refused)));
                        out.newLine();
                    }
                    try {
                        outline = outline(state, block);
                    } catch (ReflectiveOperationException | RuntimeException refused) {
                        out.write(String.join("\t", "refused", name, id, joined, "outline", reason(refused)));
                        out.newLine();
                    }
                    String full = ABSENT;
                    try {
                        full = full(state, block);
                    } catch (ReflectiveOperationException | RuntimeException refused) {
                        out.write(String.join("\t", "refused", name, id, joined, "full", reason(refused)));
                        out.newLine();
                    }
                    out.write(String.join("\t", "state", name, id, joined, collision, outline, full));
                    out.newLine();
                    if (upperDoor) {
                        placeBelow(air);
                    }
                }
            }
        }
        System.setOut(real);
    }

    public static void main(String[] args) {
        // (Bootstrap sends System.err to the logger, which would swallow the reason a run failed)
        PrintStream err = System.err;
        try {
            dump(args[0]);
        } catch (Throwable failed) {
            failed.printStackTrace(err);
            System.exit(1);
        }
        System.exit(0);
    }
}

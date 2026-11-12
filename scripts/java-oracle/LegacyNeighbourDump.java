import java.io.BufferedWriter;
import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.reflect.Array;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.lang.reflect.Proxy;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;

/**
 * What a state's neighbours do to its geometry in a release before 1.14, from the release's own code: the fixtures of
 * test/legacy-neighbours.test.cjs. The level is LegacyShapeDump's, a blank client level with one real section, and every
 * block is put in the section the way a chunk holds it, by the stored states Block.stateById gives, so no state here has
 * a neighbour's property a world could not send.
 *
 * Nothing says beforehand which blocks depend on neighbours. Each stored state is tried with probes at the 18 cells around
 * it and the 6 two away, and what moves is what the rest runs on:
 *
 *   stored ID,ID...                                 the states a chunk can hold, the neighbours of `side`
 *   alone NAME ID C O                               each stored state with air around it
 *   shape INDEX boxes                               the distinct answers, boxes as in ShapeDump.java, cell-relative
 *   depends NAME dx,dy,dz dx,dy,dz...               a block and the cells that changed some state of it
 *   side HOME dx,dy,dz C O id:c:o,id:c:o...         HOME with every stored state at that cell: C and O are the collision
 *                                                   and outline answers, the list the neighbours that give another
 *   grid HOME dx,dy,dz=id|id;dx,dy,dz=id|id c:o,... every combination of those cells at once, the last cell turning
 *                                                   fastest: the neighbours are one of each kind `side` found, or for a
 *                                                   chain the blocks a neighbour itself reads
 *   refused NAME ID reason                          a query the level could not answer
 *   wireflags ID conn top below normal              what redstone wire reads of a state, each by the wire's outline
 *                                                   with that state at one cell: conn, top and below are masks of the
 *                                                   sides north, south, west, east, the state beside the wire alone,
 *                                                   with a wire above it, with a wire below it; normal is 1 where a
 *                                                   state above the wire keeps a wire on a glowstone beside it from
 *                                                   climbing (getConnectingSide's isNormalCube)
 *   pool ID,ID...                                   the states `combo` draws from besides every stored one
 *   combo HOME SEED dx,dy,dz;dx,dy,dz... c:o,...    random neighbours at all those cells at once: per combination, per
 *                                                   cell, xorshift32 from SEED picks air (1 in 8), any stored state (3 in
 *                                                   8) or one of the pool (4 in 8), as legacy-neighbours.cjs draws them
 *
 * IDs are Block.getId(state). An outline of `-` is a release with no such method.
 *
 * With a second argument `entities` the level holds block entities instead (test/fixtures/legacy-block-entities):
 *
 *   moving NAME ID facing extending source progress progressO collision
 *                                                   a PistonMovingBlockEntity in a piston_extension cell moving the
 *                                                   state ID: the head a piston pushes out, its base drawn back, any
 *                                                   other block either way, the progress pairs its tick gives (0 0,
 *                                                   0.5 0, 1 0.5). Before 1.9 the moved block's collision is its
 *                                                   bounds as last set, here by its own setBoundingBox alone elsewhere
 *   shulker NAME ID progress progressO collision outline
 *                                                   a ShulkerBoxBlockEntity with that progress, from 1.11
 *
 * java -XX:+UnlockDiagnosticVMOptions -XX:-BytecodeVerificationRemote -cp "out;JARS" LegacyNeighbourDump OUTPUT_FILE [entities]
 */
public final class LegacyNeighbourDump {
    private static final int X = 8;
    private static final int Y = 72;
    private static final int Z = 8;
    private static final int[][] CELLS = {
        {0, -1, 0}, {0, 1, 0}, {0, 0, -1}, {0, 0, 1}, {-1, 0, 0}, {1, 0, 0},
        {-1, 0, -1}, {-1, 0, 1}, {1, 0, -1}, {1, 0, 1}, {-1, -1, 0}, {1, -1, 0}, {0, -1, -1}, {0, -1, 1}, {-1, 1, 0}, {1, 1, 0}, {0, 1, -1}, {0, 1, 1},
        {-2, 0, 0}, {2, 0, 0}, {0, -2, 0}, {0, 2, 0}, {0, 0, -2}, {0, 0, 2},
    };
    private static final int LARGEST_GRID = 400000;
    private static final int COMBOS = 1500;
    private static int seed;
    private static Object chunk;
    private static Object origin;
    private static Object level;
    private static Object section;
    private static Object air;
    private static Object query;
    private static Method setBlockState;
    private static Method idOf;
    private static Method outlineShape;
    private static Method collisionShape;
    private static final Map<String, Integer> SHAPES = new LinkedHashMap<>();
    private static final Map<Class<?>, Method> APPEND = new HashMap<>();
    private static BufferedWriter out;

    private record State(int id, String name, Object block, Object state) { }

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
                if (candidate.getName().equals(name) && candidate.getParameterCount() == parameters && !Modifier.isStatic(candidate.getModifiers())) {
                    candidate.setAccessible(true);
                    return candidate;
                }
            }
        }
        for (Method candidate : owner.getMethods()) {
            if (candidate.getName().equals(name) && candidate.getParameterCount() == parameters && !Modifier.isStatic(candidate.getModifiers())) {
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

    private static Object blankOf(Class<?> owner) throws Exception {
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        return ((sun.misc.Unsafe) theUnsafe.get(null)).allocateInstance(owner);
    }

    /** As LegacyShapeDump.prepareLevel. */
    private static void prepareLevel() throws Exception {
        Class<?> sectionClass = type("net.minecraft.world.level.chunk.LevelChunkSection");
        Constructor<?> make = sectionClass.getConstructor(int.class, boolean.class);
        section = make.newInstance(Y & ~15, true);
        Class<?> chunkClass = type("net.minecraft.world.level.chunk.LevelChunk");
        chunk = blankOf(chunkClass);
        Object sections = Array.newInstance(sectionClass, 16);
        Array.set(sections, Y >> 4, section);
        field(chunkClass, "sections").set(chunk, sections);
        Class<?> sourceClass = type("net.minecraft.world.level.chunk.ChunkSource");
        Object source = Proxy.newProxyInstance(sourceClass.getClassLoader(), new Class<?>[] {sourceClass}, (proxy, called, arguments) -> {
            Class<?> result = called.getReturnType();
            if (result == boolean.class) {
                return true;
            }
            return result.isInstance(chunk) ? chunk : null;
        });
        Class<?> levelClass = type("net.minecraft.client.multiplayer.MultiPlayerLevel", "net.minecraft.client.multiplayer.ClientLevel");
        level = blankOf(levelClass);
        field(levelClass, "chunkSource").set(level, source);
        Field client = field(levelClass, "isClientSide");
        if (client != null) {
            client.setBoolean(level, true);
        }
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
                Field normal = field(levelType, "NORMAL");
                if (normal != null) {
                    generator.set(blank, normal.get(null));
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

    private static void put(int dx, int dy, int dz, Object state) throws Exception {
        setBlockState.invoke(section, (X + dx) & 15, (Y + dy) & 15, (Z + dz) & 15, state);
    }

    /** `absolute`: appendCollisionBoxes hands boxes in the world, a shape or a block's box is its cell's. */
    private static int shapeOf(Iterable<?> list, boolean absolute) throws Exception {
        StringBuilder text = new StringBuilder();
        double[] cell = {X, Y, Z};
        String[] names = {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"};
        for (Object box : list) {
            if (text.length() > 0) {
                text.append(';');
            }
            for (int i = 0; i < 6; i++) {
                text.append(i == 0 ? "" : ",").append(box.getClass().getField(names[i]).getDouble(box) - (absolute ? cell[i % 3] : 0));
            }
        }
        return remember(text.toString());
    }

    private static int remember(String text) throws Exception {
        Integer known = SHAPES.get(text);
        if (known == null) {
            known = SHAPES.size();
            SHAPES.put(text, known);
            out.write("shape\t" + known + "\t" + text);
            out.newLine();
        }
        return known;
    }

    private static Object[] arguments(Method found, Object state, List<Object> list) {
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

    private static Method appendOf(Class<?> block) {
        return APPEND.computeIfAbsent(block, key -> {
            for (Class<?> at = key; at != null; at = at.getSuperclass()) {
                for (Method candidate : at.getDeclaredMethods()) {
                    if (candidate.getName().equals("appendCollisionBoxes") && candidate.getParameterCount() >= 6) {
                        candidate.setAccessible(true);
                        return candidate;
                    }
                }
            }
            return null;
        });
    }

    /** Throws what the block threw: a state the level cannot answer. */
    private static int collision(State home) throws Exception {
        if (collisionShape != null) {
            Object answer = collisionShape.invoke(home.state, level, origin);
            return shapeOf((List<?>) method(answer.getClass(), "toAabbs", 0).invoke(answer), false);
        }
        List<Object> list = new ArrayList<>();
        Method found = appendOf(home.block.getClass());
        found.invoke(home.block, arguments(found, home.state, list));
        return shapeOf(list, true);
    }

    /**
     * The box a ray meets. A stair's ray walks its collision boxes (1.12.2 StairBlock.java:288), and a liquid is met as a fluid, so
     * neither has an outline to compare. 1.8 has no getter: the block's bounds as setBoundingBox leaves them, put back after,
     * for a block whose collision reads the bounds it was last given.
     */
    private static int outline(State home) throws Exception {
        if (home.name.endsWith("_stairs") || home.name.equals("water") || home.name.equals("lava") || home.name.startsWith("flowing_")) {
            return remember("-");
        }
        Method bounds = method(home.block.getClass(), "setBoundingBox", 2);
        if (bounds != null) {
            String[] sides = {"boundingBoxMinX", "boundingBoxMinY", "boundingBoxMinZ", "boundingBoxMaxX", "boundingBoxMaxY", "boundingBoxMaxZ"};
            double[] kept = new double[6];
            for (int i = 0; i < 6; i++) {
                kept[i] = field(home.block.getClass(), sides[i]).getDouble(home.block);
            }
            bounds.invoke(home.block, level, origin);
            StringBuilder text = new StringBuilder();
            for (int i = 0; i < 6; i++) {
                Field side = field(home.block.getClass(), sides[i]);
                text.append(i == 0 ? "" : ",").append(side.getDouble(home.block));
                side.setDouble(home.block, kept[i]);
            }
            return remember(text.toString());
        }
        if (outlineShape != null) {
            Object answer = outlineShape.invoke(home.state, level, origin);
            return shapeOf((List<?>) method(answer.getClass(), "toAabbs", 0).invoke(answer), false);
        }
        Method onBlock = method(home.block.getClass(), "getShape", 3);
        if (onBlock == null) {
            return remember("-");
        }
        return shapeOf(List.of(onBlock.invoke(home.block, arguments(onBlock, home.state, null))), false);
    }

    private static String cell(int[] at) {
        return at[0] + "," + at[1] + "," + at[2];
    }

    /** `collision:outline` of a state in the level as it stands; null for one it refused. */
    private static String answer(State home) throws Exception {
        try {
            return collision(home) + ":" + outline(home);
        } catch (ReflectiveOperationException | RuntimeException refused) {
            return null;
        }
    }

    private static String reason(Throwable failed) {
        Throwable at = failed;
        while (at instanceof InvocationTargetException && at.getCause() != null) {
            at = at.getCause();
        }
        return at.getClass().getSimpleName();
    }

    private static List<State> states() throws Exception {
        Class<?> blockClass = type("net.minecraft.world.level.block.Block");
        Field held = field(blockClass, "REGISTRY");
        Object registry = held != null ? held.get(null) : type("net.minecraft.core.Registry").getField("BLOCK").get(null);
        Method keyOf = null;
        for (Method candidate : registry.getClass().getMethods()) {
            if (candidate.getName().equals("getKey") && candidate.getParameterCount() == 1) {
                keyOf = candidate;
            }
        }
        keyOf.setAccessible(true);
        Method stored = null;
        for (Method candidate : blockClass.getMethods()) {
            if (candidate.getName().equals("getId") && candidate.getParameterCount() == 1 && candidate.getParameterTypes()[0].getSimpleName().equals("BlockState")) {
                idOf = candidate;
            }
            if (candidate.getName().equals("stateById") && candidate.getParameterCount() == 1) {
                stored = candidate;
            }
        }
        if (stored == null) {
            throw new IllegalStateException("Block.stateById is not in this jar");
        }
        Map<Integer, State> byId = new TreeMap<>();
        for (Object block : (Iterable<?>) registry) {
            String name = keyOf.invoke(registry, block).toString().replaceFirst("^minecraft:", "");
            Object definition = method(block.getClass(), "getStateDefinition", 0).invoke(block);
            for (Object state : (List<?>) method(definition.getClass(), "getPossibleStates", 0).invoke(definition)) {
                int id = (Integer) idOf.invoke(null, state);
                if (!byId.containsKey(id)) {
                    byId.put(id, new State(id, name, block, stored.invoke(null, id)));
                }
            }
        }
        return new ArrayList<>(byId.values());
    }

    /** The cells that change each block: every state among a few kinds and its own, the first of each among every block. */
    private static Map<String, Set<Integer>> discover(List<State> all) throws Exception {
        Map<String, List<State>> byName = new LinkedHashMap<>();
        for (State state : all) {
            byName.computeIfAbsent(state.name, key -> new ArrayList<>()).add(state);
        }
        List<State> kinds = new ArrayList<>();
        for (List<State> block : byName.values()) {
            kinds.add(block.get(0));
        }
        List<State> few = new ArrayList<>();
        for (State probe : all) {
            if (Set.of("stone", "glass", "fence", "oak_fence", "oak_stairs", "chest", "cobblestone_wall", "end_stone").contains(probe.name)) {
                few.add(probe);
            }
        }
        Map<String, Set<Integer>> found = new LinkedHashMap<>();
        for (State home : all) {
            put(0, 0, 0, home.state);
            String alone = answer(home);
            if (alone == null) {
                try {
                    collision(home);
                    outline(home);
                } catch (ReflectiveOperationException | RuntimeException refused) {
                    out.write(String.join("\t", "refused", home.name, String.valueOf(home.id), reason(refused)));
                    out.newLine();
                }
                continue;
            }
            List<State> probes = new ArrayList<>(few);
            // (a flattened block's own states run to 1296, of which any 64 say whether it reads them)
            List<State> own = byName.get(home.name);
            for (int i = 0; i < own.size(); i += Math.max(1, own.size() / 64)) {
                probes.add(own.get(i));
            }
            if (home == byName.get(home.name).get(0)) {
                probes.addAll(kinds);
            }
            for (int at = 0; at < CELLS.length; at++) {
                for (State probe : probes) {
                    put(CELLS[at][0], CELLS[at][1], CELLS[at][2], probe.state);
                    String near = answer(home);
                    if (!alone.equals(near)) {
                        found.computeIfAbsent(home.name, key -> new TreeSet<>()).add(at);
                    }
                }
                put(CELLS[at][0], CELLS[at][1], CELLS[at][2], air);
            }
        }
        put(0, 0, 0, air);
        return found;
    }

    private static void grid(State home, int[][] cells, List<List<State>> palettes) throws Exception {
        long size = 1;
        for (List<State> palette : palettes) {
            size *= palette.size();
        }
        if (size > LARGEST_GRID) {
            out.write(String.join("\t", "toolarge", home.name, String.valueOf(home.id), String.valueOf(size)));
            out.newLine();
            return;
        }
        put(0, 0, 0, home.state);
        StringBuilder head = new StringBuilder();
        for (int i = 0; i < cells.length; i++) {
            head.append(i == 0 ? "" : ";").append(cell(cells[i])).append('=');
            for (int j = 0; j < palettes.get(i).size(); j++) {
                head.append(j == 0 ? "" : "|").append(palettes.get(i).get(j).id);
            }
        }
        StringBuilder rows = new StringBuilder();
        int[] turn = new int[cells.length];
        for (long row = 0; row < size; row++) {
            for (int i = 0; i < cells.length; i++) {
                put(cells[i][0], cells[i][1], cells[i][2], palettes.get(i).get(turn[i]).state);
            }
            String near = answer(home);
            rows.append(row == 0 ? "" : ",").append(near == null ? "x" : near);
            for (int i = cells.length - 1; i >= 0; i--) {
                if (++turn[i] < palettes.get(i).size()) {
                    break;
                }
                turn[i] = 0;
            }
        }
        for (int[] at : cells) {
            put(at[0], at[1], at[2], air);
        }
        put(0, 0, 0, air);
        out.write(String.join("\t", "grid", String.valueOf(home.id), head.toString(), rows.toString()));
        out.newLine();
    }

    /** One neighbour at a time, every stored state; then the cells of the block together, over one neighbour of each kind found. */
    private static void family(String name, Set<Integer> reach, List<State> all) throws Exception {
        List<State> homes = new ArrayList<>();
        for (State state : all) {
            if (state.name.equals(name)) {
                homes.add(state);
            }
        }
        Map<Integer, Map<State, StringBuilder>> signature = new LinkedHashMap<>();
        for (int at : reach) {
            signature.put(at, new LinkedHashMap<>());
        }
        for (State home : homes) {
            put(0, 0, 0, home.state);
            String alone = answer(home);
            if (alone == null) {
                continue;
            }
            for (int at : reach) {
                int[] c = CELLS[at];
                StringBuilder differ = new StringBuilder();
                for (State neighbour : all) {
                    put(c[0], c[1], c[2], neighbour.state);
                    String near = answer(home);
                    signature.get(at).computeIfAbsent(neighbour, key -> new StringBuilder()).append(near).append(' ');
                    if (!alone.equals(near)) {
                        differ.append(differ.length() == 0 ? "" : ",").append(neighbour.id).append(':').append(near == null ? "x" : near);
                    }
                }
                put(c[0], c[1], c[2], air);
                String[] answers = alone.split(":");
                out.write(String.join("\t", "side", String.valueOf(home.id), cell(c), answers[0], answers[1], differ.toString()));
                out.newLine();
            }
        }
        put(0, 0, 0, air);
        if (reach.size() < 2) {
            return;
        }
        List<Integer> order = new ArrayList<>(reach);
        int[][] cells = new int[order.size()][];
        List<List<State>> palettes = new ArrayList<>();
        for (int i = 0; i < cells.length; i++) {
            cells[i] = CELLS[order.get(i)];
            Map<String, State> kinds = new LinkedHashMap<>();
            for (Map.Entry<State, StringBuilder> entry : signature.get(order.get(i)).entrySet()) {
                kinds.putIfAbsent(entry.getValue().toString(), entry.getKey());
            }
            palettes.add(new ArrayList<>(kinds.values()));
        }
        for (State home : homes) {
            grid(home, cells, palettes);
        }
    }

    /** A block that reads a stair's face reads what the stair reads: every way round a stair at each side of it. */
    private static void chains(List<State> all, Map<String, Set<Integer>> found) throws Exception {
        List<State> stairs = new ArrayList<>();
        Map<String, State> first = new LinkedHashMap<>();
        for (State state : all) {
            if (state.name.equals("oak_stairs")) {
                stairs.add(state);
            } else if (found.containsKey(state.name) && !state.name.endsWith("_stairs") && !state.name.endsWith("_door") && !state.name.equals("chorus_plant")) {
                first.putIfAbsent(state.name, state);
            }
        }
        if (stairs.isEmpty()) {
            return;
        }
        List<State> withAir = new ArrayList<>();
        withAir.add(all.get(0));
        withAir.addAll(stairs);
        for (State home : first.values()) {
            for (int side = 2; side < 6; side++) {
                int[] c = CELLS[side];
                List<int[]> cells = new ArrayList<>();
                cells.add(c);
                for (int around = 2; around < 6; around++) {
                    int[] step = CELLS[around];
                    if (step[0] != -c[0] || step[2] != -c[2]) {
                        cells.add(new int[] {c[0] + step[0], 0, c[2] + step[2]});
                    }
                }
                grid(home, cells.toArray(new int[0][]), List.of(stairs, withAir, withAir, withAir));
            }
        }
    }

    private static int next() {
        seed ^= seed << 13;
        seed ^= seed >>> 17;
        seed ^= seed << 5;
        return seed;
    }

    private static int pick(int count) {
        return Integer.remainderUnsigned(next(), count);
    }

    private static State named(List<State> all, String name) {
        for (State state : all) {
            if (state.name.equals(name)) {
                return state;
            }
        }
        return null;
    }

    /** What getConnectingSide reads of each stored state, each at one cell beside a wire alone (1.12.2 RedStoneWireBlock.java:144-165). */
    private static void wireFlags(List<State> all, Map<String, Set<Integer>> found) throws Exception {
        State wire = named(all, "redstone_wire");
        State glowstone = named(all, "glowstone");
        if (wire == null || glowstone == null || !found.containsKey("redstone_wire")) {
            return;
        }
        put(0, 0, 0, wire.state);
        String alone = answer(wire);
        int[][] sides = {CELLS[2], CELLS[3], CELLS[4], CELLS[5]};
        for (State state : all) {
            int conn = 0;
            int top = 0;
            int below = 0;
            for (int d = 0; d < 4; d++) {
                int[] c = sides[d];
                put(c[0], 0, c[2], state.state);
                conn |= alone.equals(answer(wire)) ? 0 : 1 << d;
                put(c[0], 1, c[2], wire.state);
                top |= alone.equals(answer(wire)) ? 0 : 1 << d;
                put(c[0], 1, c[2], air);
                put(c[0], -1, c[2], wire.state);
                below |= alone.equals(answer(wire)) ? 0 : 1 << d;
                put(c[0], -1, c[2], air);
                put(c[0], 0, c[2], air);
            }
            put(0, 1, 0, state.state);
            put(sides[0][0], 0, sides[0][2], glowstone.state);
            put(sides[0][0], 1, sides[0][2], wire.state);
            int normal = alone.equals(answer(wire)) ? 1 : 0;
            put(0, 1, 0, air);
            put(sides[0][0], 0, sides[0][2], air);
            put(sides[0][0], 1, sides[0][2], air);
            out.write(String.join("\t", "wireflags", String.valueOf(state.id), String.valueOf(conn), String.valueOf(top), String.valueOf(below), String.valueOf(normal)));
            out.newLine();
        }
        put(0, 0, 0, air);
    }

    /**
     * Each block that reads its neighbours among random neighbours at every cell it reads at once, any stored state at each,
     * not one of each kind as `grid` has them: the wire with the cells above too, which a climb reads, and a block that reads
     * the four sides with the ring two away, which a stair beside it reads.
     */
    private static void combos(List<State> all, Map<String, Set<Integer>> found) throws Exception {
        List<State> pool = new ArrayList<>();
        Set<String> names = new TreeSet<>(found.keySet());
        names.addAll(List.of("air", "stone", "glass", "glowstone", "redstone_wire", "unpowered_repeater", "powered_repeater", "observer", "lever",
            "redstone_torch", "unlit_redstone_torch", "stone_button", "wooden_button", "redstone_block", "stone_slab", "wooden_slab", "snow_layer",
            "leaves", "slime", "piston", "sticky_piston", "piston_head", "carpet", "trapdoor", "daylight_detector", "wooden_pressure_plate",
            "tripwire_hook", "end_stone", "chorus_flower", "sea_lantern", "hopper", "soul_sand", "farmland", "ice", "detector_rail"));
        for (State state : all) {
            if (names.contains(state.name)) {
                pool.add(state);
            }
        }
        StringBuilder ids = new StringBuilder();
        for (State state : pool) {
            ids.append(ids.length() == 0 ? "" : ",").append(state.id);
        }
        out.write("pool\t" + ids);
        out.newLine();
        Set<Integer> sides = Set.of(2, 3, 4, 5);
        for (Map.Entry<String, Set<Integer>> entry : found.entrySet()) {
            Set<Integer> reach = new TreeSet<>(entry.getValue());
            if (entry.getKey().equals("redstone_wire")) {
                reach.addAll(List.of(1, 14, 15, 16, 17));
            }
            if (sides.containsAll(reach)) {
                reach.addAll(List.of(6, 7, 8, 9, 18, 19, 22, 23));
            }
            List<State> homes = new ArrayList<>();
            for (State state : all) {
                if (state.name.equals(entry.getKey())) {
                    homes.add(state);
                }
            }
            int count = Math.max(40, COMBOS / homes.size());
            StringBuilder head = new StringBuilder();
            int[][] cells = new int[reach.size()][];
            int at = 0;
            for (int index : reach) {
                cells[at++] = CELLS[index];
                head.append(head.length() == 0 ? "" : ";").append(cell(CELLS[index]));
            }
            for (State home : homes) {
                int start = (home.id * 31 + entry.getKey().length()) | 1;
                seed = start;
                StringBuilder rows = new StringBuilder();
                put(0, 0, 0, home.state);
                for (int row = 0; row < count; row++) {
                    for (int[] c : cells) {
                        int mode = pick(8);
                        State drawn = mode == 0 ? all.get(0) : mode < 4 ? all.get(pick(all.size())) : pool.get(pick(pool.size()));
                        put(c[0], c[1], c[2], drawn.state);
                    }
                    String near = answer(home);
                    rows.append(row == 0 ? "" : ",").append(near == null ? "x" : near);
                }
                for (int[] c : cells) {
                    put(c[0], c[1], c[2], air);
                }
                put(0, 0, 0, air);
                out.write(String.join("\t", "combo", String.valueOf(home.id), String.valueOf(start), head.toString(), rows.toString()));
                out.newLine();
            }
        }
    }

    /** A block entity of the class, made by its constructor of those arguments, at the dump's cell of this level. */
    private static Object blockEntity(Class<?> kind, Object... arguments) throws Exception {
        Object made = null;
        for (Constructor<?> maker : kind.getConstructors()) {
            if (maker.getParameterCount() == arguments.length) {
                made = maker.newInstance(arguments);
            }
        }
        Field at = field(kind, "worldPosition");
        if (at != null) {
            at.set(made, origin);
        }
        Field in = field(kind, "level");
        if (in != null) {
            in.set(made, level);
        }
        return made;
    }

    /** Before 1.9 a block's collision box is its bounds as last set: set them as its state alone sets them, at a cell of its own. */
    private static void boundsOf(State moved) throws Exception {
        Method bounds = method(moved.block.getClass(), "setBoundingBox", 2);
        if (collisionShape != null || bounds == null) {
            return;
        }
        put(4, 0, 4, moved.state);
        bounds.invoke(moved.block, level, origin.getClass().getConstructor(int.class, int.class, int.class).newInstance(X + 4, Y, Z + 4));
        put(4, 0, 4, air);
    }

    /** A piston_extension holding each moved state, and each shulker box opening and closing, with their block entities in the chunk. */
    private static void entities(List<State> all) throws Exception {
        // (the boxes a moving block's pushed block adds are those that meet the query moved back by the progress: a query wider
        // than any box, as legacy-block-entities.cjs asks the engine, so that only the block decides)
        query = query.getClass().getConstructor(double.class, double.class, double.class, double.class, double.class, double.class)
            .newInstance(X - 2.0, Y - 2.0, Z - 2.0, X + 3.0, Y + 3.0, Z + 3.0);
        Map<Object, Object> held = new HashMap<>();
        field(chunk.getClass(), "blockEntities").set(chunk, held);
        State cell = named(all, "piston_extension");
        if (cell == null) {
            cell = named(all, "moving_piston");
        }
        Class<?> pistonEntity = type("net.minecraft.world.level.block.piston.PistonMovingBlockEntity");
        Object[] facings = type("net.minecraft.core.Direction").getEnumConstants();
        Set<String> moved = Set.of("stone", "glass", "stone_slab", "wooden_slab", "fence", "oak_fence", "oak_stairs", "glass_pane", "cobblestone_wall",
            "snow_layer", "snow", "soul_sand", "carpet", "white_carpet", "slime", "slime_block", "piston", "sticky_piston", "piston_head");
        float[][] progress = {{0.0F, 0.0F}, {0.5F, 0.0F}, {1.0F, 0.5F}};
        Map<String, List<State>> byName = new LinkedHashMap<>();
        for (State state : all) {
            if (moved.contains(state.name)) {
                byName.computeIfAbsent(state.name, key -> new ArrayList<>()).add(state);
            }
        }
        for (List<State> states : byName.values()) {
            for (int i = 0; i < states.size(); i += Math.max(1, states.size() / 12)) {
                State state = states.get(i);
                String text = state.state.toString();
                boolean base = state.name.endsWith("piston");
                boolean head = state.name.equals("piston_head");
                for (Object facing : facings) {
                    boolean faces = text.contains("facing=" + facing.toString().toLowerCase(java.util.Locale.ROOT));
                    for (boolean extending : new boolean[] {true, false}) {
                        for (boolean source : new boolean[] {false, true}) {
                            // (what PistonBaseBlock makes: its head going out, itself unextended coming back, each of its facing; any block
                            // it pushes or pulls, a piston too, as no source)
                            boolean real = source ? faces && (head && extending || base && !extending && text.contains("extended=false")) : !head;
                            if (!real) {
                                continue;
                            }
                            for (float[] pair : progress) {
                                Object entity = blockEntity(pistonEntity, state.state, facing, extending, source);
                                field(pistonEntity, "progress").setFloat(entity, pair[0]);
                                field(pistonEntity, "progressO").setFloat(entity, pair[1]);
                                held.clear();
                                held.put(origin, entity);
                                put(0, 0, 0, cell.state);
                                boundsOf(state);
                                String answer;
                                try {
                                    answer = String.valueOf(collision(cell));
                                } catch (ReflectiveOperationException | RuntimeException refused) {
                                    answer = "x" + reason(refused);
                                }
                                out.write(String.join("\t", "moving", state.name, String.valueOf(state.id), String.valueOf(((Enum<?>) facing).ordinal()),
                                    extending ? "1" : "0", source ? "1" : "0", String.valueOf(pair[0]), String.valueOf(pair[1]), answer));
                                out.newLine();
                            }
                        }
                    }
                }
            }
        }
        Class<?> shulkerEntity = type("net.minecraft.world.level.block.entity.ShulkerBoxBlockEntity");
        if (shulkerEntity != null) {
            for (State state : all) {
                if (!state.name.endsWith("shulker_box") || !(state.name.startsWith("white") || state.name.equals("shulker_box") || state.name.startsWith("purple"))) {
                    continue;
                }
                // (updateAnimation: the progress goes up a tenth as a float each tick while it opens, down while it closes)
                for (boolean opening : new boolean[] {true, false}) {
                    float now = opening ? 0.0F : 1.0F;
                    for (int step = 1; step <= 10; step++) {
                        float before = now;
                        now = opening ? now + 0.1F : now - 0.1F;
                        if (opening && now >= 1.0F) {
                            now = 1.0F;
                        }
                        if (!opening && now <= 0.0F) {
                            now = 0.0F;
                        }
                        Object entity = blockEntity(shulkerEntity);
                        field(shulkerEntity, "progress").setFloat(entity, now);
                        field(shulkerEntity, "progressOld").setFloat(entity, before);
                        held.clear();
                        held.put(origin, entity);
                        put(0, 0, 0, state.state);
                        String[] answers = answer(state).split(":");
                        out.write(String.join("\t", "shulker", state.name, String.valueOf(state.id), opening ? "opening" : "closing", String.valueOf(step),
                            String.valueOf(now), String.valueOf(before), answers[0], answers[1]));
                        out.newLine();
                    }
                }
            }
        }
        held.clear();
        put(0, 0, 0, air);
    }

    private static void dump(String output, boolean entities) throws Exception {
        PrintStream real = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        type("net.minecraft.server.Bootstrap").getMethod("bootStrap").invoke(null);
        Class<?> posClass = type("net.minecraft.core.BlockPos");
        origin = posClass.getConstructor(int.class, int.class, int.class).newInstance(X, Y, Z);
        prepareLevel();
        setBlockState = method(section.getClass(), "setBlockState", 4);
        air = method(section.getClass(), "getBlockState", 3).invoke(section, 0, 0, 0);
        Class<?> aabb = type("net.minecraft.world.phys.AABB");
        query = aabb.getConstructor(double.class, double.class, double.class, double.class, double.class, double.class)
            .newInstance(X - 1.0, Y - 1.0, Z - 1.0, X + 2.0, Y + 3.0, Z + 2.0);
        List<State> all = states();
        Method shape = method(all.get(1).state.getClass(), "getCollisionShape", 2);
        if (shape != null && shape.getReturnType().getSimpleName().equals("VoxelShape")) {
            collisionShape = shape;
        }
        Method onState = method(all.get(1).state.getClass(), "getShape", 2);
        if (onState != null && onState.getReturnType().getSimpleName().equals("VoxelShape")) {
            outlineShape = onState;
        }
        try (BufferedWriter writer = Files.newBufferedWriter(Path.of(output), StandardCharsets.UTF_8)) {
            out = writer;
            if (entities) {
                entities(all);
                System.setOut(real);
                return;
            }
            StringBuilder ids = new StringBuilder();
            for (State state : all) {
                ids.append(ids.length() == 0 ? "" : ",").append(state.id);
            }
            out.write("stored\t" + ids);
            out.newLine();
            for (State state : all) {
                put(0, 0, 0, state.state);
                String alone = answer(state);
                if (alone != null) {
                    String[] answers = alone.split(":");
                    out.write(String.join("\t", "alone", state.name, String.valueOf(state.id), answers[0], answers[1]));
                    out.newLine();
                }
            }
            put(0, 0, 0, air);
            Map<String, Set<Integer>> found = discover(all);
            for (Map.Entry<String, Set<Integer>> entry : found.entrySet()) {
                StringBuilder cells = new StringBuilder();
                for (int at : entry.getValue()) {
                    cells.append(cells.length() == 0 ? "" : " ").append(cell(CELLS[at]));
                }
                out.write("depends\t" + entry.getKey() + "\t" + cells);
                out.newLine();
            }
            for (Map.Entry<String, Set<Integer>> entry : found.entrySet()) {
                family(entry.getKey(), entry.getValue(), all);
            }
            chains(all, found);
            wireFlags(all, found);
            combos(all, found);
        }
        System.setOut(real);
    }

    public static void main(String[] args) {
        PrintStream err = System.err;
        try {
            dump(args[0], args.length > 1 && args[1].equals("entities"));
        } catch (Throwable failed) {
            failed.printStackTrace(err);
            System.exit(1);
        }
        System.exit(0);
    }
}

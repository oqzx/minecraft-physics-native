import java.io.BufferedWriter;
import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.function.BiPredicate;
import java.util.function.Predicate;

/**
 * The geometry generated/block-shapes cannot hold as boxes, because the answer needs more than the state: what ShapeDump.java
 * lists as `contextual`, asked of the jar's own code in the contexts themselves, for test/fixtures/contexts and
 * test/contextual-shapes.test.cjs. Tab separated, boxes as in ShapeDump.java:
 *
 *   position NAME INDEX x y z collision outline             every state that answers otherwise elsewhere, at several cells
 *   entity NAME INDEX y bottom descending fall boots fluid collision outline
 *                                                           scaffolding, powder snow, a light block and the liquids, in an
 *                                                           EntityCollisionContext of those fields: a zombie's, with or
 *                                                           without leather boots, its feet at bottom. `fluid` 1 is
 *                                                           alwaysCollideWithFluid, where the release has it
 *   falling NAME INDEX y bottom descending collision        powder snow under a falling block
 *   moving NAME INDEX facing extending source progress x y z collision
 *                                                           a real PistonMovingBlockEntity at that progress
 *   shulker NAME INDEX way step progress progressOld collision outline
 *                                                           a real ShulkerBoxBlockEntity in the level at each step of its
 *                                                           lid, `opening` from shut or `closing` from open
 *
 * The context is allocated and its fields set, by type: its constructor would need a level for the entity. The empty level
 * is EmptyBlockGetter, so a neighbour is air; a shulker box's is a BlockGetter of that one block entity alone.
 *
 * javac -d out scripts/java-oracle/OracleContext.java
 * java -cp "out;JARS" OracleContext OUTPUT_FILE
 */
public final class OracleContext {
    private static final int[][] CELLS = {{0, 0, 0}, {1, 64, 0}, {0, 64, 1}, {13, 64, -7}, {-1, 0, -1}, {-16, 70, 15}, {123, -5, -456}, {29999999, 64, -29999999}};
    private static final double EDGE = 9.999999747378752E-6;
    private static final double[] FEET = {
        1.5, 1.0, 1.0 - EDGE, Math.nextUp(1.0 - EDGE), Math.nextDown(1.0 - EDGE), 0.875, 0.5, 0.125, 0.0, -EDGE, Math.nextUp(-EDGE), Math.nextDown(-EDGE), -0.5,
    };
    private static final float[] FALLS = {0.0F, 2.5F, Math.nextUp(2.5F), 3.0F};
    private static final float[] PROGRESS = {0.0F, 0.25F, 0.5F, 0.75F, 1.0F};
    private static Class<?> blockGetter;
    private static Class<?> blockPos;
    private static Class<?> contextClass;
    private static Class<?> stateClass;
    private static Object getter;
    private static Object emptyContext;
    private static Object leatherBoots;
    private static Object emptyStack;
    private static BufferedWriter out;

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

    private static Method method(Class<?> owner, String name, Class<?>... parameters) {
        for (Class<?> at = owner; at != null; at = at.getSuperclass()) {
            try {
                Method found = at.getDeclaredMethod(name, parameters);
                found.setAccessible(true);
                return found;
            } catch (NoSuchMethodException absent) {
                // the superclass
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
        return unsafe().allocateInstance(owner);
    }

    /** Final instance fields of a class the JVM lets reflection write once accessible; records aside, these are plain. */
    private static void put(Object target, Field where, Object value) throws Exception {
        sun.misc.Unsafe unsafe = unsafe();
        long offset = unsafe.objectFieldOffset(where);
        Class<?> kind = where.getType();
        if (kind == boolean.class) {
            unsafe.putBoolean(target, offset, (Boolean) value);
        } else if (kind == double.class) {
            unsafe.putDouble(target, offset, ((Number) value).doubleValue());
        } else if (kind == float.class) {
            unsafe.putFloat(target, offset, ((Number) value).floatValue());
        } else if (kind == int.class) {
            unsafe.putInt(target, offset, ((Number) value).intValue());
        } else {
            unsafe.putObject(target, offset, value);
        }
    }

    private static Object context(Object entity, double bottom, boolean descending, boolean fluid) throws Exception {
        Class<?> kind = type("net.minecraft.world.phys.shapes.EntityCollisionContext");
        Object context = blankOf(kind);
        for (Field candidate : kind.getDeclaredFields()) {
            if (Modifier.isStatic(candidate.getModifiers())) {
                continue;
            }
            String name = candidate.getName();
            Class<?> held = candidate.getType();
            if (name.equals("descending") || name.equals("sneaking")) {
                put(context, candidate, descending);
            } else if (name.equals("alwaysCollideWithFluid")) {
                put(context, candidate, fluid);
            } else if (held == double.class) {
                put(context, candidate, bottom);
            } else if (held == emptyStack.getClass()) {
                put(context, candidate, emptyStack);
            } else if (held.getSimpleName().equals("Item")) {
                put(context, candidate, emptyStack.getClass().getMethod("getItem").invoke(emptyStack));
            } else if (held == Predicate.class) {
                Predicate<Object> no = ignored -> false;
                put(context, candidate, no);
            } else if (held == BiPredicate.class) {
                BiPredicate<Object, Object> no = (one, other) -> false;
                put(context, candidate, no);
            } else if (held == Optional.class) {
                put(context, candidate, Optional.ofNullable(entity));
            } else if (held.getSimpleName().equals("Entity")) {
                put(context, candidate, entity);
            }
        }
        return context;
    }

    private static Object zombie(float fall, boolean boots) throws Exception {
        Class<?> kind = type("net.minecraft.world.entity.monster.zombie.Zombie", "net.minecraft.world.entity.monster.Zombie");
        Object zombie = blankOf(kind);
        put(zombie, field(kind, "type"), type("net.minecraft.world.entity.EntityTypes", "net.minecraft.world.entity.EntityType").getField("ZOMBIE").get(null));
        Field fallen = field(kind, "fallDistance");
        put(zombie, fallen, fallen.getType() == double.class ? (Object) (double) fall : (Object) fall);
        Object stack = boots ? leatherBoots : emptyStack;
        Field equipment = field(kind, "equipment");
        if (equipment != null) {
            Object held = equipment.getType().getConstructor().newInstance();
            Class<?> slots = type("net.minecraft.world.entity.EquipmentSlot");
            Object feet = slots.getField("FEET").get(null);
            method(equipment.getType(), "set", slots, emptyStack.getClass()).invoke(held, feet, stack);
            put(zombie, equipment, held);
        } else {
            Object armor = type("net.minecraft.core.NonNullList").getMethod("withSize", int.class, Object.class).invoke(null, 4, emptyStack);
            ((List<Object>) armor).set(0, stack);
            put(zombie, field(kind, "armorItems"), armor);
        }
        return zombie;
    }

    /** With no data bound from 26.1 the constructor cannot run: only `item` and `count` are read of a stack here. */
    private static Object stack(Class<?> stacks, Class<?> itemLike, Object item) throws Exception {
        try {
            return stacks.getConstructor(itemLike).newInstance(item);
        } catch (java.lang.reflect.InvocationTargetException unbound) {
            Object made = blankOf(stacks);
            Field held = field(stacks, "item");
            put(made, held, held.getType().isInstance(item) ? item : method(item.getClass(), "builtInRegistryHolder").invoke(item));
            put(made, field(stacks, "count"), 1);
            return made;
        }
    }

    private static String boxes(Object shape) throws Exception {
        StringBuilder text = new StringBuilder();
        for (Object box : (List<?>) method(shape.getClass(), "toAabbs").invoke(shape)) {
            if (text.length() > 0) {
                text.append(';');
            }
            for (int i = 0; i < 6; i++) {
                String name = new String[] {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"}[i];
                text.append(i == 0 ? "" : ",").append(box.getClass().getField(name).getDouble(box));
            }
        }
        return text.toString();
    }

    private static Object pos(int x, int y, int z) throws Exception {
        return blockPos.getConstructor(int.class, int.class, int.class).newInstance(x, y, z);
    }

    private static Object collision(Object state, Object at, Object context) throws Exception {
        return method(state.getClass(), "getCollisionShape", blockGetter, blockPos, contextClass).invoke(state, getter, at, context);
    }

    private static Object outline(Object state, Object at, Object context) throws Exception {
        return method(state.getClass(), "getShape", blockGetter, blockPos, contextClass).invoke(state, getter, at, context);
    }

    private static void line(String... fields) throws Exception {
        out.write(String.join("\t", fields));
        out.newLine();
    }

    private static List<?> statesOf(Object block) throws Exception {
        Object definition = method(block.getClass(), "getStateDefinition").invoke(block);
        return (List<?>) method(definition.getClass(), "getPossibleStates").invoke(definition);
    }

    private static void positions(String name, List<?> states) throws Exception {
        for (int index = 0; index < states.size(); index++) {
            Object state = states.get(index);
            String collisionHere = boxes(collision(state, pos(0, 0, 0), emptyContext));
            String outlineHere = boxes(outline(state, pos(0, 0, 0), emptyContext));
            boolean moves = false;
            for (int[] cell : CELLS) {
                Object at = pos(cell[0], cell[1], cell[2]);
                moves |= !boxes(collision(state, at, emptyContext)).equals(collisionHere) || !boxes(outline(state, at, emptyContext)).equals(outlineHere);
            }
            if (!moves) {
                continue;
            }
            for (int[] cell : CELLS) {
                Object at = pos(cell[0], cell[1], cell[2]);
                line("position", name, String.valueOf(index), String.valueOf(cell[0]), String.valueOf(cell[1]), String.valueOf(cell[2]),
                    boxes(collision(state, at, emptyContext)), boxes(outline(state, at, emptyContext)));
            }
        }
    }

    private static void entities(String name, List<?> states) throws Exception {
        boolean snow = name.equals("powder_snow");
        boolean bootsMatter = snow && type("net.minecraft.world.level.block.PowderSnowBlock") != null;
        for (int index = 0; index < states.size(); index++) {
            Object state = states.get(index);
            for (int y : new int[] {64, -3}) {
                Object at = pos(0, y, 0);
                for (double feet : FEET) {
                    for (boolean descending : new boolean[] {false, true}) {
                        for (float fall : snow ? FALLS : new float[] {0.0F}) {
                            for (boolean boots : bootsMatter ? new boolean[] {false, true} : new boolean[] {false}) {
                                for (boolean fluid : field(type("net.minecraft.world.phys.shapes.EntityCollisionContext"), "alwaysCollideWithFluid") != null ? new boolean[] {false, true} : new boolean[] {false}) {
                                    Object context = context(zombie(fall, boots), y + feet, descending, fluid);
                                    line("entity", name, String.valueOf(index), String.valueOf(y), String.valueOf(y + feet), descending ? "1" : "0",
                                        String.valueOf(fall), boots ? "1" : "0", fluid ? "1" : "0", boxes(collision(state, at, context)), boxes(outline(state, at, context)));
                                }
                            }
                        }
                    }
                    if (snow) {
                        Class<?> falling = type("net.minecraft.world.entity.item.FallingBlockEntity");
                        Object entity = blankOf(falling);
                        for (boolean descending : new boolean[] {false, true}) {
                            line("falling", name, String.valueOf(index), String.valueOf(y), String.valueOf(y + feet), descending ? "1" : "0",
                                boxes(collision(state, at, context(entity, y + feet, descending, false))));
                        }
                    }
                }
            }
        }
    }

    private static Object piston(Object at, Object moved, Object facing, boolean extending, boolean source) throws Exception {
        Class<?> kind = type("net.minecraft.world.level.block.piston.PistonMovingBlockEntity");
        Class<?> direction = type("net.minecraft.core.Direction");
        Object block = type("net.minecraft.world.level.block.Blocks").getField("MOVING_PISTON").get(null);
        Object own = method(block.getClass(), "defaultBlockState").invoke(block);
        for (Constructor<?> maker : kind.getConstructors()) {
            if (maker.getParameterCount() == 6) {
                return maker.newInstance(at, own, moved, facing, extending, source);
            }
            if (maker.getParameterCount() == 4) {
                return maker.newInstance(moved, facing, extending, source);
            }
        }
        throw new IllegalStateException("no constructor of PistonMovingBlockEntity " + direction);
    }

    private static void moving(Object registry, Method keyOf, Map<String, Object> blocks) throws Exception {
        Class<?> kind = type("net.minecraft.world.level.block.piston.PistonMovingBlockEntity");
        Method shape = method(kind, "getCollisionShape", blockGetter, blockPos);
        Object[] facings = type("net.minecraft.core.Direction").getEnumConstants();
        String[] moved = {"stone", "oak_slab", "oak_fence", "oak_stairs", "piston", "sticky_piston", "piston_head"};
        for (String name : moved) {
            Object block = blocks.get(name);
            if (block == null) {
                continue;
            }
            List<?> states = statesOf(block);
            for (int index = 0; index < states.size(); index += name.startsWith("oak") || name.contains("piston") ? Math.max(1, states.size() / 12) : 1) {
                for (Object facing : facings) {
                    for (boolean extending : new boolean[] {true, false}) {
                        for (boolean source : new boolean[] {false, true}) {
                            // (what PistonBaseBlock makes: its head going out, itself coming back, each of the piston's facing; and it moves other blocks)
                            boolean base = name.endsWith("piston");
                            boolean head = name.equals("piston_head");
                            boolean real = source ? head && extending || base && !extending : !head;
                            if (!real || (base || head) && source && !states.get(index).toString().contains("facing=" + facing)) {
                                continue;
                            }
                            for (float progress : PROGRESS) {
                                for (int[] cell : new int[][] {{0, 64, 0}, {-3, 64, 5}}) {
                                    Object at = pos(cell[0], cell[1], cell[2]);
                                    Object entity = piston(at, states.get(index), facing, extending, source);
                                    put(entity, field(kind, "progress"), progress);
                                    put(entity, field(kind, "progressO"), progress);
                                    line("moving", name, String.valueOf(index), String.valueOf(((Enum<?>) facing).ordinal()), extending ? "1" : "0", source ? "1" : "0",
                                        String.valueOf(progress), String.valueOf(cell[0]), String.valueOf(cell[1]), String.valueOf(cell[2]), boxes(shape.invoke(entity, getter, at)));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /** A BlockGetter with nothing but the block entity at its cell: the interface's own defaults for the rest. */
    private static Object holding(Object at, Object entity) {
        return java.lang.reflect.Proxy.newProxyInstance(blockGetter.getClassLoader(), new Class<?>[] {blockGetter}, (proxy, called, arguments) -> {
            if (called.getName().equals("getBlockEntity") && called.getParameterCount() == 1) {
                return at.equals(arguments[0]) ? entity : null;
            }
            if (called.isDefault()) {
                return java.lang.reflect.InvocationHandler.invokeDefault(proxy, called, arguments);
            }
            if (called.getName().equals("equals")) {
                return proxy == arguments[0];
            }
            if (called.getName().equals("hashCode")) {
                return System.identityHashCode(proxy);
            }
            Class<?> result = called.getReturnType();
            if (result == int.class) {
                return called.getName().startsWith("getMin") ? -64 : 384;
            }
            return called.getName().equals("getBlockState") || called.getName().equals("getFluidState")
                ? method(getter.getClass(), called.getName(), called.getParameterTypes()).invoke(getter, arguments) : null;
        });
    }

    /** ShulkerBoxBlock.getShape reads the block entity of its cell: a real one, its progress a tenth further as a float each tick. */
    private static void shulkers(Map<String, Object> blocks) throws Exception {
        Class<?> kind = type("net.minecraft.world.level.block.entity.ShulkerBoxBlockEntity");
        for (String name : List.of("shulker_box", "white_shulker_box")) {
            Object block = blocks.get(name);
            if (block == null) {
                continue;
            }
            List<?> states = statesOf(block);
            for (int index = 0; index < states.size(); index++) {
                Object state = states.get(index);
                Object at = pos(0, 64, 0);
                for (boolean opening : new boolean[] {true, false}) {
                    float now = opening ? 0.0F : 1.0F;
                    for (int step = 1; step <= 10; step++) {
                        float before = now;
                        now = opening ? Math.min(now + 0.1F, 1.0F) : Math.max(now - 0.1F, 0.0F);
                        Object entity = null;
                        for (Constructor<?> maker : kind.getConstructors()) {
                            Class<?>[] types = maker.getParameterTypes();
                            if (types.length == 2 && types[0] == blockPos) {
                                entity = maker.newInstance(at, state);
                            } else if (types.length == 0 && entity == null) {
                                entity = maker.newInstance();
                            }
                        }
                        put(entity, field(kind, "progress"), now);
                        put(entity, field(kind, "progressOld"), before);
                        Object level = holding(at, entity);
                        Object collision = method(state.getClass(), "getCollisionShape", blockGetter, blockPos, contextClass).invoke(state, level, at, emptyContext);
                        Object outline = method(state.getClass(), "getShape", blockGetter, blockPos, contextClass).invoke(state, level, at, emptyContext);
                        line("shulker", name, String.valueOf(index), opening ? "opening" : "closing", String.valueOf(step), String.valueOf(now), String.valueOf(before),
                            boxes(collision), boxes(outline));
                    }
                }
            }
        }
    }

    private static void dump(String output) throws Exception {
        PrintStream real = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        Method detect = method(type("net.minecraft.SharedConstants"), "tryDetectVersion");
        if (detect != null) {
            detect.invoke(null);
        }
        method(type("net.minecraft.server.Bootstrap"), "bootStrap").invoke(null);
        blockGetter = type("net.minecraft.world.level.BlockGetter");
        blockPos = type("net.minecraft.core.BlockPos");
        contextClass = type("net.minecraft.world.phys.shapes.CollisionContext");
        stateClass = type("net.minecraft.world.level.block.state.BlockState");
        getter = type("net.minecraft.world.level.EmptyBlockGetter").getField("INSTANCE").get(null);
        emptyContext = method(contextClass, "empty").invoke(null);
        Class<?> stacks = type("net.minecraft.world.item.ItemStack");
        emptyStack = stacks.getField("EMPTY").get(null);
        Class<?> itemLike = type("net.minecraft.world.level.ItemLike");
        leatherBoots = stack(stacks, itemLike, type("net.minecraft.world.item.Items").getField("LEATHER_BOOTS").get(null));
        // (a wrapper of a tag no data pack bound throws before 1.18.2: no zombie is in that one)
        Field walkable = field(type("net.minecraft.tags.EntityTypeTags"), "POWDER_SNOW_WALKABLE_MOBS");
        Class<?> setTag = type("net.minecraft.tags.SetTag");
        if (walkable != null && setTag != null && field(walkable.get(null).getClass(), "tag") != null) {
            put(walkable.get(null), field(walkable.get(null).getClass(), "tag"), setTag.getMethod("empty").invoke(null));
        }
        Class<?> built = type("net.minecraft.core.registries.BuiltInRegistries");
        Object registry = (built != null ? built : type("net.minecraft.core.Registry")).getField("BLOCK").get(null);
        Method keyOf = null;
        for (Method candidate : registry.getClass().getMethods()) {
            if (candidate.getName().equals("getKey") && candidate.getParameterCount() == 1) {
                keyOf = candidate;
            }
        }
        keyOf.setAccessible(true);
        Map<String, Object> blocks = new java.util.LinkedHashMap<>();
        for (Object block : (Iterable<?>) registry) {
            blocks.put(keyOf.invoke(registry, block).toString().replaceFirst("^minecraft:", ""), block);
        }
        try (BufferedWriter writer = Files.newBufferedWriter(Path.of(output), StandardCharsets.UTF_8)) {
            out = writer;
            for (Map.Entry<String, Object> entry : blocks.entrySet()) {
                String name = entry.getKey();
                if (name.equals("moving_piston") || name.endsWith("shulker_box")) {
                    continue;
                }
                positions(name, statesOf(entry.getValue()));
                if (List.of("scaffolding", "powder_snow", "light", "water", "lava").contains(name)) {
                    entities(name, statesOf(entry.getValue()));
                }
            }
            moving(registry, keyOf, blocks);
            shulkers(blocks);
        }
        System.setOut(real);
    }

    public static void main(String[] args) {
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

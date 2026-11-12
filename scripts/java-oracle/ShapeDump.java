import java.io.BufferedWriter;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.concurrent.Callable;

/**
 * Asks a vanilla jar, by its own code, what every block state's geometry is: the oracle of scripts/generate-block-shapes.cjs.
 * Nothing of this repository goes in, so a registration, a property or a branch the source generators miss shows up
 * as a difference instead of being iterated past. Reflection lets one source run on every Mojang-named jar; a method
 * a release does not have yet prints `-`, which is not the same as empty.
 *
 * Lines (tab separated, boxes as `x0,y0,z0,x1,y1,z1` joined with `;`, empty for no box, in VoxelShape.toAabbs order):
 *   tags holders|helper|wrappers
 *   block NAME CLASS friction speedFactor jumpFactor defaultIndex
 *   state NAME INDEX ID PROPS collision outline interaction support sturdy flags context
 *
 * sturdy is the six isFaceSturdy answers in Direction order (d u n s w e). flags holds, each `0`, `1` or `-`:
 * isAir, blocksMotion, isSuffocating, hasLargeCollisionShape, isCollisionShapeFullBlock, hasDynamicShape,
 * isRedstoneConductor, and the fluid follows after a `:`. A `-` is a method the release lacks, or one that refused.
 *
 * Bootstrap loads no data pack, so a predicate that asks a tag (isFaceSturdy of leaves in 1.14, isSuffocating from
 * 26.x) would answer as if no block had any. TAGS_FILE gives the default pack's resolved tags, `tag<TAB>block,block`,
 * and they are bound where the jar's own `state.is(tag)` looks: Holder.Reference from 1.18.2, StaticTagHelper's wrappers
 * from 1.16, BlockTags' wrappers in 1.14 and 1.15. A release with none of those stops the run: `tags` on the first line
 * says which one it was.
 *
 * context says what each channel's answer depends on beyond the state, `channel:what+what` comma separated, a channel
 * left out when it depends on nothing else, so a state with an empty context is static geometry:
 *   position  the answer differs at another cell of an empty level (the offset plants);
 *   level     the method read the level (a recording BlockGetter over the empty one): a block entity, a neighbour;
 *   entity    the method asked its CollisionContext anything (a recording stand-in), or a class it runs from
 *             names EntityCollisionContext, which no stand-in can answer (powder snow).
 * This is what the method does, not whether the answer happened to move, so a query that returns the same box here
 * still counts.
 *
 * javac -cp JARS -d out scripts/java-oracle/ShapeDump.java
 * java -cp "out;JARS" ShapeDump OUTPUT_FILE TAGS_FILE
 */
public final class ShapeDump {
    private static final String ABSENT = "-";
    private static final Map<Class<?>, Boolean> ENTITY_CONTEXT = new HashMap<>();
    private static Object getter;
    private static Object origin;
    private static Object elsewhere;
    private static Object[] directions;
    private static Object emptyContext;
    private static Class<?> blockGetter;
    private static Class<?> blockPos;
    private static Class<?> direction;
    private static Class<?> contextClass;

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

    private static Object call(Object target, String name, Class<?>[] parameters, Object... arguments) throws Exception {
        Method found = method(target.getClass(), name, parameters);
        return found == null ? null : found.invoke(target, arguments);
    }

    private static String boxes(Object shape) throws Exception {
        if (shape == null) {
            return ABSENT;
        }
        StringBuilder text = new StringBuilder();
        for (Object box : (List<?>) method(shape.getClass(), "toAabbs").invoke(shape)) {
            if (text.length() > 0) {
                text.append(';');
            }
            for (int i = 0; i < 6; i++) {
                String field = new String[] {"minX", "minY", "minZ", "maxX", "maxY", "maxZ"}[i];
                text.append(i == 0 ? "" : ",").append(box.getClass().getField(field).getDouble(box));
            }
        }
        return text.toString();
    }

    private static String bit(Object value) {
        return value == null ? ABSENT : Boolean.TRUE.equals(value) ? "1" : "0";
    }

    private static Object collision(Object state, Object pos, Object context) throws Exception {
        return call(state, "getCollisionShape", new Class<?>[] {blockGetter, blockPos, contextClass}, getter, pos, context);
    }

    private static Object outline(Object state, Object pos, Object context) throws Exception {
        return call(state, "getShape", new Class<?>[] {blockGetter, blockPos, contextClass}, getter, pos, context);
    }

    private static boolean read;

    /** The empty level, saying when a method looked at it. */
    private static Object recordingGetter() {
        return Proxy.newProxyInstance(blockGetter.getClassLoader(), new Class<?>[] {blockGetter}, (proxy, called, arguments) -> {
            if (called.isDefault()) {
                return InvocationHandler.invokeDefault(proxy, called, arguments);
            }
            switch (called.getName()) {
                case "hashCode":
                    return System.identityHashCode(proxy);
                case "equals":
                    return proxy == arguments[0];
                case "toString":
                    return "ShapeDump level";
                case "getHeight":
                case "getMinBuildHeight":
                case "getMinY":
                case "getMaxBuildHeight":
                    return called.invoke(getter, arguments);
                default:
                    read = true;
                    return called.invoke(getter, arguments);
            }
        });
    }

    /** A context that says when a method asked it anything. */
    private static Object recordingContext() {
        return Proxy.newProxyInstance(contextClass.getClassLoader(), new Class<?>[] {contextClass}, (proxy, called, arguments) -> {
            switch (called.getName()) {
                case "hashCode":
                    return System.identityHashCode(proxy);
                case "equals":
                    return proxy == arguments[0];
                case "toString":
                    return "ShapeDump context";
                default:
                    read = true;
                    Class<?> result = called.getReturnType();
                    if (result == boolean.class) {
                        return false;
                    }
                    if (result == double.class) {
                        return 0.0;
                    }
                    if (result == float.class) {
                        return 0.0F;
                    }
                    if (result == int.class) {
                        return 0;
                    }
                    return null;
            }
        });
    }

    /** Whether a query reads what `run` is handed: an exception from the stand-in means it wanted more than that. */
    private static boolean reads(Callable<Object> run) {
        read = false;
        try {
            run.call();
        } catch (Exception refused) {
            return true;
        }
        return read;
    }

    /** Whether a class between the block's and Block names EntityCollisionContext. */
    private static boolean namesEntityContext(Class<?> blockType) throws Exception {
        for (Class<?> at = blockType; at != null && at.getName().startsWith("net.minecraft") && !at.getName().endsWith(".BlockBehaviour"); at = at.getSuperclass()) {
            Boolean known = ENTITY_CONTEXT.get(at);
            if (known == null) {
                try (InputStream bytes = at.getClassLoader().getResourceAsStream(at.getName().replace('.', '/') + ".class")) {
                    known = bytes != null
                        && new String(bytes.readAllBytes(), StandardCharsets.ISO_8859_1).contains("EntityCollisionContext");
                }
                ENTITY_CONTEXT.put(at, known);
            }
            if (known) {
                return true;
            }
        }
        return false;
    }

    private static String depends(String channel, boolean position, boolean level, boolean entity) {
        List<String> what = new ArrayList<>();
        if (position) {
            what.add("position");
        }
        if (level) {
            what.add("level");
        }
        if (entity) {
            what.add("entity");
        }
        return what.isEmpty() ? null : channel + ":" + String.join("+", what);
    }

    private static String contexts(Object state, Object block, String collision, String outline, String interaction, String support)
        throws Exception {
        Object level = recordingGetter();
        Object asked = recordingContext();
        boolean names = namesEntityContext(block.getClass());
        Class<?>[] withContext = {blockGetter, blockPos, contextClass};
        Class<?>[] plain = {blockGetter, blockPos};
        List<String> parts = new ArrayList<>();
        parts.add(depends("c", !boxes(collision(state, elsewhere, emptyContext)).equals(collision),
            reads(() -> call(state, "getCollisionShape", withContext, level, origin, emptyContext)),
            names || reads(() -> call(state, "getCollisionShape", withContext, getter, origin, asked))));
        parts.add(depends("o", !boxes(outline(state, elsewhere, emptyContext)).equals(outline),
            reads(() -> call(state, "getShape", withContext, level, origin, emptyContext)),
            reads(() -> call(state, "getShape", withContext, getter, origin, asked))));
        parts.add(depends("i", !boxes(call(state, "getInteractionShape", plain, getter, elsewhere)).equals(interaction),
            reads(() -> call(state, "getInteractionShape", plain, level, origin)), false));
        parts.add(depends("s", !boxes(call(state, "getBlockSupportShape", plain, getter, elsewhere)).equals(support),
            reads(() -> call(state, "getBlockSupportShape", plain, level, origin)), false));
        parts.removeIf(Objects::isNull);
        return String.join(",", parts);
    }

    private static String sturdy(Object state, Object block) throws Exception {
        StringBuilder text = new StringBuilder();
        Method onState = method(state.getClass(), "isFaceSturdy", blockGetter, blockPos, direction);
        for (Object face : directions) {
            if (onState != null) {
                text.append(bit(onState.invoke(state, getter, origin, face)));
                continue;
            }
            Method onBlock = null;
            for (Method candidate : block.getClass().getMethods()) {
                if (candidate.getName().equals("isFaceSturdy") && candidate.getParameterCount() == 4) {
                    onBlock = candidate;
                }
            }
            text.append(onBlock == null ? ABSENT : bit(onBlock.invoke(null, state, getter, origin, face)));
        }
        return text.toString();
    }

    private static Object flag(Object state, String name, Class<?>[] parameters, Object... arguments) {
        try {
            return call(state, name, parameters, arguments);
        } catch (Exception refused) {
            return null;
        }
    }

    private static Object blockRegistry() throws Exception {
        Class<?> built = type("net.minecraft.core.registries.BuiltInRegistries");
        Class<?> owner = built != null ? built : type("net.minecraft.core.Registry");
        Field field = owner.getField("BLOCK");
        return field.get(null);
    }

    private static String number(Object block, String name) throws Exception {
        Object value = call(block, name, new Class<?>[0]);
        return value == null ? ABSENT : value.toString();
    }

    private static void prepare() throws Exception {
        PrintStream out = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        Method detect = method(type("net.minecraft.SharedConstants"), "tryDetectVersion");
        if (detect != null) {
            detect.invoke(null);
        }
        method(type("net.minecraft.server.Bootstrap"), "bootStrap").invoke(null);
        System.setOut(out);
        blockGetter = type("net.minecraft.world.level.BlockGetter");
        blockPos = type("net.minecraft.core.BlockPos");
        direction = type("net.minecraft.core.Direction");
        contextClass = type("net.minecraft.world.phys.shapes.CollisionContext");
        getter = type("net.minecraft.world.level.EmptyBlockGetter").getField("INSTANCE").get(null);
        origin = blockPos.getField("ZERO").get(null);
        elsewhere = blockPos.getConstructor(int.class, int.class, int.class).newInstance(13, 64, -7);
        directions = direction.getEnumConstants();
        emptyContext = method(contextClass, "empty").invoke(null);
    }

    public static void main(String[] args) {
        // (Bootstrap sends System.err to the logger, which would swallow the reason a run failed)
        PrintStream err = System.err;
        try {
            dump(args[0], args[1]);
        } catch (Throwable failed) {
            failed.printStackTrace(err);
            System.exit(1);
        }
        System.exit(0);
    }

    /** tag id -> its blocks, the objects of this jar's registry. */
    private static Map<String, Set<Object>> members(Object registry, String tagsFile) throws Exception {
        Map<String, Object> blocks = new HashMap<>();
        Method keyOf = keyMethod(registry);
        for (Object block : (Iterable<?>) registry) {
            blocks.put(keyOf.invoke(registry, block).toString().replaceFirst("^minecraft:", ""), block);
        }
        Map<String, Set<Object>> tags = new HashMap<>();
        for (String line : Files.readAllLines(Path.of(tagsFile), StandardCharsets.UTF_8)) {
            String[] parts = line.split("\\t", -1);
            Set<Object> set = new LinkedHashSet<>();
            for (String name : parts[1].split(",")) {
                if (name.isEmpty()) {
                    continue;
                }
                Object block = blocks.get(name);
                if (block == null) {
                    throw new IllegalStateException("#" + parts[0] + " holds " + name + ", which this jar does not register");
                }
                set.add(block);
            }
            tags.put(parts[0], set);
        }
        return tags;
    }

    private static Object identifier(String id) throws Exception {
        Class<?> identifier = type("net.minecraft.resources.Identifier", "net.minecraft.resources.ResourceLocation");
        for (Method candidate : identifier.getMethods()) {
            if (candidate.getName().equals("parse") && candidate.getParameterCount() == 1) {
                return candidate.invoke(null, id);
            }
        }
        java.lang.reflect.Constructor<?> built = identifier.getDeclaredConstructor(String.class);
        built.setAccessible(true);
        return built.newInstance(id);
    }

    /** Holder.Reference, from 1.18.2. */
    private static boolean bindHolders(Object registry, Map<String, Set<Object>> tags) throws Exception {
        Class<?> tagKey = type("net.minecraft.tags.TagKey");
        Class<?> holderClass = type("net.minecraft.core.Holder$Reference");
        Method bind = holderClass == null ? null : method(holderClass, "bindTags", Collection.class);
        if (tagKey == null || bind == null) {
            return false;
        }
        Class<?> keys = type("net.minecraft.core.registries.Registries");
        Object key = keys != null ? keys.getField("BLOCK").get(null) : type("net.minecraft.core.Registry").getField("BLOCK_REGISTRY").get(null);
        Method create = method(tagKey, "create", type("net.minecraft.resources.ResourceKey"),
            type("net.minecraft.resources.Identifier", "net.minecraft.resources.ResourceLocation"));
        Map<Object, List<Object>> tagsOf = new HashMap<>();
        for (Map.Entry<String, Set<Object>> tag : tags.entrySet()) {
            Object bound = create.invoke(null, key, identifier("minecraft:" + tag.getKey()));
            for (Object block : tag.getValue()) {
                tagsOf.computeIfAbsent(block, ignored -> new ArrayList<>()).add(bound);
            }
        }
        for (Object block : (Iterable<?>) registry) {
            bind.invoke(call(block, "builtInRegistryHolder", new Class<?>[0]), tagsOf.getOrDefault(block, List.of()));
        }
        return true;
    }

    private static Field field(Class<?> owner, String name) throws Exception {
        Field found = owner.getDeclaredField(name);
        found.setAccessible(true);
        return found;
    }

    /** StaticTagHelper's wrappers, 1.16 to 1.18.1. */
    private static boolean bindHelper(Map<String, Set<Object>> tags) throws Exception {
        Class<?> blockTags = type("net.minecraft.tags.BlockTags");
        Class<?> tagClass = type("net.minecraft.tags.Tag");
        Class<?> wrapperClass = type("net.minecraft.tags.StaticTagHelper$Wrapper");
        if (wrapperClass == null) {
            return false;
        }
        Field helper = field(blockTags, "HELPER");
        Field wrappers = field(helper.getType(), "wrappers");
        Field name = field(wrapperClass, "name");
        Field tag = field(wrapperClass, "tag");
        Method fromSet = method(tagClass, "fromSet", Set.class);
        Method create = fromSet != null ? fromSet : method(type("net.minecraft.tags.SetTag"), "create", Set.class);
        for (Object wrapper : (List<?>) wrappers.get(helper.get(null))) {
            String id = name.get(wrapper).toString().replaceFirst("^minecraft:", "");
            tag.set(wrapper, create.invoke(null, tags.getOrDefault(id, Set.of())));
        }
        return true;
    }

    /** BlockTags' wrappers, 1.14 and 1.15. */
    private static boolean bindWrappers(Map<String, Set<Object>> tags) throws Exception {
        Class<?> blockTags = type("net.minecraft.tags.BlockTags");
        Class<?> wrapperClass = type("net.minecraft.tags.BlockTags$Wrapper");
        Class<?> builderClass = type("net.minecraft.tags.Tag$Builder");
        if (wrapperClass == null || builderClass == null) {
            return false;
        }
        Field actual = field(wrapperClass, "actual");
        Field check = field(wrapperClass, "check");
        int resets = field(blockTags, "resetCount").getInt(null);
        Method add = method(builderClass, "add", Object.class);
        Class<?> identifierClass = type("net.minecraft.resources.ResourceLocation");
        Method build = method(builderClass, "build", identifierClass);
        for (Field constant : blockTags.getFields()) {
            Object wrapper = constant.get(null);
            if (!wrapperClass.isInstance(wrapper)) {
                continue;
            }
            Object id = method(wrapperClass, "getId").invoke(wrapper);
            Object builder = builderClass.getDeclaredConstructor().newInstance();
            for (Object block : tags.getOrDefault(id.toString().replaceFirst("^minecraft:", ""), Set.of())) {
                add.invoke(builder, block);
            }
            actual.set(wrapper, build.invoke(builder, id));
            check.setInt(wrapper, resets);
        }
        return true;
    }

    private static String bindTags(Object registry, String tagsFile) throws Exception {
        Map<String, Set<Object>> tags = members(registry, tagsFile);
        if (bindHolders(registry, tags)) {
            return "holders";
        }
        if (bindHelper(tags)) {
            return "helper";
        }
        if (bindWrappers(tags)) {
            return "wrappers";
        }
        throw new IllegalStateException("No place to bind this jar's tags: its tag predicates would all answer false");
    }

    private static Method keyMethod(Object registry) {
        Method keyOf = null;
        for (Method candidate : registry.getClass().getMethods()) {
            if (candidate.getName().equals("getKey") && candidate.getParameterCount() == 1
                && !candidate.getParameterTypes()[0].getName().contains("ResourceKey")) {
                keyOf = candidate;
            }
        }
        keyOf.setAccessible(true);
        return keyOf;
    }

    private static void dump(String output, String tagsFile) throws Exception {
        prepare();
        Class<?> blockClass = type("net.minecraft.world.level.block.Block");
        Object registry = blockRegistry();
        String bound = bindTags(registry, tagsFile);
        Method keyOf = keyMethod(registry);
        Method idOf = method(blockClass, "getId", type("net.minecraft.world.level.block.state.BlockState"));
        Class<?> propertyClass = type("net.minecraft.world.level.block.state.properties.Property");
        Method valueName = null;
        for (Method candidate : propertyClass.getDeclaredMethods()) {
            if (candidate.getName().equals("getName") && candidate.getParameterCount() == 1) {
                valueName = candidate;
            }
        }
        Method propertyName = propertyClass.getMethod("getName");
        try (BufferedWriter out = Files.newBufferedWriter(Path.of(output), StandardCharsets.UTF_8)) {
            out.write("tags\t" + bound);
            out.newLine();
            for (Object block : (Iterable<?>) registry) {
                String name = keyOf.invoke(registry, block).toString().replaceFirst("^minecraft:", "");
                Object definition = call(block, "getStateDefinition", new Class<?>[0]);
                Collection<?> properties = (Collection<?>) call(definition, "getProperties", new Class<?>[0]);
                List<?> states = (List<?>) call(definition, "getPossibleStates", new Class<?>[0]);
                out.write(String.join("\t", "block", name, block.getClass().getName(), number(block, "getFriction"),
                    number(block, "getSpeedFactor"), number(block, "getJumpFactor"),
                    Integer.toString(states.indexOf(call(block, "defaultBlockState", new Class<?>[0])))));
                out.newLine();
                for (int index = 0; index < states.size(); index++) {
                    Object state = states.get(index);
                    List<String> props = new ArrayList<>();
                    for (Object property : properties) {
                        Object value = method(state.getClass(), "getValue", propertyClass).invoke(state, property);
                        props.add(propertyName.invoke(property) + "=" + valueName.invoke(property, value));
                    }
                    props.sort(null);
                    String collision = boxes(collision(state, origin, emptyContext));
                    String outline = boxes(outline(state, origin, emptyContext));
                    String interaction = boxes(call(state, "getInteractionShape", new Class<?>[] {blockGetter, blockPos}, getter, origin));
                    String support = boxes(call(state, "getBlockSupportShape", new Class<?>[] {blockGetter, blockPos}, getter, origin));
                    Object fluid = flag(state, "getFluidState", new Class<?>[0]);
                    Object fluidType = fluid == null ? null : flag(fluid, "getType", new Class<?>[0]);
                    Object material = flag(state, "getMaterial", new Class<?>[0]);
                    String flags = bit(flag(state, "isAir", new Class<?>[0]))
                        + bit(material != null ? flag(material, "blocksMotion", new Class<?>[0]) : flag(state, "blocksMotion", new Class<?>[0]))
                        + bit(flag(state, "isSuffocating", new Class<?>[] {blockGetter, blockPos}, getter, origin))
                        + bit(flag(state, "hasLargeCollisionShape", new Class<?>[0]))
                        + bit(flag(state, "isCollisionShapeFullBlock", new Class<?>[] {blockGetter, blockPos}, getter, origin))
                        + bit(flag(block, "hasDynamicShape", new Class<?>[0]))
                        + bit(flag(state, "isRedstoneConductor", new Class<?>[] {blockGetter, blockPos}, getter, origin))
                        + ":" + (fluidType == null ? ABSENT : fluidType.getClass().getSimpleName());
                    out.write(String.join("\t", "state", name, Integer.toString(index), idOf.invoke(null, state).toString(),
                        String.join(",", props), collision, outline, interaction, support, sturdy(state, block), flags,
                        contexts(state, block, collision, outline, interaction, support)));
                    out.newLine();
                }
            }
        }
    }
}

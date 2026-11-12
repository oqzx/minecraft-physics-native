import java.io.BufferedWriter;
import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Asks a vanilla jar what its entities measure and what its attributes and effects are: the oracle of
 * scripts/generate-entity-data.cjs. Reflection lets one source run on every Mojang-named jar; `-` is a value a release
 * does not have, which is not 0. Lines, tab separated:
 *   entity NAME category width height fixed eyeHeight
 *   pose NAME width height fixed eyeHeight      the player's, Player.POSES
 *   attribute NAME default min max syncable
 *   effect NAME category modifier...            each modifier `attribute|amount|operation`
 *   callback NAME method class                  a block whose class overrides a movement callback of Block
 *   enchantment NAME maxLevel slot,slot         before 1.21, where they are classes: from 1.21 they are data
 *
 * An eye height exists as a value from 1.20.5, on EntityDimensions. Before, it is a method of a live entity, so `-`.
 *
 * javac -cp JARS -d out scripts/java-oracle/EntityDump.java
 * java -cp "out;JARS" EntityDump OUTPUT_FILE
 */
public final class EntityDump {
    private static final String ABSENT = "-";

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
                if (candidate.getName().equals(name) && candidate.getParameterCount() == parameters) {
                    candidate.setAccessible(true);
                    return candidate;
                }
            }
        }
        return null;
    }

    private static Object call(Object target, String... names) throws Exception {
        for (String name : names) {
            Method found = method(target.getClass(), name, 0);
            if (found != null) {
                return found.invoke(target);
            }
        }
        return null;
    }

    private static Object field(Object target, String name) throws Exception {
        for (Class<?> at = target.getClass(); at != null; at = at.getSuperclass()) {
            for (Field candidate : at.getDeclaredFields()) {
                if (candidate.getName().equals(name) && !Modifier.isStatic(candidate.getModifiers())) {
                    candidate.setAccessible(true);
                    return candidate.get(target);
                }
            }
        }
        return null;
    }

    private static Object registry(String name) {
        for (String owner : new String[] {"net.minecraft.core.registries.BuiltInRegistries", "net.minecraft.core.Registry"}) {
            Class<?> found = type(owner);
            if (found == null) {
                continue;
            }
            try {
                return found.getField(name).get(null);
            } catch (ReflectiveOperationException absent) {
                // the next owner
            }
        }
        return null;
    }

    private static Method keyMethod(Object registry) {
        for (Method candidate : registry.getClass().getMethods()) {
            if (candidate.getName().equals("getKey") && candidate.getParameterCount() == 1
                && !candidate.getParameterTypes()[0].getName().contains("ResourceKey")) {
                candidate.setAccessible(true);
                return candidate;
            }
        }
        throw new IllegalStateException("A registry with no getKey");
    }

    private static String name(Object key) {
        return key.toString().replaceFirst("^minecraft:", "");
    }

    private static String text(Object value) {
        return value == null ? ABSENT : value.toString();
    }

    private static String dimensions(Object dimensions) throws Exception {
        Object width = call(dimensions, "width");
        return String.join("\t", text(width != null ? width : field(dimensions, "width")),
            text(call(dimensions, "height") != null ? call(dimensions, "height") : field(dimensions, "height")),
            text(call(dimensions, "fixed") != null ? call(dimensions, "fixed") : field(dimensions, "fixed")),
            text(call(dimensions, "eyeHeight")));
    }

    private static Map<?, ?> poses() throws Exception {
        for (String owner : new String[] {"net.minecraft.world.entity.player.Player", "net.minecraft.world.entity.Avatar"}) {
            Class<?> found = type(owner);
            if (found == null) {
                continue;
            }
            for (Field candidate : found.getDeclaredFields()) {
                if (candidate.getName().equals("POSES")) {
                    candidate.setAccessible(true);
                    return (Map<?, ?>) candidate.get(null);
                }
            }
        }
        throw new IllegalStateException("No Player.POSES");
    }

    /** A modifier's name in the map: the holder's or the attribute's own registered key, else its description id. */
    private static String attributeName(Object key, Object registry, Method keyOf) throws Exception {
        Object attribute = key;
        if (method(key.getClass(), "value", 0) != null) {
            attribute = call(key, "value");
        }
        if (registry != null) {
            return name(keyOf.invoke(registry, attribute));
        }
        return text(call(attribute, "getName", "getDescriptionId"));
    }

    private static List<Object> legacyAttributes() throws Exception {
        List<Object> found = new ArrayList<>();
        for (Field constant : type("net.minecraft.world.entity.monster.SharedMonsterAttributes", "net.minecraft.world.entity.ai.attributes.SharedMonsterAttributes").getFields()) {
            found.add(constant.get(null));
        }
        return found;
    }

    /** The class below Block that declares a method of this name for a block, null where it keeps Block's. */
    private static String overrider(Class<?> blockType, String method) {
        for (Class<?> at = blockType; at != null && at.getName().startsWith("net.minecraft"); at = at.getSuperclass()) {
            String name = at.getSimpleName();
            if (name.equals("Block") || name.equals("BlockBehaviour")) {
                return null;
            }
            for (Method candidate : at.getDeclaredMethods()) {
                if (candidate.getName().equals(method)) {
                    return name;
                }
            }
        }
        return null;
    }

    private static void write(BufferedWriter out, String... fields) throws Exception {
        out.write(String.join("\t", fields));
        out.newLine();
    }

    private static void dump(String output) throws Exception {
        PrintStream real = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        Method detect = method(type("net.minecraft.SharedConstants"), "tryDetectVersion", 0);
        if (detect != null) {
            detect.invoke(null);
        }
        method(type("net.minecraft.server.Bootstrap"), "bootStrap", 0).invoke(null);
        System.setOut(real);
        try (BufferedWriter out = Files.newBufferedWriter(Path.of(output), StandardCharsets.UTF_8)) {
            Object entities = registry("ENTITY_TYPE");
            Method entityKey = keyMethod(entities);
            for (Object entity : (Iterable<?>) entities) {
                write(out, "entity", name(entityKey.invoke(entities, entity)), text(call(entity, "getCategory")),
                    dimensions(call(entity, "getDimensions")));
            }
            Map<String, String> poses = new TreeMap<>();
            for (Map.Entry<?, ?> pose : poses().entrySet()) {
                poses.put(pose.getKey().toString(), dimensions(pose.getValue()));
            }
            for (Map.Entry<String, String> pose : poses.entrySet()) {
                write(out, "pose", pose.getKey(), pose.getValue());
            }
            Object attributes = registry("ATTRIBUTE");
            Method attributeKey = attributes == null ? null : keyMethod(attributes);
            List<Object> every = new ArrayList<>();
            if (attributes != null) {
                ((Iterable<?>) attributes).forEach(every::add);
            } else {
                every.addAll(legacyAttributes());
            }
            for (Object attribute : every) {
                String id = attributes != null ? name(attributeKey.invoke(attributes, attribute)) : text(call(attribute, "getName"));
                write(out, "attribute", id, text(call(attribute, "getDefaultValue")), text(field(attribute, "minValue")),
                    text(field(attribute, "maxValue")), text(call(attribute, "isClientSyncable")));
            }
            Object blocks = registry("BLOCK");
            Method blockKey = keyMethod(blocks);
            String[] callbacks = {"entityInside", "stepOn", "fallOn", "updateEntityMovementAfterFallOn", "updateEntityAfterFallOn", "onProjectileHit"};
            for (Object block : (Iterable<?>) blocks) {
                for (String callback : callbacks) {
                    String owner = overrider(block.getClass(), callback);
                    if (owner != null) {
                        write(out, "callback", name(blockKey.invoke(blocks, block)), callback, owner);
                    }
                }
            }
            Object enchantments = registry("ENCHANTMENT");
            if (enchantments != null) {
                Method enchantmentKey = keyMethod(enchantments);
                for (Object enchantment : (Iterable<?>) enchantments) {
                    Object slots = field(enchantment, "slots");
                    Object definition = field(enchantment, "definition");
                    if (slots == null && definition != null) {
                        slots = call(definition, "slots");
                    }
                    List<String> names = new ArrayList<>();
                    if (slots instanceof Object[] array) {
                        for (Object slot : array) {
                            names.add(slot.toString());
                        }
                    } else if (slots instanceof Iterable<?> list) {
                        list.forEach(slot -> names.add(slot.toString()));
                    }
                    names.sort(null);
                    write(out, "enchantment", name(enchantmentKey.invoke(enchantments, enchantment)), text(call(enchantment, "getMaxLevel")), String.join(",", names));
                }
            }
            Object effects = registry("MOB_EFFECT");
            Method effectKey = keyMethod(effects);
            for (Object effect : (Iterable<?>) effects) {
                List<String> line = new ArrayList<>(List.of("effect", name(effectKey.invoke(effects, effect)), text(call(effect, "getCategory"))));
                List<String> modifiers = new ArrayList<>();
                for (Map.Entry<?, ?> entry : ((Map<?, ?>) field(effect, "attributeModifiers")).entrySet()) {
                    Object modifier = entry.getValue();
                    Method create = method(modifier.getClass(), "create", 1);
                    if (create != null && create.getParameterTypes()[0] == int.class) {
                        modifier = create.invoke(modifier, 0);
                    }
                    modifiers.add(attributeName(entry.getKey(), attributes, attributeKey) + "|" + text(call(modifier, "amount", "getAmount")) + "|"
                        + text(call(modifier, "operation", "getOperation")));
                }
                modifiers.sort(null);
                line.addAll(modifiers);
                write(out, line.toArray(new String[0]));
            }
        }
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

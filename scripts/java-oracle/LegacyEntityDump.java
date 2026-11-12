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
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Function;

/**
 * LiveEntityDump for 1.8.8 to 1.13.2, whose jars hold legacy-mappings' names (classpath.cjs). An entity here has no
 * dimensions object: its width and height are fields its constructor fills, so each is built on a client level with no
 * constructor run and read. Up to 1.12.2 the names and classes live in maps of EntityType, in 1.13.2 in its constants.
 *
 * Lines are LiveEntityDump's, and:
 *   size NAME width height
 *
 * java -XX:+UnlockDiagnosticVMOptions -XX:-BytecodeVerificationRemote -cp "out;JARS" LegacyEntityDump OUTPUT_FILE
 * (see OracleAttributesLegacy for the verifier)
 */
public final class LegacyEntityDump {
    private static final String ABSENT = "-";
    private static final Map<String, String> OLD_NAMES = Map.of("EntityHorse", "horse", "MinecartRideable", "minecart", "LavaSlime", "magma_cube", "PigZombie", "zombie_pigman", "Ozelot", "ocelot", "SnowMan", "snow_golem", "VillagerGolem", "iron_golem", "MushroomCow", "mooshroom", "WitherBoss", "wither", "EnderDragon", "ender_dragon");

    private static Class<?> type(String name) {
        try {
            return Class.forName(name);
        } catch (ClassNotFoundException | LinkageError absent) {
            return null;
        }
    }

    private static Field field(Class<?> owner, String name) {
        return LiveEntityDump.fieldOf(owner, name);
    }

    private static Object level() throws Exception {
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) theUnsafe.get(null);
        Class<?> levelClass = type("net.minecraft.client.multiplayer.MultiPlayerLevel");
        Object level = unsafe.allocateInstance(levelClass);
        Class<?> normal = type("net.minecraft.world.level.dimension.NormalDimension");
        Object overworld = unsafe.allocateInstance(normal);
        Class<?> dimensionType = type("net.minecraft.world.level.dimension.DimensionType");
        Field held = field(normal, "type");
        if (held != null && dimensionType != null) {
            held.set(overworld, dimensionType.getField("OVERWORLD").get(null));
        }
        // (final fields: an allocated instance holds nulls, and these are read at once)
        unsafe.putObject(level, unsafe.objectFieldOffset(field(levelClass, "dimension")), overworld);
        unsafe.putBoolean(level, unsafe.objectFieldOffset(field(levelClass, "isClientSide")), true);
        unsafe.putObject(level, unsafe.objectFieldOffset(field(levelClass, "random")), new java.util.Random(0L));
        return level;
    }

    private static String id(String name) {
        String key = name.replaceFirst("^minecraft:", "");
        if (OLD_NAMES.containsKey(key)) {
            return OLD_NAMES.get(key);
        }
        return key.replaceAll("([a-z])([A-Z])", "$1_$2").toLowerCase();
    }

    /** Each entity by its id, with what builds one on a level. */
    private static Map<String, Function<Object, Object>> factories() throws Exception {
        Class<?> entityTypes = type("net.minecraft.world.entity.EntityType");
        Class<?> levelClass = type("net.minecraft.world.level.Level");
        Map<String, Function<Object, Object>> found = new LinkedHashMap<>();
        Map<String, Class<?>> classes = new LinkedHashMap<>();
        Field names = field(entityTypes, "NAME_CLASS_MAP");
        if (names != null) {
            ((Map<?, ?>) names.get(null)).forEach((name, value) -> classes.put(name.toString(), (Class<?>) value));
        } else {
            for (Field candidate : entityTypes.getFields()) {
                Object held = Modifier.isStatic(candidate.getModifiers()) ? candidate.get(null) : null;
                if (held != null && held.getClass().getSimpleName().equals("MappedRegistry")) {
                    for (Object key : (Iterable<?>) held.getClass().getMethod("getKeySet").invoke(held)) {
                        classes.put(key.toString(), (Class<?>) held.getClass().getMethod("get", Object.class).invoke(held, key));
                    }
                }
            }
        }
        for (Map.Entry<String, Class<?>> entry : classes.entrySet()) {
            found.put(id(entry.getKey()), level -> {
                try {
                    Constructor<?> constructor = entry.getValue().getConstructor(levelClass);
                    return constructor.newInstance(level);
                } catch (ReflectiveOperationException failed) {
                    throw new IllegalStateException(failed);
                }
            });
        }
        if (classes.isEmpty()) {
            Method keyOf = entityTypes.getMethod("getKey", entityTypes);
            Field factory = field(entityTypes, "factory");
            for (Field candidate : entityTypes.getFields()) {
                if (Modifier.isStatic(candidate.getModifiers()) && candidate.getType() == entityTypes) {
                    Object entityType = candidate.get(null);
                    @SuppressWarnings("unchecked")
                    Function<Object, Object> build = (Function<Object, Object>) factory.get(entityType);
                    found.put(id(keyOf.invoke(null, entityType).toString()), build);
                }
            }
        }
        return found;
    }

    private static String ask(Object target, String name) {
        Method found = LiveEntityDump.methodOf(target.getClass(), name, 0);
        if (found == null) {
            return ABSENT;
        }
        try {
            Object answer = found.invoke(target);
            return answer == null ? ABSENT : answer.toString();
        } catch (ReflectiveOperationException | RuntimeException refused) {
            return ABSENT;
        }
    }

    private static String reason(Throwable failed) {
        Throwable at = failed;
        while (at.getCause() != null) {
            at = at.getCause();
        }
        return at.getClass().getSimpleName();
    }

    /** 1.8.8 seats one rider, in a field, and positionRider takes nothing. */
    private static void oneRider(List<String> lines, String name, Object vehicle, Object rider) throws Exception {
        Method position = LiveEntityDump.methodOf(vehicle.getClass(), "positionRider", 0);
        Field seat = field(vehicle.getClass(), "rider");
        if (position == null || seat == null) {
            return;
        }
        seat.set(vehicle, rider);
        for (float yaw : LiveEntityDump.YAWS) {
            LiveEntityDump.turn(vehicle, yaw);
            LiveEntityDump.turn(rider, 0.0F);
            field(rider.getClass(), "x").setDouble(rider, 0.0D);
            field(rider.getClass(), "y").setDouble(rider, 0.0D);
            field(rider.getClass(), "z").setDouble(rider, 0.0D);
            try {
                position.invoke(vehicle);
                lines.add(String.join("\t", "ridercase", name, "0", Float.toString(yaw), "1", Double.toString(field(rider.getClass(), "x").getDouble(rider)),
                    Double.toString(field(rider.getClass(), "y").getDouble(rider)), Double.toString(field(rider.getClass(), "z").getDouble(rider))));
            } catch (ReflectiveOperationException | RuntimeException refused) {
                // nothing is written for a case the blank level cannot answer
            }
        }
        seat.set(vehicle, null);
    }

    public static void main(String[] args) throws Exception {
        PrintStream real = System.out;
        PrintStream err = System.err;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        LiveEntityDump.methodOf(type("net.minecraft.server.Bootstrap"), "bootStrap", 0).invoke(null);
        System.setErr(err);
        Object level = level();
        Map<String, Function<Object, Object>> factories = factories();
        if (factories.isEmpty()) {
            throw new IllegalStateException("No entity in this jar");
        }
        Object seated = factories.get("zombie").apply(level);
        Object beside = factories.get("zombie").apply(level);
        List<String> lines = new ArrayList<>();
        for (Map.Entry<String, Function<Object, Object>> entry : new java.util.TreeMap<>(factories).entrySet()) {
            String name = entry.getKey();
            Object entity;
            try {
                entity = entry.getValue().apply(level);
            } catch (Throwable failed) {
                lines.add(String.join("\t", "nolive", name, reason(failed)));
                continue;
            }
            if (entity == null) {
                lines.add(String.join("\t", "nolive", name, "NoEntity"));
                continue;
            }
            lines.add(String.join("\t", "size", name, field(entity.getClass(), "width").get(entity).toString(), field(entity.getClass(), "height").get(entity).toString()));
            lines.add(String.join("\t", "live", name, ask(entity, "getEyeHeight"), ABSENT, ask(entity, "getRideHeight"), ask(entity, "getRidingHeight"), ABSENT));
            if (LiveEntityDump.VEHICLES.matcher(name).matches()) {
                if (field(entity.getClass(), "passengers") != null) {
                    LiveEntityDump.riderCases(lines, name, entity, seated, beside);
                } else {
                    oneRider(lines, name, entity, seated);
                }
            }
        }
        System.setOut(real);
        try (BufferedWriter out = Files.newBufferedWriter(Path.of(args[0]), StandardCharsets.UTF_8)) {
            for (String line : lines) {
                out.write(line);
                out.newLine();
            }
        }
        System.exit(0);
    }

    private LegacyEntityDump() {}
}

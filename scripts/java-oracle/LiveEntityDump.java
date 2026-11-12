import java.io.BufferedWriter;
import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Builds each entity of a vanilla jar for real and asks it what only a live entity knows: its eye height before 1.20.5,
 * where it is a method and not a value, and where its riders go. The level is a client level with no constructor run,
 * which is enough because an entity's constructor only keeps it; the type's own factory is called so the feature gate
 * in EntityType.create, which would ask that empty level, stays out. An entity whose constructor needs more says so.
 *
 * Lines, tab separated (`-` for a method the release lacks, or an answer that threw):
 *   live NAME eyeStanding eyeCrouching passengersRidingOffset myRidingOffset passengerSlots
 *   nolive NAME reason
 *   attachment NAME index x y z                 a rider's point from EntityDimensions, from 1.20.5
 *   riderpoint NAME x y z                       getPassengerAttachmentPoint of a live entity, 1.20.2 to 1.20.4
 *   riderfn NAME class                          the class below Entity that computes a rider's point: a table is not
 *                                               the whole answer for this entity
 *   ridercase NAME slot yaw size x y z          where Entity.positionRider puts a zombie in that slot of a vehicle at the
 *                                               origin turned to that yaw; size is 1, 2 (the scale attribute) or baby
 *                                               slot -1 is one rider alone in what seats more; else every seat is full
 *   playereye POSE value                        Player.getStandingEyeHeight, on a player with no constructor run:
 *                                               the player type has no factory, and the method only reads its pose
 *
 * javac -cp JARS -d out scripts/java-oracle/LiveEntityDump.java
 * java -cp "out;JARS" LiveEntityDump OUTPUT_FILE
 */
public final class LiveEntityDump {
    private static final String ABSENT = "-";
    private static final int SLOTS = 8;
    /** What a rider can sit on. A rider's point is asked of these only: the table of anything else is read directly. */
    static final java.util.regex.Pattern VEHICLES = java.util.regex.Pattern.compile(
        "boat|chest_boat|oak_boat|oak_chest_boat|bamboo_raft|bamboo_chest_raft|minecart|pig|strider|ravager|spider|chicken"
            + "|horse|donkey|mule|skeleton_horse|zombie_horse|llama|trader_llama|camel|camel_husk|happy_ghast|nautilus|zombie_nautilus");
    static final float[] YAWS = {0.0F, 37.5F, 90.0F};

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

    static Method methodOf(Class<?> owner, String name, int parameters) {
        return method(owner, name, parameters);
    }

    static Field fieldOf(Class<?> owner, String name) {
        return field(owner, name);
    }

    private static Object emptyLevel() throws Exception {
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        sun.misc.Unsafe unsafe = (sun.misc.Unsafe) theUnsafe.get(null);
        Class<?> levelClass = type("net.minecraft.client.multiplayer.ClientLevel", "net.minecraft.client.multiplayer.MultiPlayerLevel");
        if (levelClass == null) {
            throw new IllegalStateException("No client level class in this jar");
        }
        Object level = unsafe.allocateInstance(levelClass);
        // (before 1.16 an entity's constructor reads level.dimension.getType())
        Class<?> normal = type("net.minecraft.world.level.dimension.NormalDimension");
        Field dimension = field(levelClass, "dimension");
        if (normal != null && dimension != null) {
            Object overworld = unsafe.allocateInstance(normal);
            field(normal, "type").set(overworld, type("net.minecraft.world.level.dimension.DimensionType").getField("OVERWORLD").get(null));
            dimension.set(level, overworld);
        }
        // (a camel's seat follows its sitting animation, which reads the game time: 0 here, standing)
        Class<?> levelData = type("net.minecraft.client.multiplayer.ClientLevel$ClientLevelData");
        for (String name : new String[] {"levelData", "clientLevelData"}) {
            Field data = field(levelClass, name);
            if (levelData != null && data != null) {
                unsafe.putObject(level, unsafe.objectFieldOffset(data), unsafe.allocateInstance(levelData));
            }
        }
        Field client = field(level.getClass(), "isClientSide");
        if (client != null) {
            client.setBoolean(level, true);
        }
        return level;
    }

    private static Object registry(String name) throws Exception {
        for (String owner : new String[] {"net.minecraft.core.registries.BuiltInRegistries", "net.minecraft.core.Registry"}) {
            Class<?> found = type(owner);
            if (found != null) {
                try {
                    return found.getField(name).get(null);
                } catch (NoSuchFieldException absent) {
                    // the next owner
                }
            }
        }
        throw new IllegalStateException("No registry " + name);
    }

    private static String ask(Object target, String name, Object... arguments) {
        Method found = method(target.getClass(), name, arguments.length);
        if (found == null) {
            return ABSENT;
        }
        try {
            Object answer = found.invoke(target, arguments);
            return answer == null ? ABSENT : answer.toString();
        } catch (ReflectiveOperationException | RuntimeException refused) {
            return ABSENT;
        }
    }

    private static String reason(Throwable failed) {
        Throwable at = failed;
        while ((at instanceof InvocationTargetException || at instanceof ExceptionInInitializerError) && at.getCause() != null) {
            at = at.getCause();
        }
        return at.getClass().getSimpleName();
    }

    private static Object pose(String... names) {
        Class<?> poses = type("net.minecraft.world.entity.Pose");
        for (String name : names) {
            for (Object constant : poses.getEnumConstants()) {
                if (constant.toString().equals(name)) {
                    return constant;
                }
            }
        }
        return null;
    }

    private static List<String> attachments(String name, Object entity, Object entityType) throws Exception {
        List<String> lines = new ArrayList<>();
        Object dimensions = method(entityType.getClass(), "getDimensions", 0).invoke(entityType);
        Method attachmentsOf = method(dimensions.getClass(), "attachments", 0);
        if (attachmentsOf != null) {
            Object held = attachmentsOf.invoke(dimensions);
            Class<?> kinds = type("net.minecraft.world.entity.EntityAttachment");
            Object passenger = null;
            for (Object constant : kinds.getEnumConstants()) {
                if (constant.toString().equals("PASSENGER")) {
                    passenger = constant;
                }
            }
            Method getNullable = method(held.getClass(), "getNullable", 3);
            for (int slot = 0; slot < SLOTS; slot++) {
                Object point = getNullable.invoke(held, passenger, slot, 0.0F);
                if (point != null) {
                    lines.add(String.join("\t", "attachment", name, Integer.toString(slot), point.getClass().getField("x").get(point).toString(),
                        point.getClass().getField("y").get(point).toString(), point.getClass().getField("z").get(point).toString()));
                }
            }
        }
        return lines;
    }

    private static String riderPoint(String name, Object entity, Object entityType, Object rider) throws Exception {
        Method point = method(entity.getClass(), "getPassengerAttachmentPoint", 3);
        if (point == null || rider == null) {
            return null;
        }
        Object dimensions = method(entityType.getClass(), "getDimensions", 0).invoke(entityType);
        // (a living entity's constructor rolls its body yaw, and a rider's point turns with it: facing south is the point)
        for (String name2 : new String[] {"yBodyRot", "yBodyRotO", "yRot", "yRotO", "yHeadRot", "yHeadRotO"}) {
            Field yaw = field(entity.getClass(), name2);
            if (yaw != null && yaw.getType() == float.class) {
                yaw.setFloat(entity, 0.0F);
            }
        }
        try {
            Object answer = point.invoke(entity, rider, dimensions, 1.0F);
            String[] fields = {"x", "y", "z"};
            List<String> line = new ArrayList<>(List.of("riderpoint", name));
            for (String axis : fields) {
                line.add(answer.getClass().getField(axis).get(answer).toString());
            }
            return String.join("\t", line);
        } catch (ReflectiveOperationException | RuntimeException refused) {
            return null;
        }
    }

    private static int seats(String name) {
        return switch (name) {
            case "happy_ghast" -> 4;
            case "boat", "oak_boat", "bamboo_raft", "camel", "camel_husk" -> 2;
            default -> 1;
        };
    }

    private static double coordinate(Object entity, String axis) throws Exception {
        Method getter = method(entity.getClass(), "get" + axis.toUpperCase(), 0);
        return getter != null ? ((Number) getter.invoke(entity)).doubleValue() : field(entity.getClass(), axis).getDouble(entity);
    }

    static void turn(Object entity, float yaw) throws Exception {
        for (String name : new String[] {"yBodyRot", "yBodyRotO", "yRot", "yRotO", "yHeadRot", "yHeadRotO"}) {
            Field held = field(entity.getClass(), name);
            if (held != null && held.getType() == float.class) {
                held.setFloat(entity, yaw);
            }
        }
    }

    /** The scale attribute's base, where the entity has one; false where it has none. */
    private static boolean scaled(Object entity, double scale) {
        Class<?> attributes = type("net.minecraft.world.entity.ai.attributes.Attributes");
        Method getAttribute = method(entity.getClass(), "getAttribute", 1);
        if (attributes == null || getAttribute == null) {
            return false;
        }
        try {
            Object instance = getAttribute.invoke(entity, attributes.getField("SCALE").get(null));
            if (instance == null) {
                return false;
            }
            method(instance.getClass(), "setBaseValue", 1).invoke(instance, scale);
            return true;
        } catch (ReflectiveOperationException | RuntimeException refused) {
            return false;
        }
    }

    /** Entity.positionRider itself, which every era has: seats, turns and scales are its own, none of them worked out here. */
    private static void riderCase(List<String> lines, String name, Object vehicle, Object rider, Object other, int slot, float yaw, String size) throws Exception {
        Method position = method(vehicle.getClass(), "positionRider", 1);
        if (position == null) {
            return;
        }
        List<Object> riders = new ArrayList<>();
        for (int at = 0; at < seats(name); at++) {
            riders.add(at == slot ? rider : other);
        }
        Field held = field(vehicle.getClass(), "passengers");
        Object before = held.get(vehicle);
        held.set(vehicle, Class.forName("com.google.common.collect.ImmutableList").getMethod("copyOf", java.util.Collection.class).invoke(null, slot < 0 ? List.of(rider) : riders));
        turn(vehicle, yaw);
        turn(rider, 0.0F);
        Method setPos = method(rider.getClass(), "setPos", 3);
        setPos.invoke(rider, 0.0D, 0.0D, 0.0D);
        try {
            position.invoke(vehicle, rider);
            lines.add(String.join("\t", "ridercase", name, Integer.toString(slot), Float.toString(yaw), size,
                Double.toString(coordinate(rider, "x")), Double.toString(coordinate(rider, "y")), Double.toString(coordinate(rider, "z"))));
        } catch (ReflectiveOperationException | RuntimeException refused) {
            // nothing is written for a case the blank level cannot answer
        } finally {
            held.set(vehicle, before);
        }
    }

    static void riderCases(List<String> lines, String name, Object vehicle, Object rider, Object other) throws Exception {
        // (from 26.2 Entity.equals reads an id, and a passenger list is searched by equals)
        int id = 1;
        for (Object entity : new Object[] {vehicle, rider, other}) {
            Method setId = method(entity.getClass(), "setId", 1);
            if (setId != null && setId.getParameterTypes()[0] == int.class) {
                setId.invoke(entity, id++);
            }
        }
        for (int slot = seats(name) > 1 ? -1 : 0; slot < seats(name); slot++) {
            for (float yaw : YAWS) {
                riderCase(lines, name, vehicle, rider, other, slot, yaw, "1");
            }
        }
        if (scaled(vehicle, 2.0D)) {
            for (int slot = 0; slot < seats(name); slot++) {
                for (float yaw : YAWS) {
                    riderCase(lines, name, vehicle, rider, other, slot, yaw, "2");
                }
            }
            scaled(vehicle, 1.0D);
        }
        Method setBaby = method(vehicle.getClass(), "setBaby", 1);
        if (setBaby != null && setBaby.getParameterTypes()[0] == boolean.class) {
            try {
                setBaby.invoke(vehicle, true);
                riderCase(lines, name, vehicle, rider, other, 0, YAWS[1], "baby");
                setBaby.invoke(vehicle, false);
            } catch (ReflectiveOperationException | RuntimeException refused) {
                // an entity that keeps no age
            }
        }
    }

    /** Player.getStandingEyeHeight of every pose in POSES: only where it is a method, before 1.20.5. */
    private static List<String> playerEyes() throws Exception {
        List<String> lines = new ArrayList<>();
        Class<?> remote = type("net.minecraft.client.player.RemotePlayer");
        Method eye = method(remote, "getStandingEyeHeight", 2);
        if (eye == null) {
            return lines;
        }
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        Object player = ((sun.misc.Unsafe) theUnsafe.get(null)).allocateInstance(remote);
        Field poses = field(type("net.minecraft.world.entity.player.Player"), "POSES");
        Object standing = field(type("net.minecraft.world.entity.player.Player"), "STANDING_DIMENSIONS").get(null);
        for (Object pose : type("net.minecraft.world.entity.Pose").getEnumConstants()) {
            Object dimensions = ((java.util.Map<?, ?>) poses.get(null)).get(pose);
            lines.add(String.join("\t", "playereye", pose.toString(), eye.invoke(player, pose, dimensions != null ? dimensions : standing).toString()));
        }
        return lines;
    }

    private static void dump(String output) throws Exception {
        PrintStream real = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        Method detect = method(type("net.minecraft.SharedConstants"), "tryDetectVersion", 0);
        if (detect != null) {
            detect.invoke(null);
        }
        method(type("net.minecraft.server.Bootstrap"), "bootStrap", 0).invoke(null);
        Object level = emptyLevel();
        Object types = registry("ENTITY_TYPE");
        Method keyOf = null;
        for (Method candidate : types.getClass().getMethods()) {
            if (candidate.getName().equals("getKey") && candidate.getParameterCount() == 1 && !candidate.getParameterTypes()[0].getName().contains("ResourceKey")) {
                keyOf = candidate;
            }
        }
        keyOf.setAccessible(true);
        Object standing = pose("STANDING");
        Object crouching = pose("CROUCHING", "SNEAKING");
        List<String> lines = new ArrayList<>();
        Object rider = null;
        Object seated = null;
        Object beside = null;
        for (Object entityType : (Iterable<?>) types) {
            if (keyOf.invoke(types, entityType).toString().endsWith(":zombie")) {
                Object factory = field(entityType.getClass(), "factory").get(entityType);
                for (Method candidate : factory.getClass().getMethods()) {
                    if (candidate.getName().equals("create") && candidate.getParameterCount() == 2) {
                        candidate.setAccessible(true);
                        seated = candidate.invoke(factory, entityType, level);
                        beside = candidate.invoke(factory, entityType, level);
                    }
                }
            }
        }
        for (Object entityType : (Iterable<?>) types) {
            String name = keyOf.invoke(types, entityType).toString().replaceFirst("^minecraft:", "");
            if (System.getenv("LIVE_TRACE") != null) {
                Files.writeString(Path.of(System.getenv("LIVE_TRACE")), name);
            }
            Object factory = field(entityType.getClass(), "factory").get(entityType);
            Method create = null;
            for (Method candidate : factory.getClass().getMethods()) {
                if (candidate.getName().equals("create") && candidate.getParameterCount() == 2) {
                    create = candidate;
                }
            }
            create.setAccessible(true);
            Object entity;
            try {
                entity = create.invoke(factory, entityType, level);
            } catch (Throwable failed) {
                lines.add(String.join("\t", "nolive", name, reason(failed)));
                // (a table belongs to the type: an entity that cannot be built still has one)
                lines.addAll(attachments(name, null, entityType));
                continue;
            }
            if (entity == null) {
                lines.add(String.join("\t", "nolive", name, "NoEntity"));
                continue;
            }
            if (rider == null && name.equals("pig")) {
                rider = entity;
            }
            // (getRideHeight and getRidingHeight before 1.16; getMyRidingOffset takes its vehicle from 1.20.2)
            String mine = ask(entity, "getMyRidingOffset");
            if (mine.equals(ABSENT)) {
                mine = ask(entity, "getRidingHeight");
            }
            if (mine.equals(ABSENT) && rider != null) {
                mine = ask(entity, "getMyRidingOffset", rider);
            }
            String seats = ask(entity, "getPassengersRidingOffset");
            lines.add(String.join("\t", "live", name, ask(entity, "getEyeHeight", standing), ask(entity, "getEyeHeight", crouching),
                seats.equals(ABSENT) ? ask(entity, "getRideHeight") : seats, mine, ask(entity, "getMaxPassengers")));
            lines.addAll(attachments(name, entity, entityType));
            for (Class<?> at = entity.getClass(); at != null && !at.getSimpleName().equals("Entity"); at = at.getSuperclass()) {
                boolean computes = false;
                for (Method candidate : at.getDeclaredMethods()) {
                    computes |= candidate.getName().equals("getPassengerAttachmentPoint") || candidate.getName().equals("positionRider");
                }
                if (computes) {
                    lines.add(String.join("\t", "riderfn", name, at.getSimpleName()));
                    break;
                }
            }
            String point = riderPoint(name, entity, entityType, rider);
            if (point != null) {
                lines.add(point);
            }
            if (seated != null && VEHICLES.matcher(name).matches()) {
                riderCases(lines, name, entity, seated, beside);
            }
        }
        lines.addAll(playerEyes());
        System.setOut(real);
        try (BufferedWriter out = Files.newBufferedWriter(Path.of(output), StandardCharsets.UTF_8)) {
            for (String line : lines) {
                out.write(line);
                out.newLine();
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

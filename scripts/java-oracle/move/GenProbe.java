import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;

/**
 * Writes ProbeLevel.java and ProbeEntity.java for the jar on the classpath: subclasses of Level and Entity with every
 * abstract method of that release filled in, so OracleMove can run the jar's own Entity.move. The level is allocated
 * without its constructor and answers from Hooks; anything else it is asked throws, so a query the harness does not
 * model cannot go unnoticed.
 *
 * java -cp "JARS" scripts/java-oracle/move/GenProbe.java OUTDIR
 */
public final class GenProbe {
    private static String type(Class<?> c) {
        return c.isArray() ? type(c.getComponentType()) + "[]" : c.getCanonicalName();
    }

    private static String params(Method m) {
        StringBuilder out = new StringBuilder();
        Class<?>[] p = m.getParameterTypes();
        for (int i = 0; i < p.length; i++) {
            out.append(i == 0 ? "" : ", ").append(type(p[i])).append(" a").append(i);
        }
        return out.toString();
    }

    private static String nullOf(Class<?> c) {
        if (c == boolean.class) return "false";
        if (c == void.class) return "";
        if (c.isPrimitive()) return "(" + c.getName() + ") 0";
        return "(" + type(c) + ") null";
    }

    /** The methods a subclass has to fill in or may override, nearest declaration first. */
    private static Map<String, Method> visible(Class<?> root) {
        Map<String, Method> seen = new LinkedHashMap<>();
        ArrayDeque<Class<?>> walk = new ArrayDeque<>();
        List<Class<?>> interfaces = new ArrayList<>();
        for (Class<?> c = root; c != null; c = c.getSuperclass()) {
            for (Method m : c.getDeclaredMethods()) {
                add(seen, m);
            }
            walk.addAll(List.of(c.getInterfaces()));
        }
        while (!walk.isEmpty()) {
            Class<?> c = walk.poll();
            if (interfaces.contains(c)) continue;
            interfaces.add(c);
            walk.addAll(List.of(c.getInterfaces()));
        }
        // (a sub-interface's default answers a super-interface's abstract method)
        for (int i = 0; i < interfaces.size(); i++) {
            for (int j = i + 1; j < interfaces.size(); j++) {
                if (interfaces.get(i).isAssignableFrom(interfaces.get(j))) {
                    Class<?> swap = interfaces.get(i);
                    interfaces.set(i, interfaces.get(j));
                    interfaces.set(j, swap);
                }
            }
        }
        for (Class<?> c : interfaces) {
            for (Method m : c.getDeclaredMethods()) {
                add(seen, m);
            }
        }
        return seen;
    }

    private static void add(Map<String, Method> seen, Method m) {
        int mods = m.getModifiers();
        if (Modifier.isStatic(mods) || Modifier.isPrivate(mods) || m.isSynthetic()) return;
        seen.putIfAbsent(m.getName() + java.util.Arrays.toString(m.getParameterTypes()), m);
    }

    private static final Map<String, String> LEVEL = Map.ofEntries(
        Map.entry("getBlockState", "return Hooks.state(a0);"),
        Map.entry("getFluidState", "return Hooks.state(a0).getFluidState();"),
        Map.entry("getWorldBorder", "return Hooks.border();"),
        Map.entry("getEntityCollisions", "return java.util.List.of();"),
        Map.entry("getEntities", "return Hooks.near(a1);"),
        Map.entry("isRainingAt", "return false;"),
        Map.entry("neighborShapeChanged", ""),
        Map.entry("updateNeighborsAt", ""),
        Map.entry("getChunkForCollisions", "return this;"),
        Map.entry("getBlockEntity", "return null;"),
        Map.entry("getGameTime", "return Hooks.gameTime;"),
        Map.entry("getMinBuildHeight", "return -64;"),
        Map.entry("getMinY", "return -64;"),
        Map.entry("isOutsideBuildHeight", "return false;"),
        Map.entry("getBlockStatesIfLoaded", "return java.util.stream.Stream.empty();"),
        Map.entry("hasChunksAt", "return true;"),
        Map.entry("hasChunkAt", "return true;"),
        Map.entry("damageSources", "return Hooks.nothing(net.minecraft.world.damagesource.DamageSources.class);"),
        Map.entry("getProfiler", "return Hooks.nothing(net.minecraft.util.profiling.InactiveProfiler.class);"));

    private static final String NL = System.lineSeparator();

    private static String levelClass() {
        Constructor<?> ctor = Level.class.getDeclaredConstructors()[0];
        StringBuilder args = new StringBuilder();
        for (Class<?> p : ctor.getParameterTypes()) {
            args.append(args.length() == 0 ? "" : ", ").append(nullOf(p));
        }
        StringBuilder out = new StringBuilder("@SuppressWarnings({\"unchecked\", \"rawtypes\"})\n");
        out.append("public final class ProbeLevel extends net.minecraft.world.level.Level {\n");
        out.append("    ProbeLevel() { super(").append(args).append("); }\n");
        for (Method m : visible(Level.class).values()) {
            int mods = m.getModifiers();
            if (Modifier.isFinal(mods) || m.getDeclaringClass() == Object.class) continue;
            String body = LEVEL.get(m.getName());
            if (m.getName().equals("getEntities") && m.getReturnType() != List.class) body = null;
            boolean byPosition = m.getName().equals("getBlockState") || m.getName().equals("getFluidState") || m.getName().equals("getBlockEntity");
            if (byPosition && (m.getParameterCount() != 1 || m.getParameterTypes()[0] != net.minecraft.core.BlockPos.class)) {
                body = null;
            }
            if (body == null && !Modifier.isAbstract(mods)) continue;
            if (m.getName().equals("getEntityCollisions") && m.getReturnType() == java.util.stream.Stream.class) {
                body = "return java.util.stream.Stream.empty();";
            }
            // (what a move tells the world and reads nothing back from)
            if (body == null && m.getReturnType() == void.class && List.of("gameEvent", "playSound", "levelEvent", "playLocalSound", "playSeededSound").contains(m.getName())) {
                body = "";
            }
            out.append("    @Override public ").append(type(m.getReturnType())).append(' ').append(m.getName()).append('(').append(params(m)).append(") { ");
            out.append(body != null ? body : "throw new UnsupportedOperationException(\"" + m.getName() + "\");").append(" }\n");
        }
        return out.append("}\n").toString();
    }

    private static String entityClass() {
        StringBuilder out = new StringBuilder("@SuppressWarnings({\"unchecked\", \"rawtypes\"})\n");
        out.append("public abstract class ProbeEntity extends net.minecraft.world.entity.Entity {\n");
        out.append("    ProbeEntity(net.minecraft.world.entity.EntityType<?> type, net.minecraft.world.level.Level level) { super(type, level); }\n");
        out.append("    boolean local = true;\n");
        for (Method m : visible(Entity.class).values()) {
            // the authority the jar asks of an entity: a name of its own per release
            if (m.getName().equals("isClientAuthoritative") && !Modifier.isFinal(m.getModifiers())) {
                out.append("    @Override public boolean isClientAuthoritative() { return false; }\n");
            }
            // a move's step, gravity and noise, as the jar spells them
            if (m.getName().equals("maxUpStep") && m.getParameterCount() == 0 && !Modifier.isFinal(m.getModifiers())) {
                out.append("    @Override public float maxUpStep() { return 0.6F; }" + NL);
            }
            if (m.getName().equals("getDefaultGravity") && !Modifier.isFinal(m.getModifiers())) {
                out.append("    @Override protected double getDefaultGravity() { return 0.08; }" + NL);
            }
            if (m.getName().equals("getMovementEmission")) {
                out.append("    @Override protected net.minecraft.world.entity.Entity.MovementEmission getMovementEmission() { return net.minecraft.world.entity.Entity.MovementEmission.NONE; }" + NL);
            }
            if (m.getName().equals("isMovementNoisy")) {
                out.append("    @Override protected boolean isMovementNoisy() { return false; }" + NL);
            }
            if (m.getName().equals("isControlledByLocalInstance") && !Modifier.isFinal(m.getModifiers())) {
                out.append("    @Override public boolean isControlledByLocalInstance() { return local; }\n");
            }
            if (!Modifier.isAbstract(m.getModifiers())) continue;
            String value = nullOf(m.getReturnType());
            out.append("    @Override ").append(Modifier.isPublic(m.getModifiers()) ? "public " : "protected ").append(type(m.getReturnType())).append(' ').append(m.getName());
            out.append('(').append(params(m)).append(") { ").append(value.isEmpty() ? "" : "return " + value + ";").append(" }\n");
        }
        return out.append("}\n").toString();
    }

    /** A LivingEntity with its abstract methods filled in, for scripts/java-oracle/living/OracleLiving.java. */
    private static String livingClass() {
        StringBuilder out = new StringBuilder("@SuppressWarnings({\"unchecked\", \"rawtypes\"})\n");
        out.append("public abstract class ProbeLiving extends net.minecraft.world.entity.LivingEntity {\n");
        out.append("    ProbeLiving(net.minecraft.world.entity.EntityType type, net.minecraft.world.level.Level level) { super(type, level); }\n");
        for (Method m : visible(net.minecraft.world.entity.LivingEntity.class).values()) {
            if (m.getName().equals("isControlledByLocalInstance") && !Modifier.isFinal(m.getModifiers())) {
                out.append("    @Override public boolean isControlledByLocalInstance() { return true; }\n");
            }
            if (m.getName().equals("getMovementEmission")) {
                out.append("    @Override protected net.minecraft.world.entity.Entity.MovementEmission getMovementEmission() { return net.minecraft.world.entity.Entity.MovementEmission.NONE; }\n");
            }
            if (m.getName().equals("isMovementNoisy")) {
                out.append("    @Override protected boolean isMovementNoisy() { return false; }\n");
            }
            // (from 26.1 a dry probe would ask for the chunk's sections)
            if (m.getName().equals("updateFluidInteraction") && m.getReturnType() == boolean.class) {
                out.append("    @Override protected boolean updateFluidInteraction() { return false; }\n");
            }
            if (!Modifier.isAbstract(m.getModifiers())) continue;
            String value = nullOf(m.getReturnType());
            if (m.getReturnType().getSimpleName().equals("HumanoidArm")) value = "net.minecraft.world.entity.HumanoidArm.RIGHT";
            if (m.getReturnType().getSimpleName().equals("ItemStack")) value = "net.minecraft.world.item.ItemStack.EMPTY";
            if (m.getReturnType() == Iterable.class) value = "java.util.List.of()";
            out.append("    @Override ").append(Modifier.isPublic(m.getModifiers()) ? "public " : "protected ").append(type(m.getReturnType())).append(' ').append(m.getName());
            out.append('(').append(params(m)).append(") { ").append(value.isEmpty() ? "" : "return " + value + ";").append(" }\n");
        }
        return out.append("}\n").toString();
    }

    public static void main(String[] args) throws java.io.IOException {
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0], "ProbeLevel.java"), levelClass());
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0], "ProbeEntity.java"), entityClass());
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0], "ProbeLiving.java"), livingClass());
    }

    private GenProbe() {}
}

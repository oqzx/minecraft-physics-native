import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import net.minecraft.world.entity.ai.attributes.Attribute;
import net.minecraft.world.entity.ai.attributes.RangedAttribute;

/**
 * OracleAttributes from 1.8.8 to 1.15, which have no attribute registry: SharedMonsterAttributes holds them as static
 * fields, each named by its own getName. Same line: `release key default min max playerBase zombieBase`. The bases come
 * from each entity's own registerAttributes, run on an instance allocated without its constructor, whose attribute map
 * is built on first use: the instance has no level, and nothing else of it is touched.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleAttributesLegacy.java
 * java -cp "out;JARS" OracleAttributesLegacy RELEASE >> test/fixtures/attributes.tsv
 *
 * Before 1.14 the jar carries legacy-mappings' names (classpath.cjs). A class the mapping left in another package
 * fails the verifier on a protected read, though the bytecode is Mojang's: up to 1.12.2 add
 * -XX:+UnlockDiagnosticVMOptions -XX:-BytecodeVerificationRemote.
 */
public final class OracleAttributesLegacy {
    private static Object allocated(String name) throws ReflectiveOperationException {
        Field field = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        field.setAccessible(true);
        return ((sun.misc.Unsafe) field.get(null)).allocateInstance(Class.forName(name));
    }

    /** registerAttributes of the class and those above it, as the constructor would have run it. */
    private static Object registered(String name) throws ReflectiveOperationException {
        Object entity = allocated(name);
        // (Zombie's draws a reinforcement chance, an attribute no movement reads)
        Field random = net.minecraft.world.entity.Entity.class.getDeclaredField("random");
        random.setAccessible(true);
        random.set(entity, new java.util.Random(1L));
        for (Class<?> owner = entity.getClass(); owner != null; owner = owner.getSuperclass()) {
            for (Method method : owner.getDeclaredMethods()) {
                if (method.getName().equals("registerAttributes") && method.getParameterCount() == 0) {
                    method.setAccessible(true);
                    method.invoke(entity);
                    return entity;
                }
            }
        }
        throw new IllegalStateException(name);
    }

    private static String base(Object entity, Attribute attribute) throws ReflectiveOperationException {
        Method getter = net.minecraft.world.entity.LivingEntity.class.getMethod("getAttribute", Attribute.class);
        Object instance = getter.invoke(entity, attribute);
        return instance == null ? "-" : String.valueOf((double) instance.getClass().getMethod("getBaseValue").invoke(instance));
    }

    public static void main(String[] args) throws Exception {
        java.io.PrintStream out = System.out;
        java.io.PrintStream err = System.err;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        net.minecraft.server.Bootstrap.bootStrap();
        System.setErr(err);
        System.setOut(out);
        Object player = registered("net.minecraft.server.level.ServerPlayer");
        Object zombie = registered("net.minecraft.world.entity.monster.Zombie");
        Class<?> shared = Class.forName("net.minecraft.world.entity.monster.SharedMonsterAttributes");
        Field min = RangedAttribute.class.getDeclaredField("minValue");
        Field max = RangedAttribute.class.getDeclaredField("maxValue");
        min.setAccessible(true);
        max.setAccessible(true);
        java.util.List<String> lines = new java.util.ArrayList<>();
        for (Field field : shared.getFields()) {
            if (!Modifier.isStatic(field.getModifiers()) || !Attribute.class.isAssignableFrom(field.getType())) {
                continue;
            }
            Attribute attribute = (Attribute) field.get(null);
            boolean ranged = attribute instanceof RangedAttribute;
            lines.add(args[0] + "\t" + attribute.getName() + "\t" + attribute.getDefaultValue() + "\t" + (ranged ? min.get(attribute) : "-") + "\t" + (ranged ? max.get(attribute) : "-") + "\t" + base(player, attribute) + "\t" + base(zombie, attribute));
        }
        java.util.Collections.sort(lines);
        lines.forEach(System.out::println);
    }

    private OracleAttributesLegacy() {}
}

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.ai.attributes.Attribute;
import net.minecraft.world.entity.ai.attributes.AttributeSupplier;
import net.minecraft.world.entity.ai.attributes.DefaultAttributes;
import net.minecraft.world.entity.ai.attributes.RangedAttribute;

/**
 * Every attribute a jar registers, from its own registry, and what a player and a zombie start with, for
 * test/fixtures/attributes.tsv: `release key default min max playerBase zombieBase`, with - where the entity has none.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleAttributes.java
 * java -cp "out;JARS" OracleAttributes RELEASE >> test/fixtures/attributes.tsv
 */
public final class OracleAttributes {
    private static Object registry(String field) throws ReflectiveOperationException {
        for (String holder : new String[] {"net.minecraft.core.registries.BuiltInRegistries", "net.minecraft.core.Registry"}) {
            try {
                return Class.forName(holder).getField(field).get(null);
            } catch (ClassNotFoundException | NoSuchFieldException absent) {
                // the other one
            }
        }
        throw new IllegalStateException(field);
    }

    private static Method keyOf() {
        for (Method method : net.minecraft.core.Registry.class.getMethods()) {
            if (method.getName().equals("getKey") && method.getParameterCount() == 1 && method.getParameterTypes()[0] == Object.class) {
                return method;
            }
        }
        throw new IllegalStateException("getKey");
    }

    private static Object entity(Object types, Method key, String name) throws ReflectiveOperationException {
        for (Object type : (Iterable<?>) types) {
            if (key.invoke(types, type).toString().equals("minecraft:" + name)) {
                return type;
            }
        }
        throw new IllegalArgumentException(name);
    }

    /** From 1.20.5 a supplier is asked by Holder<Attribute>, before by the Attribute. */
    private static String base(AttributeSupplier supplier, Object attribute, Object registry) throws ReflectiveOperationException {
        Object key = attribute;
        for (Method method : AttributeSupplier.class.getMethods()) {
            if (method.getName().equals("getBaseValue") || method.getName().equals("getBaseValue".toString())) {
                Class<?> wanted = method.getParameterTypes()[0];
                if (!wanted.isInstance(attribute)) {
                    key = registry.getClass().getMethod("wrapAsHolder", Object.class).invoke(registry, attribute);
                }
                try {
                    return String.valueOf((double) method.invoke(supplier, key));
                } catch (java.lang.reflect.InvocationTargetException none) {
                    return "-";
                }
            }
        }
        throw new IllegalStateException("getBaseValue");
    }

    public static void main(String[] args) throws Exception {
        java.io.PrintStream out = System.out;
        java.io.PrintStream err = System.err;
        System.setOut(new java.io.PrintStream(java.io.OutputStream.nullOutputStream()));
        try {
            SharedConstants.class.getMethod("tryDetectVersion").invoke(null);
        } catch (NoSuchMethodException older) {
            // (the release has no such step)
        }
        Bootstrap.bootStrap();
        System.setOut(out);
        System.setErr(err);
        Method key = keyOf();
        Object attributes = registry("ATTRIBUTE");
        Object types = registry("ENTITY_TYPE");
        Method supplierOf = null;
        for (Method method : DefaultAttributes.class.getMethods()) {
            if (method.getName().equals("getSupplier")) {
                supplierOf = method;
            }
        }
        AttributeSupplier player = (AttributeSupplier) supplierOf.invoke(null, entity(types, key, "player"));
        AttributeSupplier zombie = (AttributeSupplier) supplierOf.invoke(null, entity(types, key, "zombie"));
        Field min = RangedAttribute.class.getDeclaredField("minValue");
        Field max = RangedAttribute.class.getDeclaredField("maxValue");
        min.setAccessible(true);
        max.setAccessible(true);
        java.util.List<String> lines = new java.util.ArrayList<>();
        for (Object attribute : (Iterable<?>) attributes) {
            Attribute kind = (Attribute) attribute;
            boolean ranged = kind instanceof RangedAttribute;
            lines.add(args[0] + "\t" + key.invoke(attributes, attribute) + "\t" + kind.getDefaultValue()
                + "\t" + (ranged ? min.get(kind) : "-") + "\t" + (ranged ? max.get(kind) : "-")
                + "\t" + base(player, attribute, attributes) + "\t" + base(zombie, attribute, attributes));
        }
        java.util.Collections.sort(lines);
        lines.forEach(System.out::println);
    }

    private OracleAttributes() {}
}

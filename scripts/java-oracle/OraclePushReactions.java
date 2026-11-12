import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;

/**
 * What PistonBaseBlock.isPushable reads of every block of a jar, for src/mineflayer/push-reactions.json: `RELEASE NAME REACTION
 * ENTITY`, REACTION the enum's name of the default state (NORMAL or PUSH_PULL, DESTROY or POPPED, BLOCK or IMMOVEABLE, PUSH_ONLY or
 * PUSH, IGNORE or IGNORE_ENTITY), ENTITY 1 for a block with a block entity. Read by reflection, on Mojang's names where the jar has
 * them and on yarn's before: no class of the jars is compiled against.
 *
 * javac -d out scripts/java-oracle/OraclePushReactions.java
 * java -cp "out;JARS" OraclePushReactions RELEASE
 */
public final class OraclePushReactions {
    private static Class<?> first(String... names) {
        for (String name : names) {
            try {
                return Class.forName(name, false, OraclePushReactions.class.getClassLoader());
            } catch (ClassNotFoundException | LinkageError e) {
                // the next name
            }
        }
        return null;
    }

    private static Method method(Class<?> type, String[] names, Class<?>... args) {
        for (String name : names) {
            try {
                return type.getMethod(name, args);
            } catch (NoSuchMethodException e) {
                // the next name
            }
        }
        return null;
    }

    private static Object field(Class<?> type, String... names) throws ReflectiveOperationException {
        for (String name : names) {
            try {
                return type.getField(name).get(null);
            } catch (NoSuchFieldException e) {
                // the next name
            }
        }
        return null;
    }

    public static void main(String[] args) throws Exception {
        Class<?> constants = first("net.minecraft.SharedConstants");
        if (constants != null) {
            Method detect = method(constants, new String[] {"tryDetectVersion", "createGameVersion"});
            if (detect != null) {
                detect.invoke(null);
            }
        }
        Class<?> boot = first("net.minecraft.server.Bootstrap", "net.minecraft.Bootstrap");
        method(boot, new String[] {"bootStrap", "initialize"}).invoke(null);
        Class<?> block = first("net.minecraft.world.level.block.Block", "net.minecraft.block.Block");
        Class<?> entityBlock = first("net.minecraft.world.level.block.EntityBlock", "net.minecraft.block.BlockEntityProvider", "net.minecraft.block.Block" + "EntityProvider");
        Class<?> registries = first("net.minecraft.core.registries.BuiltInRegistries", "net.minecraft.core.Registry", "net.minecraft.util.registry.Registry");
        Object registry = registries == null ? field(block, "REGISTRY") : field(registries, "BLOCK");
        if (registry == null) {
            registry = field(block, "REGISTRY");
        }
        Method key = null;
        for (String name : new String[] {"getKey", "getId", "getIdentifier"}) {
            for (Method m : registry.getClass().getMethods()) {
                if (m.getName().equals(name) && m.getParameterCount() == 1 && m.getParameterTypes()[0] == Object.class) {
                    key = m;
                }
            }
            if (key != null) {
                break;
            }
        }
        Method stateOf = method(block, new String[] {"defaultBlockState", "getDefaultState"});
        List<String> out = new ArrayList<>();
        for (Object b : (Iterable<?>) registry) {
            Object state = stateOf.invoke(b);
            Class<?> stateType = first("net.minecraft.world.level.block.state.BlockState", "net.minecraft.block.BlockState");
            Method onBlock = method(block, new String[] {"getPistonPushReaction", "getPistonBehavior"}, stateType);
            Method plain = method(block, new String[] {"getPistonBehavior"});
            Object reaction;
            if (onBlock != null) {
                reaction = onBlock.invoke(b, state);
            } else if (plain != null) {
                reaction = plain.invoke(b);
            } else if (method(state.getClass(), new String[] {"getPistonPushReaction", "getPistonBehavior"}) != null) {
                Method onState = method(state.getClass(), new String[] {"getPistonPushReaction", "getPistonBehavior"});
                onState.setAccessible(true);
                reaction = onState.invoke(state);
            } else {
                // 1.8: Material.getPistonInteractionType, 0 normal, 1 destroy, 2 block
                Object material = method(block, new String[] {"getMaterial"}).invoke(b);
                int type = (Integer) method(material.getClass(), new String[] {"getPistonInteractionType"}).invoke(material);
                reaction = new String[] {"NORMAL", "DESTROY", "BLOCK"}[type];
            }
            String name = String.valueOf(key.invoke(registry, b)).replace("minecraft:", "");
            out.add(args[0] + " " + name + " " + reaction + " " + (entityBlock != null && entityBlock.isInstance(b) ? 1 : 0));
        }
        out.forEach(System.out::println);
    }

    private OraclePushReactions() {}
}

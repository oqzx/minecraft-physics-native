import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.border.WorldBorder;

/** What ProbeLevel answers from, for OracleMove. */
public final class Hooks {
    static Map<BlockPos, BlockState> blocks = Map.of();
    static long gameTime;
    /** What Level.getEntities finds, by the box asked of it. */
    static java.util.List<net.minecraft.world.entity.Entity> near = new java.util.ArrayList<>();

    public static java.util.List near(Object box) {
        java.util.List<net.minecraft.world.entity.Entity> found = new java.util.ArrayList<>();
        for (net.minecraft.world.entity.Entity entity : near) {
            if (box instanceof net.minecraft.world.phys.AABB aabb && entity.getBoundingBox().intersects(aabb)) {
                found.add(entity);
            }
        }
        return found;
    }
    private static final WorldBorder BORDER = new WorldBorder();

    public static BlockState state(BlockPos pos) {
        return blocks.getOrDefault(pos, Blocks.AIR.defaultBlockState());
    }

    public static WorldBorder border() {
        return BORDER;
    }

    /**
     * Binds the static tags of a jar that keeps them in a StaticTagHelper (before 1.18.2) to empty ones, as a world's
     * load would bind them: a move there asks them, and nothing here holds a tagged block.
     */
    public static void bindEmptyTags() throws ReflectiveOperationException {
        Class<?> helper;
        try {
            helper = Class.forName("net.minecraft.tags.StaticTagHelper");
        } catch (ClassNotFoundException none) {
            return;
        }
        for (String name : new String[] {"BlockTags", "ItemTags", "FluidTags", "EntityTypeTags", "GameEventTags"}) {
            Class<?> tags;
            try {
                tags = Class.forName("net.minecraft.tags." + name);
            } catch (ClassNotFoundException none) {
                continue;
            }
            for (java.lang.reflect.Field field : tags.getDeclaredFields()) {
                if (helper.isAssignableFrom(field.getType())) {
                    field.setAccessible(true);
                    java.lang.reflect.Method reset = helper.getDeclaredMethod("resetToEmpty");
                    reset.setAccessible(true);
                    reset.invoke(field.get(null));
                }
            }
        }
    }

    /** Level.isClientSide, the field the level was allocated without. */
    static void clientSide(net.minecraft.world.level.Level level, boolean value) throws ReflectiveOperationException {
        java.lang.reflect.Field field = net.minecraft.world.level.Level.class.getDeclaredField("isClientSide");
        field.setAccessible(true);
        field.setBoolean(level, value);
    }

    /** An object to hand on: Block.fallOn asks for the source of a fall a bare Entity then ignores. */
    public static <T> T nothing(Class<T> type) {
        try {
            return allocate(type);
        } catch (ReflectiveOperationException impossible) {
            throw new IllegalStateException(impossible);
        }
    }

    /** An instance without its constructor: a Level's wants a server. */
    @SuppressWarnings("unchecked")
    static <T> T allocate(Class<T> type) throws ReflectiveOperationException {
        java.lang.reflect.Field field = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        field.setAccessible(true);
        return (T) ((sun.misc.Unsafe) field.get(null)).allocateInstance(type);
    }

    private Hooks() {}
}

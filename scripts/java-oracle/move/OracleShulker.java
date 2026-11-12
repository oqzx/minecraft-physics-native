import java.lang.reflect.Constructor;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Map;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ShulkerBoxBlock;
import net.minecraft.world.level.block.entity.ShulkerBoxBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;

/**
 * A jar's own ShulkerBoxBlockEntity opening and closing, each tick, for native/fixtures/java-oracle-shulker-RELEASE.txt, and
 * what its moveCollidedEntities does to an entity above the lid. `triggerEvent(1, n)` is the block event the server sends.
 *
 * `shulker facing closeAt ticks = (progress minX minY minZ maxX maxY maxZ)...` with the box at the origin cell,
 * `push facing x y z ticks = (x y z)...` with the entity's place after each tick, an open from the first.
 *
 * java -cp "JARS" scripts/java-oracle/move/GenProbe.java GEN
 * javac -cp JARS -d out GEN/*.java scripts/java-oracle/move/Hooks.java scripts/java-oracle/move/OracleMove.java scripts/java-oracle/move/OracleShulker.java
 * java -cp "out;JARS" OracleShulker > native/fixtures/java-oracle-shulker-RELEASE.txt
 */
public final class OracleShulker {
    private static ShulkerBoxBlockEntity make(BlockPos pos, BlockState state, ProbeLevel level) throws ReflectiveOperationException {
        ShulkerBoxBlockEntity entity = null;
        for (Constructor<?> ctor : ShulkerBoxBlockEntity.class.getConstructors()) {
            Class<?>[] types = ctor.getParameterTypes();
            if (types.length == 2 && types[0] == BlockPos.class) {
                entity = (ShulkerBoxBlockEntity) ctor.newInstance(pos, state);
            } else if (types.length == 0 && entity == null) {
                entity = (ShulkerBoxBlockEntity) ctor.newInstance();
            }
        }
        for (Method method : net.minecraft.world.level.block.entity.BlockEntity.class.getMethods()) {
            if (method.getName().equals("setLevel") && method.getParameterCount() == 1) {
                method.invoke(entity, level);
            } else if (method.getName().equals("setLevelAndPosition")) {
                method.invoke(entity, level, pos);
            }
        }
        return entity;
    }

    private static void tick(ShulkerBoxBlockEntity entity, ProbeLevel level, BlockPos pos, BlockState state) throws ReflectiveOperationException {
        for (Method method : ShulkerBoxBlockEntity.class.getMethods()) {
            if (method.getName().equals("tick")) {
                if (method.getParameterCount() == 4) {
                    method.invoke(null, level, pos, state, entity);
                } else {
                    method.invoke(entity);
                }
                return;
            }
        }
        throw new IllegalStateException("tick");
    }

    private static float progress(ShulkerBoxBlockEntity entity) throws ReflectiveOperationException {
        Field field = ShulkerBoxBlockEntity.class.getDeclaredField("progress");
        field.setAccessible(true);
        return field.getFloat(entity);
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
        Hooks.bindEmptyTags();
        ProbeLevel level = Hooks.allocate(ProbeLevel.class);
        Hooks.clientSide(level, true);
        Hooks.gameTime = 100L;
        BlockPos pos = BlockPos.ZERO;
        Direction[] ways = Direction.values();
        for (int way = 0; way < ways.length; way++) {
            BlockState state = Blocks.SHULKER_BOX.defaultBlockState().setValue(ShulkerBoxBlock.FACING, ways[way]);
            Hooks.blocks = Map.of(pos, state);
            for (int closeAt : new int[] {4, 12, 16}) {
                ShulkerBoxBlockEntity entity = make(pos, state, level);
                entity.triggerEvent(1, 1);
                StringBuilder line = new StringBuilder("shulker ").append(way).append(' ').append(closeAt).append(" 26 =");
                for (int t = 0; t < 26; t++) {
                    if (t == closeAt) {
                        entity.triggerEvent(1, 0);
                    }
                    tick(entity, level, pos, state);
                    AABB box = entity.getBoundingBox(state);
                    line.append(' ').append((double) progress(entity)).append(' ').append(box.minX).append(' ').append(box.minY).append(' ').append(box.minZ)
                        .append(' ').append(box.maxX).append(' ').append(box.maxY).append(' ').append(box.maxZ);
                }
                System.out.println(line);
            }
            double[][] places = {{0.5, 1.0, 0.5}, {1.3, 0.2, 0.5}, {-0.3, 0.2, 0.5}, {0.5, 0.2, 1.3}, {0.5, 0.2, -0.3}, {0.5, -1.8, 0.5}, {0.5, 1.5, 0.5}};
            for (double[] place : places) {
                ShulkerBoxBlockEntity entity = make(pos, state, level);
                OracleMove.Probe probe = new OracleMove.Probe(level);
                probe.boots();
                probe.setPos(place[0], place[1], place[2]);
                Hooks.near.clear();
                Hooks.near.add(probe);


                entity.triggerEvent(1, 1);
                StringBuilder line = new StringBuilder("push ").append(way).append(' ').append(place[0]).append(' ').append(place[1]).append(' ').append(place[2]).append(" 12 =");
                {
                    for (int t = 0; t < 12; t++) {
                        Hooks.gameTime += 1;
                        tick(entity, level, pos, state);
                        net.minecraft.world.phys.Vec3 at = probe.position();
                        line.append(' ').append(at.x).append(' ').append(at.y).append(' ').append(at.z);
                    }
                    System.out.println(line);
                }
                Hooks.near.clear();
                // the lid coming back: before 1.17 it pushes in either direction
                ShulkerBoxBlockEntity closing = make(pos, state, level);
                OracleMove.Probe rider = new OracleMove.Probe(level);
                rider.boots();
                rider.setPos(place[0], place[1], place[2]);
                closing.triggerEvent(1, 1);
                for (int t = 0; t < 11; t++) {
                    tick(closing, level, pos, state);
                }
                Hooks.near.add(rider);
                closing.triggerEvent(1, 0);
                StringBuilder back = new StringBuilder("pushclose ").append(way).append(' ').append(place[0]).append(' ').append(place[1]).append(' ').append(place[2]).append(" 12 =");
                for (int t = 0; t < 12; t++) {
                    Hooks.gameTime += 1;
                    tick(closing, level, pos, state);
                    net.minecraft.world.phys.Vec3 at = rider.position();
                    back.append(' ').append(at.x).append(' ').append(at.y).append(' ').append(at.z);
                }
                System.out.println(back);
                Hooks.near.clear();
            }
        }
    }

    private OracleShulker() {}
}

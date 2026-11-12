import java.lang.reflect.Method;
import java.util.Random;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.phys.Vec2;

/**
 * The move vector of a release from 1.21.5, through a jar's own Vec2 and LocalPlayer.modifyInputSpeedForSquareMovement, in the
 * order of LocalPlayer.modifyInput (26.3 LocalPlayer.java:690-705; 1.21.5 :624-640), for native/fixtures/java-oracle-input.txt:
 * `RELEASE input x z item sneak using passenger slowly = x z`. An analog vector is what no key gives: under and over one,
 * opposite signs, signed zeros, the diagonal, and values a float's ulp apart around the ones the keys give.
 *
 * modifyInput is an instance method of a LocalPlayer no harness can make, so the scales are the jar's Vec2.scale in its order and
 * the instance's predicates are arguments. The order itself is held to vanilla by the recordings (pl_attack_use_diagonal_shield).
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleInput.java
 * java -cp "out;JARS" OracleInput RELEASE >> native/fixtures/java-oracle-input.txt
 */
public final class OracleInput {
    private static Method square;

    private static Vec2 modify(Vec2 input, float item, float sneak, boolean using, boolean passenger, boolean slowly) throws ReflectiveOperationException {
        if (input.lengthSquared() == 0.0F) return input;
        Vec2 scaled = input.scale(0.98F);
        if (using && !passenger) scaled = scaled.scale(item);
        if (slowly) scaled = scaled.scale(sneak);
        return (Vec2) square.invoke(null, scaled);
    }

    public static void main(String[] args) throws ReflectiveOperationException {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        square = LocalPlayer.class.getDeclaredMethod("modifyInputSpeedForSquareMovement", Vec2.class);
        square.setAccessible(true);
        float[] values = {0.0F, -0.0F, 1.0F, -1.0F, 0.5F, -0.5F, 0.25F, 2.0F, -2.5F, 0.70710677F, Math.nextUp(1.0F), Math.nextDown(1.0F), 1.0E-5F, 1.0E-4F, Math.nextDown(1.0E-4F), 0.33F, -0.77F, 1000.0F};
        float[] items = {0.2F, 0.0F, 0.75F, 1.0F};
        float[] sneaks = {0.3F, 0.0F, 0.15F, 1.0F};
        Random random = new Random(20261113L);
        for (float x : values) {
            for (float z : values) {
                for (int i = 0; i < 6; i++) {
                    boolean using = i >= 2 && i != 4;
                    boolean passenger = i == 3;
                    boolean slowly = i >= 4 || i == 1;
                    float item = items[random.nextInt(items.length)];
                    float sneak = sneaks[random.nextInt(sneaks.length)];
                    Vec2 out = modify(new Vec2(x, z).normalized(), item, sneak, using, passenger, slowly);
                    System.out.println(args[0] + " input " + x + " " + z + " " + item + " " + sneak + " " + (using ? 1 : 0) + " " + (passenger ? 1 : 0) + " " + (slowly ? 1 : 0)
                        + " = " + out.x + " " + out.y);
                }
            }
        }
    }
}

import java.io.OutputStream;
import java.io.PrintStream;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.Random;
import net.minecraft.server.Bootstrap;
import net.minecraft.util.Mth;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.monster.zombie.Zombie;
import net.minecraft.world.entity.projectile.Projectile;
import net.minecraft.world.entity.projectile.throwableitemprojectile.Snowball;
import net.minecraft.world.phys.Vec3;

/**
 * Runs vanilla's Entity.getInputVector over a fixed seed, with the values its locals take, for
 * native/fixtures/java-ir-RELEASE.txt: `input x y z speed yRot = length sin cos x y z`, doubles in Double.toString's
 * shortest form. test/java-ir.test.cjs holds scripts/java-ir.cjs to these, native/src/oracle_tests.rs holds the engine
 * to them. `sin` and `cos` are taken with the jar's own Mth, so a table or a cast of the release's comes with them.
 * A case that returns early has `-` for its sin and cos: the method never computes them.
 *
 * `friction blockFriction speed = result` is LivingEntity.getFrictionInfluencedSpeed on the ground, called on a zombie
 * with no constructor run and only the two fields it reads set.
 *
 * `shoot x y z pow = mx my mz yRot xRot` is Projectile.shoot of a snowball built the same way, uncertainty 0 so that its
 * random spread is 0: the motion it writes and the rotation it reads back from that motion.
 *
 * javac -cp JARS -d out scripts/java-oracle/OracleIr.java
 * java -cp "out;JARS" OracleIr > native/fixtures/java-ir-RELEASE.txt
 */
public final class OracleIr {
    public static void main(String[] args) throws Exception {
        PrintStream out = System.out;
        System.setOut(new PrintStream(OutputStream.nullOutputStream()));
        net.minecraft.SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        System.setOut(out);
        Method input = Entity.class.getDeclaredMethod("getInputVector", Vec3.class, float.class, float.class);
        input.setAccessible(true);
        Random random = new Random(20261106L);
        for (int i = 0; i < 400; i++) {
            double scale = i % 8 == 0 ? 1.0E-4 : i % 8 == 1 ? 3.0E-4 : 2.0;
            Vec3 vector = new Vec3((random.nextDouble() - 0.5) * scale, (random.nextDouble() - 0.5) * scale, (random.nextDouble() - 0.5) * scale);
            float speed = random.nextFloat() * 0.3F;
            float yRot = (random.nextFloat() - 0.5F) * 720.0F;
            Vec3 result = (Vec3) input.invoke(null, vector, speed, yRot);
            double length = vector.lengthSqr();
            String sin = length < 1.0E-7 ? "-" : Float.toString(Mth.sin((double) (yRot * 0.017453292F)));
            String cos = length < 1.0E-7 ? "-" : Float.toString(Mth.cos((double) (yRot * 0.017453292F)));
            System.out.println("input " + vector.x + " " + vector.y + " " + vector.z + " " + speed + " " + yRot + " = " + length + " " + sin + " " + cos
                + " " + result.x + " " + result.y + " " + result.z);
        }
        Field theUnsafe = sun.misc.Unsafe.class.getDeclaredField("theUnsafe");
        theUnsafe.setAccessible(true);
        Object zombie = ((sun.misc.Unsafe) theUnsafe.get(null)).allocateInstance(Zombie.class);
        Field onGround = Entity.class.getDeclaredField("onGround");
        onGround.setAccessible(true);
        onGround.setBoolean(zombie, true);
        Field speedField = LivingEntity.class.getDeclaredField("speed");
        speedField.setAccessible(true);
        Method friction = LivingEntity.class.getDeclaredMethod("getFrictionInfluencedSpeed", float.class);
        friction.setAccessible(true);
        float[] edges = {0.6F, Math.nextDown(0.6F), Math.nextUp(0.6F), 0.98F, 0.989F, 0.8F, 0.4F, 0.1F};
        for (int i = 0; i < 200; i++) {
            float blockFriction = i < edges.length ? edges[i] : random.nextFloat();
            float speed = random.nextFloat() * 0.3F;
            speedField.setFloat(zombie, speed);
            System.out.println("friction " + blockFriction + " " + speed + " = " + friction.invoke(zombie, blockFriction));
        }
        Projectile snowball = (Projectile) ((sun.misc.Unsafe) theUnsafe.get(null)).allocateInstance(Snowball.class);
        Field source = Entity.class.getDeclaredField("random");
        source.setAccessible(true);
        source.set(snowball, RandomSource.create(1L));
        Field motion = Entity.class.getDeclaredField("deltaMovement");
        motion.setAccessible(true);
        for (int i = 0; i < 200; i++) {
            double x = (random.nextDouble() - 0.5) * 4.0;
            double y = (random.nextDouble() - 0.5) * 4.0;
            double z = (random.nextDouble() - 0.5) * 4.0;
            float pow = random.nextFloat() * 3.0F;
            snowball.shoot(x, y, z, pow, 0.0F);
            Vec3 moved = (Vec3) motion.get(snowball);
            System.out.println("shoot " + x + " " + y + " " + z + " " + pow + " = " + moved.x + " " + moved.y + " " + moved.z + " " + snowball.getYRot() + " " + snowball.getXRot());
        }
    }
}

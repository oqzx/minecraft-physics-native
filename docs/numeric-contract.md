# Numbers

What the engine promises of its arithmetic, and how each promise is checked.

## Comparison target

The target is HotSpot 25.0.3 running the real jars, on x86-64 Windows. `native/fixtures/java-oracle-*.txt` are that
output. Comparisons are by bits, so signed zeros count. No test takes a tolerance.

| operation | promise | evidence |
|---|---|---|
| `+ - * /`, float and double, `sqrt` | bit equal: IEEE and Java agree | every fixture |
| `Mth.sin`, `Mth.cos` (table) | bit equal | `sintable` and `sin`, `cos` |
| `Math.sin`, `Math.cos`, `Mth.sin`, `Mth.cos` past a million radians, up to the largest double, signed zero, subnormals, infinities and NaN | bit equal on `java-oracle-trig-large.txt` (a NaN as a NaN), 1.16.5, 1.21.10, 1.21.11 and 26.3 | `trig_at_very_large_and_non_finite_arguments_matches_the_jars` |
| `Math.sin`, `Math.cos` | bit equal on x86_64 HotSpot, on the JDKs below | `trig.rs`, Intel LIBM's code, the section below |
| `Math.atan2`, `Math.acos`, `Math.asin` | bit equal on 10,160,354 inputs each, on the JDKs below: fdlibm's through `trig::atan2` and the `libm` crate, never the platform's | `TrigDigest arc`, `atan-tables` |
| floor, ceil, round, clamp, frac, wrap | bit equal, by release | `floor`, `ceil`, `lfloor`, `round`, `clamp`, `clampf` at integers, half ties, extrema, -0.0, NaN |
| a whole trajectory | bit equal | `walk` lines, recordings |

## Math.sin and Math.cos

`trig.rs` is a port of Intel LIBM's sin and cos from Android bionic's `s_sin.S` and `s_cos.S` (BSD-3-clause, `THIRD_PARTY_NOTICES`),
the code HotSpot's x86_64 stubs carry. It is not correctly rounded, and neither is Java. It takes no `mul_add`, and every
addition stays where the assembly has it.

`scripts/java-oracle/TrigDigest.java` and `scripts/java-oracle/wasm/src/trig_digest.rs` print the same lines, digests of the raw
bits, so `diff` compares Java with Rust. The sets come from `scripts/java-oracle/trig-vectors.cjs`.

| set | count per function | Rust against the JDK |
|---|---|---|
| every finite float, both zeros and subnormals included (`floats`) | 4,278,190,080 | equal on 21.0.11 and 25.0.3 |
| the two `Mth` tables as `(float)`, by the jars' formulas (`tables`) | 65,536 each | equal |
| `PrimedTnt`'s kick, angles drawn as `LegacyRandomSource` does (`kick`, 4 seeds) | 8,000,000 | equal |
| `Mth.atan2`'s two tables (`atan-tables`) | 257 each | equal |
| random floats and doubles of every exponent, 1,000,000 large ones (`file`) | 22,000,000 | equal |
| doubles nearest multiples of pi/2, powers of two, the ends of the main path, the rounding of N, NaNs (`file`) | 160,361 | equal |

`-Xint` on 21.0.11 and C1 alone on 25.0.3 give the same digests on all but `floats`. On 25.0.3 a run with
`-XX:-UseLibmIntrinsic` differs on 3% of angles, so the check can fail; interpreter, C1, C2 and `-Xcomp`, with AVX off or FMA
off, do not differ. `native/fixtures/java-oracle-trig-bits.txt` keeps 2,486 of the hardest with their bits, and a NaN by its
raw bits.

`wasm32-wasip1` and `wasm32-wasip1-threads` under Node's WASI print the same digests on every set above, `floats` included. On
the threads build `kick` ran one seed and `arc` two sets. x86_64 Linux native on Temurin 21.0.13 prints the same digests, Rust against the JDK, on all 529 lines; the `boundaries` word file written by Node differs between the two machines in 12 words (Node 24.15 and 24.21), and the Windows file gives the Windows digests on Linux. macOS did not run them.

**The JDK.** Java 21.0.11 and 25.0.3 agree on all of the above. The recordings run on 21 for most releases and on 25 for 26.x.
**Older JDKs.** Each release follows the Math of the JVM its version json declares, below; `11.0.32` and `17.0.21` give these digests on every line but one, the `arc boundaries` atan2 digest, where they return other NaN bits for `atan2(NaN, NaN)` (4 of 143,318 pairs, all NaN). Java 8 is other code: see *Java 8* below.

## The JVM of each release (B315)

Mojang's version json gives each release a Java major (`javaVersion.majorVersion` of each client's `version.json`
as `scripts/fetch-jar-inputs.cjs` fetches it, every one matching its sha1 in `generated/mojang-releases.json`): 8 for
1.8.8 to 1.16.5, 16 for 1.17 and 1.17.1, 17 for 1.18 to 1.20.4, 21 for 1.20.5 to 1.21.11 and 25 for 26.x. The engine follows
that JVM's `Math` at every direct call (`math::math_java`, `math_sin`, `math_cos`); `setMathJava` (N-API) puts every release
on one major for the process. The recorder runs every release on Java 21 or 25 (each build's `jdk` in
`recordings/<v>/builds`), so the recorder's `replay/engine.ts` sets 21, and no recording shows Java 8: the Java 8 mode stands on the
digests and measurements below. Replays of the 36 boat and 21 minecart recordings of 1.8.8 match under either mode.

Direct `Math` and `StrictMath` calls of `net.minecraft` (`source-cache`, `legacy-src`), and what reaches simulated state:

| Vanilla call | Releases | Engine | JVM | Evidence |
|---|---|---|---|---|
| `Mth.SIN`, `(float)Math.sin` of each index (1.8.8 `Mth.java:395`, 1.21.11 `:36`) | all | `math::sine_table`, `legacy_sine_table`, `trig::sin` | declared | `tables` digest equal on 8, 11, 17, 21 and 25: one table for all |
| `Mth.atan2`'s `ASIN_TAB` (`Math.asin`) and `COS_TAB` (`Math.cos`) (1.8.8 `Mth.java:404-410`) | all | `math::atan_tables(java)` | declared | `atan-tables`: the Java 8 table gives Temurin 8's digest (`f19212d48c40b659`), the other 21's. Java 8 differs in entries 61, 182 and 186; 0 to 181 are reached, so entry 61, by one unit |
| `Mth.atan2` itself: a cart's yaw (1.8.8 `AbstractMinecart`, 26.3 `OldMinecartBehavior.java:67-84`), the 1.8.8 boat's (`Boat`), the dragon's steering (`EnderDragon`) | all | `math::atan2_vanilla(.., release)` | declared | 3,885,010 cart moves: Java 8's angle differs from 21's in 2,649 (1 or 2 units), the float yaw in none. 1,000,000 dragon headings: 1,335 angles, 223 `180 - a * 57.29...` doubles, up to 16 units |
| `Boat.positionRider`, `Math.cos` and `sin` of `yRot * Math.PI / 180.0` (1.8.8 `Boat.java:372-373`) | 1.8.8 | `vehicle::boat_passenger_position` | 8 | `seat-188` below |
| the paddle, `Math.sin` and `cos` of `(yaw - strafe * 90F) * (float)Math.PI / 180F` (1.8.8 `Boat.java:265-267`) | 1.8.8 | `vehicle::tick_legacy_boat` | 8 | `cart-188` below |
| the rider's push of a cart, `Math.sin` and `cos` of `yRot * (float)Math.PI / 180F`, and of `yRot * (float)(Math.PI / 180.0)` from 1.9 (1.8.8 `AbstractMinecart.java:446-447`, 1.13.2 `:471-472`) | 1.8.8 to 1.13.2 | `minecart.rs`, the rider's push | 8 | `cart-188`, `cart-194` below |
| the glide's `Math.cos(pitch)` (1.18.2 `LivingEntity.java:2082`) | 1.18.2 on | `simulation::glide` | 17, 21, 25 | 17 gives 21's digests on every sin and cos line |
| JOML `Math.sin` of the passenger's yaw (1.20.4 `Entity.java:1880`) | 1.20.2 to 1.20.4 | `vehicle::boat_passenger_position` | 17 | the same |
| `Math.acos` of the minor horizontal collision (1.18.2 `LocalPlayer.java:1006`) | 1.18 on | `simulation::horizontal_collision_is_minor` | 17, 21, 25 | `arc` acos and asin digests equal on 8, 11, 17, 21, 25 |
| `Math.atan2` of `NewMinecartBehavior` (1.21.11 `:171-267`) | 1.21.2 on | `minecart.rs`, `trig::atan2` | 21, 25 | `arc` digests |
| `PrimedTnt`'s kick (`Math.random`), a dropped item's spread (`Player.drop`), `EyeOfEnder`'s flight | all | not computed: the server's motion is an input | - | - |
| AI and visuals: `RandomLookAroundGoal`, `Guardian`, `Fox`, `DolphinJumpGoal`, `RandomPos`, `LongJumpUtil`, `Ravager`, the dragon's phases (`acos`), its wing beat (`pow`), `Boat.bubbleAngle`, `Player.getRopeHoldPosition`, `RemotePlayer` (`atan`) | various | not modelled | - | - |

The Java 16 of 1.17 and 1.17.1 was not run; its `Math.sin` and `cos` are the same Intel LIBM stubs as 11, 17 and 21 (JDK 9 on), and
1.17 has no direct call outside the two `Mth` tables.

### Java 8

HotSpot 8 has no Intel LIBM stub. Its interpreter and both compilers run `MacroAssembler::trigfunc`: the x87 `fsin` or
`fcos` when |x| <= pi/4, else `SharedRuntime::dsin` or `dcos`, fdlibm 5.3. `trig::java8` does the same: past pi/4 a port
of fdlibm 5.3 (Sun's notice, `THIRD_PARTY_NOTICES`), within it the true value rounded to a 64-bit significand and then to
a double, both to nearest even, from a double-double series.

`scripts/java-oracle/TrigCallSites.java` writes each call site's arguments (2,644,001 float yaws: every hundredth of a
degree in [-720, 720], 2,000,000 random in [-360, 360), 500,000 in [-3600, 3600)) and the JVM's values. Temurin 1.8.0_512
against 21.0.13, x86_64 Linux on an AMD EPYC 9354P (host `recorder`):

| set | values (sin and cos) | Java 8 differs from 21 | Java 8 past pi/4 equals its StrictMath | `trig::java8` differs from Java 8 |
|---|---|---|---|---|
| `cart-188` | 5,288,002 | 160,339, each by one unit (3.0%) | all 4,755,358 | 0 past pi/4, 3 of 532,644 within |
| `cart-194` | 5,288,002 | 160,588, one unit | all 4,757,504 | 0 past pi/4, 2 of 530,498 within |
| `seat-188` | 5,288,002 | 160,374, one unit | all 4,757,500 | 0 past pi/4, 4 of 530,502 within |

On the TrigDigest sets, run with `TRIG_JVM=8 trig-digest` against Temurin 8's digests, `trig::java8` gives Java 8's bits on
every line whose arguments all lie past pi/4 or below 2^-6: every finite float of exponent below -6 or from 0 up (the
`floats` lines but 121 to 126 and 377 to 382), `tables`, `atan-tables`, `file` `hard`, `boundaries`, `float-turn`,
`float-far`, `float-any`, `double-any`, `double-large` and `double-edge`, and the `kick` seed -4962768465676381896. The
other 18 sin and cos lines differ, each holding arguments in [2^-6, pi/4] (`floats` 121 to 126 and 377 to 382, `kick` 0, 1
and 12345, `float-quarter`, `double-quarter`, `double-main`); the 19th, `arc boundaries`, is the atan2 NaN above. Value by
value, the call-site sets show what is left there: 9 values. Each of the 9 lies within a 64-bit unit of the midpoint of two
doubles, where the processor's own `fsin` result decides; that is not a function this code can give, and it is the
processor's and the precision control's. Linux starts threads at 64-bit x87 precision, Windows at 53, and Intel's
microcode is other code: neither Windows nor Intel ran Java 8 here (`docs/limitations.md`).

**ARM.** arm64 HotSpot runs other code. tickwright measured it differing from x86_64 on about 1 angle in 6 (Oracle JDK 24, Apple
silicon); it was not measured here. This engine's result is x86_64's on every target.

**Correct rounding.** Java differs from the correctly rounded value on about 0.12% of angles in every band: 6,194 and 5,931 of
5,000,000 random float angles in [0, pi/2] for sin and cos, 3,643 and 3,606 of 3,000,000 in (pi/2, 2pi], and 70 to 100 in each
65,535 of a table's. V8's fdlibm differs from Java on about 3%.

**Not finite.** An infinity gives x86's default NaN, `0xfff8000000000000`, and a NaN comes back quieted with its sign and payload.
Those are Java's raw bits on x86_64, and `trig.rs` writes them as bits, so that WebAssembly, where the sign and payload of a NaN
from arithmetic are not fixed, returns the same.

**Rounding mode.** The engine never sets one, and Rust and WebAssembly have no way to read one. Native `+ - *` take the
processor's mode, so a host that leaves the FPU out of round-to-nearest gives wrong results here and everywhere else in the engine.
WebAssembly always rounds to nearest.

**Mth tables.** Both are built with these functions and their digest is Java's. The floats of the two are the same, though 9,570
of the 65,536 angles differ. The adapter's tables in `src/mineflayer/mth.ts` come from V8's `Math.sin`; as floats they have the
same digest, so no table entry depends on the difference.

## Targets

`scripts/java-oracle/wasm` has two programs, each built for the host and for `wasm32-wasip1`, and both targets must give the
same digest. `oracle-wasm` is the math vectors. `oracle-engine` links the real crate, built without its `bindings` feature
so that no emnapi is needed, and runs every `walk` line through `versioned::dispatch`: 8 releases, 512 walks and 26228 ticks,
none differing from Java, with digest `55bc5299d48e37b1` on both. The N-API layer itself does not run under WASM here, and
`wasm32-wasip1-threads`, Linux and macOS did not run. Building for WASM takes the rustup toolchain's binaries directly: the
`rustc` on `PATH` has no wasm std.

## Types of the high-impact paths

Order of operations is preserved as written: do not replace an expression with an equal-looking one.

| path | Java | Rust |
|---|---|---|
| friction speed, 1.14+ | `speed * (0.21600002F / (f * f * f))`, all float | `speed as f32 * (0.21600002_f32 / (f * f * f))`, widened at the end |
| friction test, 26.2+ | `(double)f > 0.6`: 0.6F is above 0.6 | `f64::from(friction) <= 0.6` returns the speed |
| jump power | `(float)strength * factor + boost` in float, double boost on 1.17.1-1.19.4 and before 1.14 | `jump_power` |
| sprint jump before 1.20.5 | `-sin * 0.2F`, float, then added to a double | `sprint_jump_push` |
| sprint jump from 1.20.5 | `(double)-sin * 0.2`, double | `sprint_jump_push` |
| jump floor from 1.20.5 | `jumpPower <= 1.0E-5F` is no jump | `power as f32 <= 1.0e-5` |
| yaw | float degrees `* 0.017453292F`, then widened | `yaw_radians`, `yaw` holds the widened value |
| fall distance | see `accumulated_fall_distance` for each release's cast | float before 1.21.5, double from it |
| dimensions | `width * scale`, float, `/ 2.0F`, then `x - (double)half` | `half_width` holds the float widened |
| input | `xxa *= 0.98F`, float, then widened | `scale_input` |

Rust does not fuse or reorder floating-point operations without an explicit flag, and the crate sets none. An
equal-looking change that moves a rounding point fails `a_living_entity_matches_the_jar`.

1.17.1 and 1.18.2 numeric boundaries come from the jars for the paths above where a fixture is listed. `E:/pr/mc-decompiled` has
no tree of these releases. Their source record is their own client jar through vineflower (`docs/source-records.md`), and a path
with no fixture stays unverified there.

## Fixtures

`scripts/java-oracle/living/OracleLiving.java` runs a real LivingEntity on 1.18.2, 1.20.4, 1.20.5, 1.21.5, 1.21.11, 26.2 and
26.3: friction around 0.6F, jumps, the 1.0E-5F jump rejection by the JUMP_STRENGTH attribute, scaled dimensions, pose sizes, and 54 walks of 60 ticks on stone, ice and blue ice at the origin,
at -1024 and at 29999000, with float speeds and turning yaws. A difference names its tick and field. See
`native/src/oracle_living_tests.rs`. Older releases are covered by the recordings, as static tags are unbound there.

## The 1.0E-5F tolerances

`Mth.equal` and `BlockGetter`'s two squares are compared with the jars at their call sites, and each is shown to hold by
breaking it. Exact-length travels, found by a seeded search in `OracleEpsilon.java`, are what make the strictness visible:
ulps of the root never land on the square. Those lines also found the 1.21.4 walk, which alone starts from the box's corner
with no 1.0E-7 at its ends.

| threshold | spelled as a double instead | `<=` for `<` |
|---|---|---|
| `Mth.square(1.0E-5F)`, from 1.21.9 | 304 lines differ | 24 lines differ |
| `Mth.square(0.99999F)`, to 1.21.8 | 346 lines differ | 44 lines differ |
| `Mth.equal`, `Math.abs(b - a) < 1.0E-5F` | 624 of 1872 walls on 1.18.2, and its own lines | the same |

## Exceptional values

| where | behaviour | failure leaves |
|---|---|---|
| a public position, size, box or config not finite or out of range | refused with the field's name | the state as it was |
| a field of a horse, ridden mob, boat, projectile, arrow, trident, moving entity, entity push, moving piston or entity descriptor not finite or out of its range | refused as `<kind> contains invalid values: <field> is <value>, it must be <rule>`, the first such field by name (`FieldCheck`, `native/src/napi_api.rs`) | the state as it was |
| a motion not finite, from 1.21.11 | `setDeltaMovement` refuses it, as vanilla: the entity keeps its motion | no error |
| a NaN motion, before 1.21.11 | as vanilla: the move drops it, a NaN x or z ends at 0.0 by its collision, a NaN y stays NaN and does not move the entity | no error |
| an infinite motion, before 1.21.11 | refused as "the tick's move". This departs from vanilla: 1.21.10 walks it to the border or to an infinite position, and on 1.18.2 the jar's collision walk does not end | the state as it was |
| a finite motion too large to walk (`MAX_WALKED_CELLS`) | refused by name: "the tick's move" | the state as it was, through `NativePlayer` too |
| a motion past what one adapter sync reads (`MAX_SYNCED_CELLS`), which from 1.21.9 a velocity packet can carry (26.3 LpVec3.java:17, 70: 1.7179869183E10 blocks a tick) | `NativePhysics.simulate`, the boat's and ridden mob's ticks and piston pushes, `NativeMinecartPhysics` (a cart of a scene sits its tick out in its place; `hurt` lands on the cart at rest), `NativeMovingEntityPhysics` and `NativeProjectilePhysics` skip the tick and say `moveRefused`: on the state a tick moves in place, else on the state or result returned. This departs from vanilla, which walks it; on jars around 1.18.2 such a walk does not end, so no recording stands for it | the state as it was, but for a velocity of zero and `age`: no error |
| a value that overflows to infinity inside a tick | follows the Java arithmetic and the row of its release | no error |
| `NaN` at a floor or ceil | 0 and the int extremes of the release, as `Mth` does | no error |
| a counter | a Java int: `tickCount` wraps, by `wrapping_add` | no error |
| a double to int | saturating, as a Java cast | no error |
| a fractional or out-of-range cell at a world write | refused: N-API would truncate it | the world as it was |
| a fraction, `NaN`, an infinity or a value past its type in any whole-number field or argument of a public call | refused by `src/whole-numbers.ts`, which names the field: N-API would truncate it, wrap it or read 0. Its table is generated from the Rust types (`scripts/generate-whole-fields.cjs`), and a binding the caller gives the adapter (`options.binding`, `NativeWorldSync`, `ShulkerBoxTracker`) gets the same check through `guardNativeBinding`. Only a raw addon called directly, outside the package, has none | the state as it was: the addon is not called |
| a minecart's `randomSeed` that is not whole, negative or past 2^53 | refused by name: a double holds no more | the state as it was |
| a double the engine holds as a float | rounded by its reader, as vanilla holds it: `docs/field-maps.md` names each | no error |

Validation runs before the tick mutates anything. `validate_tick_state` covers the object path and `validate_input` the
packed one, and a packed tick that fails restores what it took. A failure names its phase in its message: the operation
and the field, or "the tick's move". The object and packed paths reach the same refusal, which
`test/numeric-limits.test.cjs` shows.

Modern rejection of non-finite motion is not applied to older releases.

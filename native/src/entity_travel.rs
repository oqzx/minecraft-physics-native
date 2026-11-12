//! Which travel the engine has for an entity kind, by release: the inventory of the kinds that
//! move through fluids or are steered through them. A kind with no travel here is refused by
//! name, so it never runs on the player's equations (LivingEntity.travel has an override or an
//! AI move control for each of these: 26.3 WaterAnimal.java, AbstractFish.java:115-123,
//! Squid.java, Dolphin.java, Axolotl.java, Turtle.java, Guardian.java).

use crate::version::MinecraftVersion;

/// The path that moves a kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TravelFamily {
    /// The local player's tick (simulateVersionedTick, NativePlayer), water and lava included.
    Player,
    /// Boat.tick (simulateVersionedBoatTick), its own status and water level kept between ticks.
    Boat,
    /// LivingEntity.travelRidden of a mob a player steers (simulateVersionedHorseTick).
    Ridden,
    /// LivingEntity.travel of a mob from where its AI left it (simulateVersionedMobTravel).
    Mob,
    /// EnderDragon.aiStep, which moves the dragon itself and never calls LivingEntity.travel
    /// (simulateVersionedDragonTick): 1.8.8 EnderDragon.java:102-220, 26.3 EnderDragon.java:146-231.
    Dragon,
    /// A kind of the game with no path here.
    Unsupported,
    /// No LivingEntity: a minecart, an item, a projectile, each with its own tick, none with a travel.
    Entity,
}

impl TravelFamily {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Player => "player",
            Self::Boat => "boat",
            Self::Ridden => "ridden",
            Self::Mob => "mob",
            Self::Dragon => "dragon",
            Self::Unsupported => "unsupported",
            Self::Entity => "entity",
        }
    }
}

type Release = (u16, u16, u16);

/// The kinds by the first release whose registry has them, from each release's EntityType source.
/// (Before 1.11 a horse's variants and the elder guardian are data of one type, 1.10.2
/// HorseBaseEntity, GuardianEntity: they count from those types' first release.)
const INVENTORY: &[(&str, TravelFamily, Release)] = &[
    ("player", TravelFamily::Player, (1, 8, 0)),
    ("pig", TravelFamily::Ridden, (1, 8, 0)),
    ("horse", TravelFamily::Ridden, (1, 8, 0)),
    ("donkey", TravelFamily::Ridden, (1, 8, 0)),
    ("mule", TravelFamily::Ridden, (1, 8, 0)),
    ("skeleton_horse", TravelFamily::Ridden, (1, 8, 0)),
    ("zombie_horse", TravelFamily::Ridden, (1, 8, 0)),
    ("llama", TravelFamily::Ridden, (1, 11, 0)),
    ("trader_llama", TravelFamily::Ridden, (1, 14, 0)),
    ("strider", TravelFamily::Ridden, (1, 16, 0)),
    ("camel", TravelFamily::Ridden, (1, 19, 3)),
    ("happy_ghast", TravelFamily::Ridden, (1, 21, 6)),
    ("camel_husk", TravelFamily::Ridden, (1, 21, 11)),
    ("nautilus", TravelFamily::Ridden, (1, 21, 11)),
    ("zombie_nautilus", TravelFamily::Ridden, (1, 21, 11)),
    ("squid", TravelFamily::Mob, (1, 8, 0)),
    ("guardian", TravelFamily::Mob, (1, 8, 0)),
    ("elder_guardian", TravelFamily::Mob, (1, 8, 0)),
    // (1.8.8 EntityType.java:266; 1.11.2 :207; 1.13.2 :240; 1.19 :159)
    ("ghast", TravelFamily::Mob, (1, 8, 0)),
    ("vex", TravelFamily::Mob, (1, 11, 0)),
    ("phantom", TravelFamily::Mob, (1, 13, 0)),
    ("allay", TravelFamily::Mob, (1, 19, 0)),
    ("cod", TravelFamily::Mob, (1, 13, 0)),
    ("salmon", TravelFamily::Mob, (1, 13, 0)),
    ("pufferfish", TravelFamily::Mob, (1, 13, 0)),
    ("tropical_fish", TravelFamily::Mob, (1, 13, 0)),
    ("dolphin", TravelFamily::Mob, (1, 13, 0)),
    ("turtle", TravelFamily::Mob, (1, 13, 0)),
    ("drowned", TravelFamily::Mob, (1, 13, 0)),
    ("glow_squid", TravelFamily::Mob, (1, 17, 0)),
    ("axolotl", TravelFamily::Mob, (1, 17, 0)),
    ("tadpole", TravelFamily::Mob, (1, 19, 0)),
    ("frog", TravelFamily::Mob, (1, 19, 0)),
    // (1.8.8 EntityType.java; PolarBear 1.10.2, Parrot 1.12.2, Bee 1.15, SulfurCube 26.3)
    ("blaze", TravelFamily::Mob, (1, 8, 0)),
    ("bat", TravelFamily::Mob, (1, 8, 0)),
    ("zombie", TravelFamily::Mob, (1, 8, 0)),
    ("cow", TravelFamily::Mob, (1, 8, 0)),
    ("spider", TravelFamily::Mob, (1, 8, 0)),
    ("cave_spider", TravelFamily::Mob, (1, 8, 0)),
    ("wither", TravelFamily::Mob, (1, 8, 0)),
    ("armor_stand", TravelFamily::Mob, (1, 8, 0)),
    ("polar_bear", TravelFamily::Mob, (1, 10, 0)),
    ("parrot", TravelFamily::Mob, (1, 12, 0)),
    ("bee", TravelFamily::Mob, (1, 15, 0)),
    ("sulfur_cube", TravelFamily::Mob, (26, 2, 0)),
    // LivingEntity.travel with nothing of their own: no class under them overrides it in any tree
    // (the overrides are AbstractHorse, Squid, ArmorStand, FlyingMob, Guardian, Pig, AbstractFish,
    // Dolphin, Turtle, Drowned, Strider, Axolotl, Allay, Frog, Camel, HappyGhast, Ghast, Phantom and
    // SulfurCube, each above). A kind that was data before 1.11 counts from its type, as above.
    ("creeper", TravelFamily::Mob, (1, 8, 0)),
    ("skeleton", TravelFamily::Mob, (1, 8, 0)),
    ("giant", TravelFamily::Mob, (1, 8, 0)),
    ("slime", TravelFamily::Mob, (1, 8, 0)),
    ("enderman", TravelFamily::Mob, (1, 8, 0)),
    ("silverfish", TravelFamily::Mob, (1, 8, 0)),
    ("witch", TravelFamily::Mob, (1, 8, 0)),
    ("endermite", TravelFamily::Mob, (1, 8, 0)),
    ("sheep", TravelFamily::Mob, (1, 8, 0)),
    ("chicken", TravelFamily::Mob, (1, 8, 0)),
    ("wolf", TravelFamily::Mob, (1, 8, 0)),
    ("rabbit", TravelFamily::Mob, (1, 8, 0)),
    ("magma_cube", TravelFamily::Mob, (1, 8, 0)),
    ("ender_dragon", TravelFamily::Dragon, (1, 8, 0)),
    ("mooshroom", TravelFamily::Mob, (1, 8, 0)),
    ("ocelot", TravelFamily::Mob, (1, 8, 0)),
    ("snow_golem", TravelFamily::Mob, (1, 8, 0)),
    ("iron_golem", TravelFamily::Mob, (1, 8, 0)),
    ("villager", TravelFamily::Mob, (1, 8, 0)),
    ("zombie_pigman", TravelFamily::Mob, (1, 8, 0)),
    // (a shulker's travel moves nothing that stays: 1.9.4 Shulker.java:365-369 zeroes its motion after
    // aiStep, 1.16.5 Shulker.java:257 puts it back on its block in tick, 26.3 Shulker.java:295-300
    // holds no motion at all; no path here has its attachment)
    ("shulker", TravelFamily::Unsupported, (1, 9, 0)),
    ("wither_skeleton", TravelFamily::Mob, (1, 11, 0)),
    ("stray", TravelFamily::Mob, (1, 11, 0)),
    ("husk", TravelFamily::Mob, (1, 11, 0)),
    ("zombie_villager", TravelFamily::Mob, (1, 11, 0)),
    ("evoker", TravelFamily::Mob, (1, 11, 0)),
    ("vindicator", TravelFamily::Mob, (1, 11, 0)),
    ("illusioner", TravelFamily::Mob, (1, 12, 0)),
    ("cat", TravelFamily::Mob, (1, 14, 0)),
    ("fox", TravelFamily::Mob, (1, 14, 0)),
    ("panda", TravelFamily::Mob, (1, 14, 0)),
    ("pillager", TravelFamily::Mob, (1, 14, 0)),
    ("ravager", TravelFamily::Mob, (1, 14, 0)),
    ("wandering_trader", TravelFamily::Mob, (1, 14, 0)),
    ("hoglin", TravelFamily::Mob, (1, 16, 0)),
    ("piglin", TravelFamily::Mob, (1, 16, 0)),
    ("zoglin", TravelFamily::Mob, (1, 16, 0)),
    ("zombified_piglin", TravelFamily::Mob, (1, 16, 0)),
    ("piglin_brute", TravelFamily::Mob, (1, 16, 2)),
    ("goat", TravelFamily::Mob, (1, 17, 0)),
    ("warden", TravelFamily::Mob, (1, 19, 0)),
    ("sniffer", TravelFamily::Mob, (1, 19, 4)),
    ("breeze", TravelFamily::Mob, (1, 20, 3)),
    ("bogged", TravelFamily::Mob, (1, 20, 5)),
    ("armadillo", TravelFamily::Mob, (1, 20, 5)),
    ("creaking", TravelFamily::Mob, (1, 21, 2)),
    ("creaking_transient", TravelFamily::Mob, (1, 21, 2)),
    ("copper_golem", TravelFamily::Mob, (1, 21, 9)),
    ("mannequin", TravelFamily::Mob, (1, 21, 9)),
    ("parched", TravelFamily::Mob, (1, 21, 11)),
    ("item", TravelFamily::Entity, (1, 8, 0)),
    ("experience_orb", TravelFamily::Entity, (1, 8, 0)),
    ("painting", TravelFamily::Entity, (1, 8, 0)),
    ("item_frame", TravelFamily::Entity, (1, 8, 0)),
    ("leash_knot", TravelFamily::Entity, (1, 8, 0)),
    ("tnt", TravelFamily::Entity, (1, 8, 0)),
    ("falling_block", TravelFamily::Entity, (1, 8, 0)),
    ("end_crystal", TravelFamily::Entity, (1, 8, 0)),
    ("lightning_bolt", TravelFamily::Entity, (1, 8, 0)),
    ("eye_of_ender", TravelFamily::Entity, (1, 8, 0)),
    ("arrow", TravelFamily::Entity, (1, 8, 0)),
    ("snowball", TravelFamily::Entity, (1, 8, 0)),
    ("egg", TravelFamily::Entity, (1, 8, 0)),
    ("ender_pearl", TravelFamily::Entity, (1, 8, 0)),
    ("potion", TravelFamily::Entity, (1, 8, 0)),
    ("experience_bottle", TravelFamily::Entity, (1, 8, 0)),
    ("firework_rocket", TravelFamily::Entity, (1, 8, 0)),
    ("fishing_bobber", TravelFamily::Entity, (1, 8, 0)),
    ("fireball", TravelFamily::Entity, (1, 8, 0)),
    ("small_fireball", TravelFamily::Entity, (1, 8, 0)),
    ("wither_skull", TravelFamily::Entity, (1, 8, 0)),
    ("minecart", TravelFamily::Entity, (1, 8, 0)),
    ("chest_minecart", TravelFamily::Entity, (1, 8, 0)),
    ("furnace_minecart", TravelFamily::Entity, (1, 8, 0)),
    ("tnt_minecart", TravelFamily::Entity, (1, 8, 0)),
    ("hopper_minecart", TravelFamily::Entity, (1, 8, 0)),
    ("spawner_minecart", TravelFamily::Entity, (1, 8, 0)),
    ("command_block_minecart", TravelFamily::Entity, (1, 8, 0)),
    ("area_effect_cloud", TravelFamily::Entity, (1, 9, 0)),
    ("shulker_bullet", TravelFamily::Entity, (1, 9, 0)),
    ("spectral_arrow", TravelFamily::Entity, (1, 9, 0)),
    ("dragon_fireball", TravelFamily::Entity, (1, 9, 0)),
    ("llama_spit", TravelFamily::Entity, (1, 11, 0)),
    ("evoker_fangs", TravelFamily::Entity, (1, 11, 0)),
    ("trident", TravelFamily::Entity, (1, 13, 0)),
    ("glow_item_frame", TravelFamily::Entity, (1, 17, 0)),
    ("marker", TravelFamily::Entity, (1, 17, 0)),
    ("block_display", TravelFamily::Entity, (1, 19, 4)),
    ("item_display", TravelFamily::Entity, (1, 19, 4)),
    ("text_display", TravelFamily::Entity, (1, 19, 4)),
    ("interaction", TravelFamily::Entity, (1, 19, 4)),
    ("wind_charge", TravelFamily::Entity, (1, 20, 3)),
    ("breeze_wind_charge", TravelFamily::Entity, (1, 20, 5)),
    ("ominous_item_spawner", TravelFamily::Entity, (1, 20, 5)),
    ("splash_potion", TravelFamily::Entity, (1, 21, 5)),
    ("lingering_potion", TravelFamily::Entity, (1, 21, 5)),
    ("cushion", TravelFamily::Entity, (26, 3, 0)),
];

/// The names a registry used before the one kept here: 1.8 - 1.10's EntityList, 1.11 - 1.12's.
const LEGACY_NAMES: &[(&str, &str)] = &[
    ("XPOrb", "experience_orb"),
    ("xp_orb", "experience_orb"),
    ("Painting", "painting"),
    ("Creeper", "creeper"),
    ("Skeleton", "skeleton"),
    ("Spider", "spider"),
    ("Giant", "giant"),
    ("Zombie", "zombie"),
    ("Slime", "slime"),
    ("Ghast", "ghast"),
    ("PigZombie", "zombie_pigman"),
    ("Enderman", "enderman"),
    ("CaveSpider", "cave_spider"),
    ("Silverfish", "silverfish"),
    ("Blaze", "blaze"),
    ("LavaSlime", "magma_cube"),
    ("EnderDragon", "ender_dragon"),
    ("WitherBoss", "wither"),
    ("Bat", "bat"),
    ("Witch", "witch"),
    ("Endermite", "endermite"),
    ("Guardian", "guardian"),
    ("Pig", "pig"),
    ("Sheep", "sheep"),
    ("Cow", "cow"),
    ("Chicken", "chicken"),
    ("Squid", "squid"),
    ("Wolf", "wolf"),
    ("MushroomCow", "mooshroom"),
    ("SnowMan", "snow_golem"),
    ("snowman", "snow_golem"),
    ("Ozelot", "ocelot"),
    ("VillagerGolem", "iron_golem"),
    ("villager_golem", "iron_golem"),
    ("EntityHorse", "horse"),
    ("Rabbit", "rabbit"),
    ("Villager", "villager"),
    ("Shulker", "shulker"),
    ("PolarBear", "polar_bear"),
    ("evocation_illager", "evoker"),
    ("vindication_illager", "vindicator"),
    ("illusion_illager", "illusioner"),
    ("commandblock_minecart", "command_block_minecart"),
    ("Boat", "boat"),
    ("Item", "item"),
    ("MinecartRideable", "minecart"),
    ("PrimedTnt", "tnt"),
    ("EnderCrystal", "end_crystal"),
    ("ender_crystal", "end_crystal"),
    ("Arrow", "arrow"),
    ("Snowball", "snowball"),
    ("ThrownEgg", "egg"),
    ("Fireball", "fireball"),
    ("SmallFireball", "small_fireball"),
    ("ThrownEnderpearl", "ender_pearl"),
    ("WitherSkull", "wither_skull"),
    ("FallingSand", "falling_block"),
    ("ItemFrame", "item_frame"),
    ("EyeOfEnderSignal", "eye_of_ender"),
    ("eye_of_ender_signal", "eye_of_ender"),
    ("ThrownPotion", "potion"),
    ("ThrownExpBottle", "experience_bottle"),
    ("xp_bottle", "experience_bottle"),
    ("FireworksRocketEntity", "firework_rocket"),
    ("fireworks_rocket", "firework_rocket"),
    ("LeashKnot", "leash_knot"),
    ("ArmorStand", "armor_stand"),
    ("Fishing Float", "fishing_bobber"),
    ("Fishing Hook", "fishing_bobber"),
    ("AreaEffectCloud", "area_effect_cloud"),
    ("ShulkerBullet", "shulker_bullet"),
    ("SpectralArrow", "spectral_arrow"),
    ("DragonFireball", "dragon_fireball"),
    ("evocation_fangs", "evoker_fangs"),
];

/// The first release a kind is gone from: 1.16 renames the zombie pigman, 1.21.4 drops the
/// transient creaking, 1.21.5 splits the potion (each release's registry).
fn gone_from(name: &str) -> Option<Release> {
    match name {
        "zombie_pigman" => Some((1, 16, 0)),
        "creaking_transient" => Some((1, 21, 4)),
        "potion" => Some((1, 21, 5)),
        _ => None,
    }
}

/// The woods of the boats EntityType names one by one from 1.21.2 (client1.21.2 EntityType.java);
/// 1.21.1 has `boat` and `chest_boat`, the latter from 1.19.
const BOAT_WOODS: &[&str] = &[
    "oak", "spruce", "birch", "jungle", "acacia", "cherry", "dark_oak", "pale_oak", "mangrove",
];

/// The wood 26.3 adds (generated/entity-data.json: 26.3's dump has poplar_boat, 26.2's has not).
const POPLAR: Release = (26, 3, 0);

/// A boat's name, the first release that has it and the first that no longer does.
fn boat(name: &str) -> Option<(Release, Option<Release>)> {
    const NAMED: Release = (1, 21, 2);
    match name {
        "boat" => return Some(((1, 8, 0), Some(NAMED))),
        "chest_boat" => return Some(((1, 19, 0), Some(NAMED))),
        "bamboo_raft" | "bamboo_chest_raft" => return Some((NAMED, None)),
        _ => {}
    }
    let wood = name
        .strip_suffix("_chest_boat")
        .or_else(|| name.strip_suffix("_boat"))?;
    if wood == "poplar" {
        return Some((POPLAR, None));
    }
    BOAT_WOODS.contains(&wood).then_some((NAMED, None))
}

/// What the engine says of a kind in a release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TravelSupport {
    /// None for a name outside the inventory.
    pub family: Option<TravelFamily>,
    /// The release's registry holds the kind.
    pub available: bool,
    /// The first release that has it.
    pub since: Option<Release>,
}

/// The inventory's entry for a kind in a release.
#[must_use]
pub fn travel_support(name: &str, release: MinecraftVersion) -> TravelSupport {
    let at_least = |(major, minor, patch): Release| release.at_least(major, minor, patch);
    let name = LEGACY_NAMES
        .iter()
        .find(|(legacy, _)| *legacy == name)
        .map_or(name, |(_, modern)| modern);
    if let Some((since, until)) = boat(name) {
        return TravelSupport {
            family: Some(TravelFamily::Boat),
            available: at_least(since) && !until.is_some_and(at_least),
            since: Some(since),
        };
    }
    match INVENTORY.iter().find(|(kind, ..)| *kind == name) {
        Some(&(_, family, since)) => TravelSupport {
            family: Some(family),
            available: at_least(since) && !gone_from(name).is_some_and(at_least),
            since: Some(since),
        },
        None => TravelSupport {
            family: None,
            available: false,
            since: None,
        },
    }
}

/// Why the player's tick refuses a kind it was named for: everything but the player, by what it
/// should go through instead.
pub fn refuse_for_player_tick(name: &str) -> Result<(), String> {
    // (the release plays no part: no kind but the player has this path in any of them)
    let family = travel_support(name, MinecraftVersion::new(u16::MAX, 0, 0)).family;
    match family {
        Some(TravelFamily::Player) => Ok(()),
        Some(TravelFamily::Boat) => Err(format!("{name} ticks through simulateVersionedBoatTick")),
        Some(TravelFamily::Ridden) => Err(format!("{name} ticks through simulateVersionedHorseTick")),
        Some(TravelFamily::Mob) => Err(format!(
            "no travel simulation for {name} on the player's equations: it travels through simulateVersionedMobTravel"
        )),
        Some(TravelFamily::Dragon) => Err(format!("{name} ticks through simulateVersionedDragonTick")),
        Some(TravelFamily::Unsupported) => Err(format!(
            "no travel simulation for {name}: it is not run on the player's equations"
        )),
        Some(TravelFamily::Entity) => Err(format!(
            "no travel simulation for {name}: no living entity, in the engine's entity inventory, has its own tick"
        )),
        None => Err(format!("{name} is not in the engine's entity inventory")),
    }
}

/// Why a mob's travel refuses a kind, when it does: one its release does not have runs no equations.
pub fn refuse_for_mob_travel(name: &str, release: MinecraftVersion) -> Result<(), String> {
    // ("living" is any LivingEntity with nothing of its own, which names no kind; a mount out of a
    // saddle travels here as well)
    if name == "living" {
        return Ok(());
    }
    let support = travel_support(name, release);
    let unridden = matches!(name, "strider" | "skeleton_horse");
    match support.family {
        Some(TravelFamily::Mob) if support.available => Ok(()),
        Some(TravelFamily::Ridden) if unridden && support.available => Ok(()),
        Some(TravelFamily::Mob) => Err(format!("{name} is not in {release}")),
        Some(TravelFamily::Ridden) if unridden => Err(format!("{name} is not in {release}")),
        Some(TravelFamily::Dragon) => Err(format!(
            "no LivingEntity.travel for {name}: it ticks through simulateVersionedDragonTick"
        )),
        Some(TravelFamily::Unsupported) => Err(format!("no travel simulation for {name}")),
        _ => Err(format!("no travel of its own: {name}")),
    }
}

/// Why a ridden tick refuses a kind, when it does.
pub fn refuse_for_ridden_tick(name: &str, release: MinecraftVersion) -> Result<(), String> {
    let support = travel_support(name, release);
    if support.family == Some(TravelFamily::Ridden) && !support.available {
        return Err(format!("{name} does not exist in this release"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        refuse_for_mob_travel, refuse_for_player_tick, refuse_for_ridden_tick, travel_support,
        TravelFamily,
    };
    use crate::version::MinecraftVersion;

    const fn release(major: u16, minor: u16, patch: u16) -> MinecraftVersion {
        MinecraftVersion::new(major, minor, patch)
    }

    #[test]
    fn names_every_kind_and_keeps_a_name_outside_the_game_unknown() {
        let family = |name| travel_support(name, release(26, 3, 0)).family;
        assert_eq!(family("creeper"), Some(TravelFamily::Mob));
        assert_eq!(family("minecart"), Some(TravelFamily::Entity));
        assert_eq!(family("poplar_chest_boat"), Some(TravelFamily::Boat));
        assert_eq!(family("no_such_kind"), None);
        // the names of the registries before 1.11 and 1.13
        assert_eq!(family("PigZombie"), family("zombie_pigman"));
        assert!(travel_support("LavaSlime", release(1, 8, 8)).available);
        assert!(!travel_support("potion", release(1, 21, 5)).available);
        assert!(travel_support("potion", release(1, 21, 4)).available);
    }

    #[test]
    fn the_dragon_and_the_shulker_do_not_travel_as_a_living_entity() {
        // (EnderDragon.aiStep never calls LivingEntity.travel; a shulker's motion is zeroed or held
        // at none, and tick puts it back on its block)
        for at in [release(1, 9, 4), release(1, 16, 5), release(26, 3, 0)] {
            assert_eq!(travel_support("ender_dragon", at).family, Some(TravelFamily::Dragon));
            assert_eq!(travel_support("shulker", at).family, Some(TravelFamily::Unsupported));
            assert!(refuse_for_mob_travel("ender_dragon", at)
                .unwrap_err()
                .contains("simulateVersionedDragonTick"));
            assert!(refuse_for_mob_travel("shulker", at).is_err());
            assert!(refuse_for_mob_travel("creeper", at).is_ok());
        }
        assert_eq!(
            refuse_for_player_tick("EnderDragon").unwrap_err(),
            "EnderDragon ticks through simulateVersionedDragonTick"
        );
    }

    #[test]
    fn the_sulfur_cube_and_the_poplar_boat_start_where_their_trees_do() {
        // (26.2 monster/cubemob/SulfurCube.java, EntityTypes.java)
        assert!(!travel_support("sulfur_cube", release(26, 1, 2)).available);
        assert!(travel_support("sulfur_cube", release(26, 2, 0)).available);
        assert!(!travel_support("poplar_boat", release(26, 2, 0)).available);
        assert!(travel_support("poplar_boat", release(26, 3, 0)).available);
    }

    #[test]
    fn a_kind_is_available_from_its_first_release() {
        let available = |name, at| travel_support(name, at).available;
        assert!(!available("strider", release(1, 15, 2)));
        assert!(available("strider", release(1, 16, 0)));
        assert!(!available("cod", release(1, 12, 2)));
        assert!(available("cod", release(1, 13, 0)));
        assert!(!available("nautilus", release(1, 21, 10)));
        assert!(available("nautilus", release(1, 21, 11)));
    }

    #[test]
    fn the_boats_change_names_at_1_21_2() {
        let available = |name, at| travel_support(name, at).available;
        assert!(available("boat", release(1, 21, 1)));
        assert!(!available("boat", release(1, 21, 2)));
        assert!(!available("oak_boat", release(1, 21, 1)));
        assert!(available("oak_boat", release(1, 21, 2)));
        assert!(!available("chest_boat", release(1, 18, 2)));
        assert!(available("chest_boat", release(1, 19, 0)));
        assert!(available("bamboo_chest_raft", release(26, 3, 0)));
        assert_eq!(
            travel_support("pale_oak_chest_boat", release(26, 3, 0)).family,
            Some(TravelFamily::Boat)
        );
        assert_eq!(travel_support("stone_boat", release(26, 3, 0)).family, None);
    }

    #[test]
    fn the_player_tick_takes_the_player_alone() {
        assert!(refuse_for_player_tick("player").is_ok());
        for (name, reason) in [
            ("cod", "no travel simulation"),
            ("squid", "no travel simulation"),
            ("strider", "simulateVersionedHorseTick"),
            ("oak_boat", "simulateVersionedBoatTick"),
            ("minecart", "inventory"),
        ] {
            let refused = refuse_for_player_tick(name).expect_err(name);
            assert!(refused.contains(reason), "{name}: {refused}");
        }
    }

    #[test]
    fn a_mobs_travel_exists_from_the_release_that_has_the_mob() {
        let refuses = |name, at| refuse_for_mob_travel(name, at).is_err();
        for (name, before, first) in [
            ("vex", release(1, 10, 2), release(1, 11, 0)),
            ("phantom", release(1, 12, 2), release(1, 13, 0)),
            ("cod", release(1, 12, 2), release(1, 13, 0)),
            ("glow_squid", release(1, 16, 5), release(1, 17, 0)),
            ("allay", release(1, 18, 2), release(1, 19, 0)),
        ] {
            assert!(refuses(name, before), "{name}");
            assert!(!refuses(name, first), "{name}");
        }
        assert!(!refuses("squid", release(1, 8, 8)));
        assert!(!refuses("ghast", release(1, 8, 8)));
        // a kind with no travel here is refused whatever the release
        assert!(refuses("minecart", release(26, 3, 0)));
        assert!(refuses("trident", release(26, 3, 0)));
        assert!(refuses("horse", release(26, 3, 0)));
    }

    #[test]
    fn a_ridden_tick_refuses_a_mob_its_release_does_not_have() {
        assert!(refuse_for_ridden_tick("strider", release(1, 16, 0)).is_ok());
        assert!(refuse_for_ridden_tick("strider", release(1, 15, 2)).is_err());
        assert!(refuse_for_ridden_tick("happy_ghast", release(1, 21, 5)).is_err());
    }
}

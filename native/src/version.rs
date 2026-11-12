use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MinecraftVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionCapabilities {
    pub fluid_physics: bool,
    pub honey_blocks: bool,
    pub fall_flying: bool,
    pub water_movement_efficiency: bool,
    pub minor_horizontal_collision: bool,
}

impl MinecraftVersion {
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = match parts.next() {
            Some(patch) => patch.parse().ok()?,
            None => 0,
        };
        parts
            .next()
            .is_none()
            .then_some(Self::new(major, minor, patch))
    }

    #[must_use]
    pub const fn capabilities(self) -> VersionCapabilities {
        VersionCapabilities {
            // (1.12.2 Entity.java:1032-1036; 1.13.2 Entity.java:1052-1055,2504)
            fluid_physics: self.at_least(1, 13, 0),
            // (1.14.4 Blocks.java:897; 1.15 Blocks.java:2036)
            honey_blocks: self.at_least(1, 15, 0),
            // (1.8.8 LivingEntity.java has no fall flying; 1.9.4 LivingEntity.java:1327 travels
            // fall flying)
            fall_flying: self.at_least(1, 9, 0),
            // LivingEntity registers WATER_MOVEMENT_EFFICIENCY from 1.21 (1.21 LivingEntity.java:311)
            water_movement_efficiency: self.at_least(1, 21, 0),
            minor_horizontal_collision: self.supports_minor_horizontal_collision(),
        }
    }

    #[must_use]
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    #[must_use]
    pub const fn supports_minor_horizontal_collision(self) -> bool {
        // (1.17.1 Entity.java (no minorHorizontalCollision, by grep); 1.18 Entity.java:574)
        self.at_least(1, 18, 0)
    }

    pub const fn at_least(self, major: u16, minor: u16, patch: u16) -> bool {
        self.major > major
            || (self.major == major
                && (self.minor > minor || (self.minor == minor && self.patch >= patch)))
    }
}

impl fmt::Display for MinecraftVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.patch == 0 {
            write!(formatter, "{}.{}", self.major, self.minor)
        } else {
            write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MinecraftVersion;

    #[test]
    fn compares_versions() {
        assert!(MinecraftVersion::new(1, 21, 4) > MinecraftVersion::new(1, 20, 6));
        assert!(MinecraftVersion::new(1, 21, 0) < MinecraftVersion::new(1, 21, 1));
    }

    #[test]
    fn parses_release_names_with_and_without_patch() {
        assert_eq!(
            MinecraftVersion::parse("1.21.4"),
            Some(MinecraftVersion::new(1, 21, 4))
        );
        assert_eq!(
            MinecraftVersion::parse("1.21"),
            Some(MinecraftVersion::new(1, 21, 0))
        );
        assert_eq!(
            MinecraftVersion::parse("26.1"),
            Some(MinecraftVersion::new(26, 1, 0))
        );
        assert_eq!(MinecraftVersion::parse("1"), None);
        assert_eq!(MinecraftVersion::parse("1.21."), None);
        assert_eq!(MinecraftVersion::parse("1.21.4.extra"), None);
    }

    #[test]
    fn formats_as_release_name() {
        assert_eq!(MinecraftVersion::new(1, 21, 0).to_string(), "1.21");
        assert_eq!(MinecraftVersion::new(1, 21, 4).to_string(), "1.21.4");
        assert_eq!(MinecraftVersion::new(26, 1, 0).to_string(), "26.1");
    }

    #[test]
    fn derives_capabilities_from_version() {
        let legacy = MinecraftVersion::new(1, 12, 2).capabilities();
        assert!(!legacy.fluid_physics);
        assert!(!legacy.honey_blocks);
        assert!(!legacy.water_movement_efficiency);
        assert!(!legacy.minor_horizontal_collision);

        let modern = MinecraftVersion::new(1, 21, 4).capabilities();
        assert!(modern.fluid_physics);
        assert!(modern.honey_blocks);
        assert!(modern.fall_flying);
        assert!(modern.water_movement_efficiency);
        assert!(modern.minor_horizontal_collision);
        // 1.21.0 registers and reads the attribute (1.21 LivingEntity.java:311, 2130); 1.20.6 has
        // neither (origin/client1.20.6 LivingEntity.java)
        assert!(
            MinecraftVersion::new(1, 21, 0)
                .capabilities()
                .water_movement_efficiency
        );
        assert!(
            !MinecraftVersion::new(1, 20, 6)
                .capabilities()
                .water_movement_efficiency
        );
    }
}

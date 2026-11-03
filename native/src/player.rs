use crate::entity::Dimensions;
use crate::math::Vec3;
use crate::version::MinecraftVersion;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Pose {
    #[default]
    Standing,
    Sneaking,
    Swimming,
    FallFlying,
    Sleeping,
    SpinAttack,
    LongJumping,
    Dying,
}

impl Pose {
    /// Player pose dimensions from the decompiled `Player` pose table.
    #[must_use]
    pub const fn dimensions(self) -> Dimensions {
        match self {
            Self::Standing | Self::Sneaking | Self::LongJumping => Dimensions::new(
                0.3,
                if matches!(self, Self::Sneaking) {
                    1.5
                } else {
                    1.8
                },
            ),
            Self::Swimming | Self::FallFlying | Self::SpinAttack => Dimensions::new(0.3, 0.6),
            Self::Sleeping => Dimensions::new(0.1, 0.2),
            Self::Dying => Dimensions::new(0.1, 0.2),
        }
    }

    /// Eye height selected by `LivingEntity#getEyeHeight` for each player pose.
    #[must_use]
    pub const fn eye_height(self) -> f64 {
        match self {
            Self::Standing | Self::LongJumping => 1.62,
            Self::Sneaking => 1.27,
            Self::Swimming | Self::FallFlying | Self::SpinAttack => 0.4,
            // 0.2F asleep (1.14 LivingEntity.java:2749, 1.21.11 LivingEntity.java:3878); the
            // fixed dying dimensions keep the 1.62F eyes (1.21.11 Avatar.java:55)
            Self::Sleeping => 0.20000000298023224,
            Self::Dying => 1.62,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerControls {
    pub forward: bool,
    pub back: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sprint: bool,
    pub sneak: bool,
}

impl PlayerControls {
    #[must_use]
    pub fn input_vector(self) -> Vec3 {
        let x = f64::from(self.right as u8) - f64::from(self.left as u8);
        let z = f64::from(self.back as u8) - f64::from(self.forward as u8);
        let length = (x * x + z * z).sqrt();
        if length == 0.0 {
            Vec3::ZERO
        } else {
            Vec3::new(x / length, 0.0, z / length)
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub controls: PlayerControls,
    pub pose: Pose,
    pub yaw: f64,
    pub pitch: f64,
    pub jump_ticks: u32,
}

impl PlayerState {
    #[must_use]
    pub fn look_vector(self, release: MinecraftVersion) -> Vec3 {
        Vec3::direction_from_rotation(self.pitch as f32, self.yaw as f32, release)
    }
}

#[cfg(test)]
mod tests {
    use super::{PlayerControls, PlayerState, Pose};
    use crate::version::MinecraftVersion;

    #[test]
    fn normalizes_diagonal_input() {
        let input = PlayerControls {
            forward: true,
            right: true,
            ..PlayerControls::default()
        }
        .input_vector();

        assert!((input.x * input.x + input.z * input.z - 1.0).abs() < 1e-12);
    }

    #[test]
    fn computes_look_vector() {
        let state = PlayerState {
            yaw: 0.0,
            pitch: 0.0,
            ..PlayerState::default()
        };

        assert_eq!(state.look_vector(MinecraftVersion::new(1, 21, 11)).z, 1.0);
    }

    #[test]
    fn crouching_uses_vanilla_dimensions_and_eye_height() {
        assert_eq!(
            Pose::Sneaking.dimensions(),
            super::Dimensions::new(0.3, 1.5)
        );
        assert_eq!(Pose::Sneaking.eye_height(), 1.27);
        assert!(Pose::Sneaking.dimensions().height < Pose::Standing.dimensions().height);
    }

    #[test]
    fn compact_poses_match_vanilla_bounds() {
        assert_eq!(
            Pose::Swimming.dimensions(),
            super::Dimensions::new(0.3, 0.6)
        );
        assert_eq!(Pose::FallFlying.dimensions(), Pose::Swimming.dimensions());
        assert_eq!(Pose::SpinAttack.dimensions(), Pose::Swimming.dimensions());
        assert_eq!(Pose::LongJumping.dimensions(), Pose::Standing.dimensions());
        assert_eq!(
            Pose::Sleeping.dimensions(),
            super::Dimensions::new(0.1, 0.2)
        );
        // LivingEntity.getEyeHeight: 0.2F sleeping (1.14 LivingEntity.java:2749, 1.20.4 :3267)
        assert_eq!(Pose::Sleeping.eye_height(), f64::from(0.2_f32));
        assert_eq!(Pose::Dying.eye_height(), 1.62);
    }
}

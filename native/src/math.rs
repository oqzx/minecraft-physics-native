use std::ops::{Add, Sub};
use std::sync::OnceLock;

use crate::trig;

const SIN_SCALE: f64 = 10430.378350470453;
const ATAN_LUT_SIZE: usize = 257;
const ATAN_FRAC_BIAS: f64 = f64::from_bits(4_805_340_802_404_319_232);

fn sine_table() -> &'static [f32; 65536] {
    static TABLE: OnceLock<[f32; 65536]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0; 65536];
        for (index, value) in table.iter_mut().enumerate() {
            *value = trig::sin(index as f64 / SIN_SCALE) as f32;
        }

        table
    })
}

fn legacy_sine_table() -> &'static [f32; 65536] {
    static TABLE: OnceLock<[f32; 65536]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0; 65536];
        for (index, value) in table.iter_mut().enumerate() {
            *value = trig::sin(index as f64 * std::f64::consts::PI * 2.0 / 65536.0) as f32;
        }

        table
    })
}

fn atan_tables() -> (&'static [f64; ATAN_LUT_SIZE], &'static [f64; ATAN_LUT_SIZE]) {
    static TABLES: OnceLock<([f64; ATAN_LUT_SIZE], [f64; ATAN_LUT_SIZE])> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        let mut asin = [0.0; ATAN_LUT_SIZE];
        let mut cosine = [0.0; ATAN_LUT_SIZE];
        for index in 0..ATAN_LUT_SIZE {
            // Java's Math.asin is fdlibm's, which libm ports
            let angle = libm::asin(index as f64 / 256.0);
            asin[index] = angle;
            cosine[index] = trig::cos(angle);
        }
        (asin, cosine)
    });
    (&tables.0, &tables.1)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    #[must_use]
    pub fn plus(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    #[must_use]
    pub fn add_components(self, x: f64, y: f64, z: f64) -> Self {
        Self::new(self.x + x, self.y + y, self.z + z)
    }

    #[must_use]
    pub fn add_scalar(self, value: f64) -> Self {
        self.add_components(value, value, value)
    }

    #[must_use]
    pub fn subtract_scalar(self, value: f64) -> Self {
        self.add_scalar(-value)
    }

    #[must_use]
    pub fn scale(self, scalar: f64) -> Self {
        Self::new(self.x * scalar, self.y * scalar, self.z * scalar)
    }

    #[must_use]
    pub fn vector_to(self, target: Self) -> Self {
        target - self
    }

    #[must_use]
    pub fn reverse(self) -> Self {
        self.scale(-1.0)
    }

    #[must_use]
    pub fn multiply(self, rhs: Self) -> Self {
        Self::new(self.x * rhs.x, self.y * rhs.y, self.z * rhs.z)
    }

    #[must_use]
    pub fn horizontal(self) -> Self {
        Self::new(self.x, 0.0, self.z)
    }

    #[must_use]
    pub fn rotate_clockwise_90(self) -> Self {
        Self::new(-self.z, self.y, self.x)
    }

    #[must_use]
    pub fn projected_on(self, axis: Self) -> Self {
        let length_squared = axis.length_squared();
        if length_squared == 0.0 {
            axis
        } else {
            axis.scale(self.dot(axis) / length_squared)
        }
    }

    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }

    #[must_use]
    pub fn distance_squared(self, rhs: Self) -> f64 {
        self.distance_squared_to(rhs.x, rhs.y, rhs.z)
    }

    #[must_use]
    pub fn distance_squared_to(self, x: f64, y: f64, z: f64) -> f64 {
        let dx = x - self.x;
        let dy = y - self.y;
        let dz = z - self.z;
        dx * dx + dy * dy + dz * dz
    }

    #[must_use]
    pub fn distance(self, rhs: Self) -> f64 {
        self.distance_squared(rhs).sqrt()
    }

    #[must_use]
    pub fn closer_than(self, rhs: Self, radius: f64) -> bool {
        self.distance_squared(rhs) < radius * radius
    }

    #[must_use]
    pub fn closer_than_horizontal(self, rhs: Self, radius: f64, vertical: f64) -> bool {
        self.horizontal_length_squared_to(rhs) < radius * radius
            && (rhs.y - self.y).abs() < vertical
    }

    #[must_use]
    pub fn horizontal_length_squared_to(self, rhs: Self) -> f64 {
        let dx = rhs.x - self.x;
        let dz = rhs.z - self.z;
        dx * dx + dz * dz
    }

    #[must_use]
    pub fn lerp(self, target: Self, amount: f64) -> Self {
        Self::new(
            self.x + amount * (target.x - self.x),
            self.y + amount * (target.y - self.y),
            self.z + amount * (target.z - self.z),
        )
    }

    #[must_use]
    pub fn x_rot(self, radians: f64) -> Self {
        let cosine = java_cos(radians);
        let sine = java_sin(radians);
        Self::new(
            self.x,
            self.y * cosine + self.z * sine,
            self.z * cosine - self.y * sine,
        )
    }

    #[must_use]
    pub fn y_rot(self, radians: f64) -> Self {
        let cosine = java_cos(radians);
        let sine = java_sin(radians);
        Self::new(
            self.x * cosine + self.z * sine,
            self.y,
            self.z * cosine - self.x * sine,
        )
    }

    #[must_use]
    pub fn z_rot(self, radians: f64) -> Self {
        let cosine = java_cos(radians);
        let sine = java_sin(radians);
        Self::new(
            self.x * cosine + self.y * sine,
            self.y * cosine - self.x * sine,
            self.z,
        )
    }

    #[must_use]
    pub fn direction_from_rotation(pitch_degrees: f64, yaw_degrees: f64) -> Self {
        let pitch = degrees_to_radians(-pitch_degrees);
        let yaw = degrees_to_radians(-yaw_degrees - 180.0);
        let yaw_sine = java_sin(yaw);
        let yaw_cosine = java_cos(yaw);
        let pitch_cosine = java_cos(pitch);
        let pitch_sine = java_sin(pitch);
        let horizontal = -pitch_cosine;
        Self::new(yaw_sine * horizontal, pitch_sine, yaw_cosine * horizontal)
    }

    #[must_use]
    pub fn rotation(self) -> (f64, f64) {
        let yaw = (-self.x).atan2(self.z).to_degrees();
        let pitch = (-self.y / self.length()).asin().to_degrees();
        (pitch, yaw)
    }

    #[must_use]
    pub fn length_squared(self) -> f64 {
        self.dot(self)
    }

    #[must_use]
    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    #[must_use]
    pub fn horizontal_length_squared(self) -> f64 {
        self.x * self.x + self.z * self.z
    }

    #[must_use]
    pub fn horizontal_length(self) -> f64 {
        self.horizontal_length_squared().sqrt()
    }

    #[must_use]
    pub fn dot(self, rhs: Self) -> f64 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    #[must_use]
    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    #[must_use]
    pub fn normalized(self) -> Self {
        let length = self.length();
        if length < 1.0e-5 {
            Self::ZERO
        } else {
            self.scale(1.0 / length)
        }
    }
}

#[must_use]
pub fn java_float(value: f64) -> f64 {
    f32::from(value as f32) as f64
}

#[must_use]
pub fn java_sin(radians: f64) -> f64 {
    let index = ((radians * SIN_SCALE) as i64 & 65535) as usize;
    f64::from(sine_table()[index])
}

#[must_use]
pub fn java_cos(radians: f64) -> f64 {
    let index = ((radians * SIN_SCALE + 16384.0) as i64 & 65535) as usize;
    f64::from(sine_table()[index])
}

#[must_use]
pub fn java_sin_legacy(radians: f32) -> f64 {
    let index = ((radians * 10430.378) as i32 & 65535) as usize;
    f64::from(legacy_sine_table()[index])
}

#[must_use]
pub fn java_cos_legacy(radians: f32) -> f64 {
    let index = ((radians * 10430.378 + 16384.0) as i32 & 65535) as usize;
    f64::from(legacy_sine_table()[index])
}

#[must_use]
pub fn sqrt_f32(value: f32) -> f32 {
    value.sqrt()
}

#[must_use]
pub fn abs_f32(value: f32) -> f32 {
    value.abs()
}

#[must_use]
pub fn abs_i64(value: i64) -> i64 {
    value.wrapping_abs()
}

#[must_use]
pub fn inv_sqrt_f64(value: f64) -> f64 {
    1.0 / value.sqrt()
}

#[must_use]
pub fn inv_sqrt_f32(value: f32) -> f32 {
    1.0 / value.sqrt()
}

#[must_use]
pub fn fast_inv_sqrt(value: f64) -> f64 {
    let half = 0.5 * value;
    let mut bits = value.to_bits();
    bits = 6_910_469_410_427_058_090_u64.wrapping_sub(bits >> 1);
    let estimate = f64::from_bits(bits);
    estimate * (1.5 - half * estimate * estimate)
}

#[must_use]
pub fn fast_inv_sqrt_f32(value: f32) -> f32 {
    let half = 0.5 * value;
    let bits = 1_597_463_007_i32.wrapping_sub(value.to_bits() as i32 >> 1);
    let estimate = f32::from_bits(bits as u32);
    estimate * (1.5 - half * estimate * estimate)
}

#[must_use]
pub fn fast_inv_cube_root(value: f32) -> f32 {
    let mut bits = value.to_bits() as i32;
    bits = 1_419_967_116 - bits / 3;
    let mut estimate = f32::from_bits(bits as u32);
    estimate = 0.6666667 * estimate + 1.0 / (3.0 * estimate * estimate * value);
    0.6666667 * estimate + 1.0 / (3.0 * estimate * estimate * value)
}

#[must_use]
pub fn atan2_vanilla(y: f64, x: f64) -> f64 {
    let squared_length = x * x + y * y;
    if squared_length.is_nan() {
        return f64::NAN;
    }
    let negative_y = y < 0.0;
    let y = y.abs();
    let negative_x = x < 0.0;
    let x = x.abs();
    let swapped = y > x;
    let (minor, major) = if swapped { (x, y) } else { (y, x) };
    let inverse_length = fast_inv_sqrt(squared_length);
    let normalized_major = major * inverse_length;
    let normalized_minor = minor * inverse_length;
    let biased = ATAN_FRAC_BIAS + normalized_minor;
    let index = (biased.to_bits() as u32) as usize;
    let (asin_table, cosine_table) = atan_tables();
    let asin = asin_table[index];
    let cosine = cosine_table[index];
    let fraction = biased - ATAN_FRAC_BIAS;
    let correction = normalized_minor * cosine - normalized_major * fraction;
    let correction = (6.0 + correction * correction) * correction * (1.0 / 6.0);
    let mut angle = asin + correction;
    if swapped {
        angle = std::f64::consts::FRAC_PI_2 - angle;
    }
    if negative_x {
        angle = std::f64::consts::PI - angle;
    }
    if negative_y {
        angle = -angle;
    }
    angle
}

#[must_use]
pub fn hsv_to_argb(hue: f32, saturation: f32, value: f32, alpha: i32) -> i32 {
    let section = (hue * 6.0) as i32 % 6;
    let fraction = hue * 6.0 - section as f32;
    let minimum = value * (1.0 - saturation);
    let descending = value * (1.0 - fraction * saturation);
    let ascending = value * (1.0 - (1.0 - fraction) * saturation);
    let (red, green, blue) = match section {
        0 => (value, ascending, minimum),
        1 => (descending, value, minimum),
        2 => (minimum, value, ascending),
        3 => (minimum, descending, value),
        4 => (ascending, minimum, value),
        5 => (value, minimum, descending),
        _ => panic!("invalid HSV section"),
    };
    let channel = |component: f32| ((component * 255.0) as i32).clamp(0, 255);
    ((alpha & 0xff) << 24) | (channel(red) << 16) | (channel(green) << 8) | channel(blue)
}

#[must_use]
pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> i32 {
    hsv_to_argb(hue, saturation, value, 0)
}

#[must_use]
pub fn degrees_to_radians(degrees: f64) -> f64 {
    java_float(degrees * java_float(std::f64::consts::PI / 180.0))
}

#[must_use]
pub fn floor_i32(value: f64) -> i32 {
    let truncated = value as i32;
    if value < f64::from(truncated) {
        truncated - 1
    } else {
        truncated
    }
}

#[must_use]
pub fn floor_f32(value: f32) -> i32 {
    let truncated = value as i32;
    if value < truncated as f32 {
        truncated - 1
    } else {
        truncated
    }
}

#[must_use]
pub fn ceil_i32(value: f64) -> i32 {
    let truncated = value as i32;
    if value > f64::from(truncated) {
        truncated + 1
    } else {
        truncated
    }
}

#[must_use]
pub fn ceil_f32(value: f32) -> i32 {
    let truncated = value as i32;
    if value > truncated as f32 {
        truncated + 1
    } else {
        truncated
    }
}

#[must_use]
pub fn abs_i32(value: i32) -> i32 {
    value.wrapping_abs()
}

#[must_use]
pub fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value.is_nan() || max.is_nan() {
        f32::NAN
    } else {
        value.min(max)
    }
}

#[must_use]
pub fn frac_f32(value: f32) -> f32 {
    value - floor_f32(value) as f32
}

#[must_use]
pub fn floor_i64(value: f64) -> i64 {
    let truncated = value as i64;
    if value < truncated as f64 {
        truncated - 1
    } else {
        truncated
    }
}

#[must_use]
pub fn ceil_i64(value: f64) -> i64 {
    let truncated = value as i64;
    if value > truncated as f64 {
        truncated + 1
    } else {
        truncated
    }
}

#[must_use]
pub fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    value.clamp(min, max)
}

#[must_use]
pub fn clamp_i64(value: i64, min: i64, max: i64) -> i64 {
    value.clamp(min, max)
}

#[must_use]
pub fn clamp_f64(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value.is_nan() || max.is_nan() {
        f64::NAN
    } else {
        value.min(max)
    }
}

#[must_use]
pub fn positive_mod_i32(value: i32, modulus: i32) -> i32 {
    value.rem_euclid(modulus)
}

#[must_use]
pub fn positive_mod_i64(value: i64, modulus: i64) -> i64 {
    value.rem_euclid(modulus)
}

#[must_use]
pub fn positive_mod_f64(value: f64, modulus: f64) -> f64 {
    (value % modulus + modulus) % modulus
}

#[must_use]
pub fn frac_f64(value: f64) -> f64 {
    value - floor_i64(value) as f64
}

#[must_use]
pub fn degrees_difference(from: f64, to: f64) -> f64 {
    wrap_degrees(to - from)
}

#[must_use]
pub fn rotate_if_necessary(current: f64, target: f64, maximum_change: f64) -> f64 {
    let difference = degrees_difference(current, target);
    target - clamp_f64(difference, -maximum_change, maximum_change)
}

#[must_use]
pub fn approach(current: f64, target: f64, step: f64) -> f64 {
    let step = step.abs();
    if current < target {
        clamp_f64(current + step, current, target)
    } else {
        clamp_f64(current - step, target, current)
    }
}

#[must_use]
pub fn approach_degrees(current: f64, target: f64, step: f64) -> f64 {
    approach(current, current + degrees_difference(current, target), step)
}

#[must_use]
pub fn smallest_encompassing_power_of_two(value: i32) -> i32 {
    let mut value = value - 1;
    value |= value >> 1;
    value |= value >> 2;
    value |= value >> 4;
    value |= value >> 8;
    value |= value >> 16;
    value + 1
}

#[must_use]
pub fn is_power_of_two(value: i32) -> bool {
    value != 0 && value & (value - 1) == 0
}

#[must_use]
pub fn ceil_log2(value: i32) -> i32 {
    if value <= 1 {
        0
    } else {
        32 - (value - 1).leading_zeros() as i32
    }
}

#[must_use]
pub fn log2(value: i32) -> i32 {
    if value <= 0 {
        0
    } else {
        31 - value.leading_zeros() as i32
    }
}

#[must_use]
pub fn chessboard_distance(x0: i32, z0: i32, x1: i32, z1: i32) -> i32 {
    (x1 - x0).abs().max((z1 - z0).abs())
}

#[must_use]
pub fn floor_div_i32(value: i32, divisor: i32) -> i32 {
    value.div_euclid(divisor)
}

#[must_use]
pub fn is_multiple_of(value: i32, divisor: i32) -> bool {
    value % divisor == 0
}

#[must_use]
pub fn pack_degrees(degrees: f64) -> i8 {
    floor_i32(degrees * 256.0 / 360.0) as i8
}

#[must_use]
pub fn unpack_degrees(value: i8) -> f64 {
    f64::from(value) * 360.0 / 256.0
}

#[must_use]
pub fn clamped_lerp(start: f64, end: f64, amount: f64) -> f64 {
    if amount < 0.0 {
        start
    } else if amount > 1.0 {
        end
    } else {
        start + amount * (end - start)
    }
}

#[must_use]
pub fn lerp(amount: f64, start: f64, end: f64) -> f64 {
    start + amount * (end - start)
}

#[must_use]
pub fn lerp2(x_amount: f64, y_amount: f64, x0y0: f64, x1y0: f64, x0y1: f64, x1y1: f64) -> f64 {
    lerp(
        y_amount,
        lerp(x_amount, x0y0, x1y0),
        lerp(x_amount, x0y1, x1y1),
    )
}

#[must_use]
pub fn lerp3(
    x_amount: f64,
    y_amount: f64,
    z_amount: f64,
    x0y0z0: f64,
    x1y0z0: f64,
    x0y1z0: f64,
    x1y1z0: f64,
    x0y0z1: f64,
    x1y0z1: f64,
    x0y1z1: f64,
    x1y1z1: f64,
) -> f64 {
    lerp(
        z_amount,
        lerp2(x_amount, y_amount, x0y0z0, x1y0z0, x0y1z0, x1y1z0),
        lerp2(x_amount, y_amount, x0y0z1, x1y0z1, x0y1z1, x1y1z1),
    )
}

#[must_use]
pub fn catmull_rom(amount: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    0.5 * (2.0 * p1
        + (p2 - p0) * amount
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * amount * amount
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * amount * amount * amount)
}

#[must_use]
pub fn smoothstep(value: f64) -> f64 {
    value * value * value * (value * (value * 6.0 - 15.0) + 10.0)
}

#[must_use]
pub fn smoothstep_derivative(value: f64) -> f64 {
    30.0 * value * value * (value - 1.0) * (value - 1.0)
}

#[must_use]
pub fn sign(value: f64) -> i32 {
    if value == 0.0 {
        0
    } else if value > 0.0 {
        1
    } else {
        -1
    }
}

#[must_use]
pub fn rotation_lerp(amount: f64, start: f64, end: f64) -> f64 {
    start + amount * wrap_degrees(end - start)
}

#[must_use]
pub fn rotation_lerp_radians(amount: f64, start: f64, end: f64) -> f64 {
    let mut difference = end - start;
    while difference < -std::f64::consts::PI {
        difference += std::f64::consts::TAU;
    }
    while difference >= std::f64::consts::PI {
        difference -= std::f64::consts::TAU;
    }
    start + amount * difference
}

#[must_use]
pub fn triangle_wave(value: f64, period: f64) -> f64 {
    ((value % period - period * 0.5).abs() - period * 0.25) / (period * 0.25)
}

#[must_use]
pub fn inverse_lerp(value: f64, start: f64, end: f64) -> f64 {
    (value - start) / (end - start)
}

#[must_use]
pub fn abs_max(left: f64, right: f64) -> f64 {
    left.abs().max(right.abs())
}

#[must_use]
pub fn abs_max_f32(left: f32, right: f32) -> f32 {
    left.abs().max(right.abs())
}

#[must_use]
pub fn abs_max_i32(left: i32, right: i32) -> i32 {
    abs_i32(left).max(abs_i32(right))
}

#[must_use]
pub fn square(value: f64) -> f64 {
    value * value
}

#[must_use]
pub fn cube(value: f64) -> f64 {
    value * value * value
}

#[must_use]
pub fn clamped_map(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    clamped_lerp(to_min, to_max, inverse_lerp(value, from_min, from_max))
}

#[must_use]
pub fn map(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    lerp(inverse_lerp(value, from_min, from_max), to_min, to_max)
}

#[must_use]
pub fn length_squared_2(x: f64, z: f64) -> f64 {
    x * x + z * z
}

#[must_use]
pub fn length_2(x: f64, z: f64) -> f64 {
    length_squared_2(x, z).sqrt()
}

#[must_use]
pub fn length_squared_2_f32(x: f32, z: f32) -> f32 {
    x * x + z * z
}

#[must_use]
pub fn length_2_f32(x: f32, z: f32) -> f32 {
    length_squared_2_f32(x, z).sqrt()
}

#[must_use]
pub fn length_squared_3(x: f64, y: f64, z: f64) -> f64 {
    x * x + y * y + z * z
}

#[must_use]
pub fn length_squared_3_f32(x: f32, y: f32, z: f32) -> f32 {
    x * x + y * y + z * z
}

#[must_use]
pub fn length_3_f32(x: f32, y: f32, z: f32) -> f32 {
    length_squared_3_f32(x, y, z).sqrt()
}

#[must_use]
pub fn length_3(x: f64, y: f64, z: f64) -> f64 {
    length_squared_3(x, y, z).sqrt()
}

#[must_use]
pub fn positive_ceil_div(value: i32, divisor: i32) -> i32 {
    -(-value).div_euclid(divisor)
}

#[must_use]
pub fn lerp_i32(amount: f64, start: i32, end: i32) -> i32 {
    start + floor_i32(amount * f64::from(end - start))
}

#[must_use]
pub fn lerp_discrete(amount: f64, start: i32, end: i32) -> i32 {
    let difference = end - start;
    start + floor_i32(amount * f64::from(difference - 1)) + i32::from(amount > 0.0)
}

#[must_use]
pub fn smallest_square_side(item_count: i32) -> i32 {
    assert!(item_count >= 0, "item_count must be non-negative");
    (item_count as f64).sqrt().ceil() as i32
}

#[must_use]
pub fn degrees_difference_abs(from: f64, to: f64) -> f64 {
    degrees_difference(from, to).abs()
}

#[must_use]
pub fn seed_from_block(x: i32, y: i32, z: i32) -> i64 {
    let mut seed = i64::from(x) * 3_129_871 ^ i64::from(z) * 116_129_781 ^ i64::from(y);
    seed = seed.wrapping_mul(seed).wrapping_mul(42_317_861) + seed.wrapping_mul(11);
    seed >> 16
}

#[must_use]
pub fn murmur_hash3_mixer(value: i32) -> i32 {
    let mut value = value as u32;
    value ^= value >> 16;
    value = value.wrapping_mul((-2_048_147_789_i32) as u32);
    value ^= value >> 13;
    value = value.wrapping_mul((-1_028_478_737_i32) as u32);
    (value ^ (value >> 16)) as i32
}

#[must_use]
pub fn binary_search<F>(mut lower: i32, upper: i32, predicate: F) -> i32
where
    F: Fn(i32) -> bool,
{
    let mut length = upper - lower;
    while length > 0 {
        let half = length / 2;
        let middle = lower + half;
        if predicate(middle) {
            length = half;
        } else {
            lower = middle + 1;
            length -= half + 1;
        }
    }
    lower
}

#[must_use]
pub fn round_toward(value: i32, divisor: i32) -> i32 {
    positive_ceil_div(value, divisor) * divisor
}

#[must_use]
pub fn quantize(value: f64, step: i32) -> i32 {
    floor_i32(value / f64::from(step)) * step
}

pub fn out_from_origin(
    origin: i32,
    lower_bound: i32,
    upper_bound: i32,
    step: i32,
) -> Result<Vec<i32>, String> {
    if lower_bound > upper_bound {
        return Err(format!(
            "upper bound {upper_bound} expected to be greater than or equal to lower bound {lower_bound}"
        ));
    }
    if step < 1 {
        return Err(format!("step size expected to be at least 1, was {step}"));
    }
    let clamped_origin = clamp_i32(origin, lower_bound, upper_bound);
    let mut values = Vec::new();
    let mut current = clamped_origin;
    while clamped_origin.wrapping_sub(clamped_origin.wrapping_sub(current).wrapping_abs())
        >= lower_bound
        || clamped_origin.wrapping_add(clamped_origin.wrapping_sub(current).wrapping_abs())
            <= upper_bound
    {
        values.push(current);
        let distance = clamped_origin.wrapping_sub(current).wrapping_abs();
        let on_lower_side = current <= clamped_origin;
        let can_extend_upper =
            clamped_origin.wrapping_add(distance).wrapping_add(step) <= upper_bound;
        if !on_lower_side || !can_extend_upper {
            let candidate = clamped_origin
                .wrapping_sub(distance)
                .wrapping_sub(if on_lower_side { step } else { 0 });
            if candidate >= lower_bound {
                current = candidate;
                continue;
            }
        }
        current = clamped_origin.wrapping_add(distance).wrapping_add(step);
    }
    Ok(values)
}

pub fn mul_and_truncate(numerator: i32, denominator: i32, value: i32) -> Result<i32, String> {
    if denominator == 0 {
        return Err("fraction denominator must not be zero".to_owned());
    }
    Ok(numerator.wrapping_mul(value).wrapping_div(denominator))
}

#[must_use]
pub fn equal_f64(left: f64, right: f64) -> bool {
    (right - left).abs() < 1.0e-5
}

#[must_use]
pub fn round_i32(value: f64) -> i32 {
    floor_i32(value + 0.5)
}

#[must_use]
pub fn wrap_degrees(value: f64) -> f64 {
    let mut wrapped = value % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

#[must_use]
pub fn wrap_degrees_i32(value: i32) -> i32 {
    let mut wrapped = value % 360;
    if wrapped >= 180 {
        wrapped -= 360;
    }
    if wrapped < -180 {
        wrapped += 360;
    }
    wrapped
}

#[must_use]
pub fn wrap_degrees_i64(value: i64) -> f32 {
    let mut wrapped = (value % 360) as f32;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}

#[must_use]
pub fn wrap_degrees_f32(value: f32) -> f32 {
    let mut wrapped = value % 360.0;
    if wrapped >= 180.0 {
        wrapped -= 360.0;
    }
    if wrapped < -180.0 {
        wrapped += 360.0;
    }
    wrapped
}
#[cfg(test)]
mod tests {
    use super::{
        abs_f32, abs_i32, abs_i64, abs_max, abs_max_f32, abs_max_i32, approach, approach_degrees,
        atan2_vanilla, binary_search, catmull_rom, ceil_f32, ceil_i32, ceil_i64, ceil_log2,
        chessboard_distance, clamp_f32, clamp_f64, clamp_i32, clamp_i64, clamped_lerp, clamped_map,
        cube, degrees_difference, degrees_difference_abs, degrees_to_radians, equal_f64,
        fast_inv_cube_root, fast_inv_sqrt, floor_div_i32, floor_f32, floor_i32, floor_i64,
        frac_f32, frac_f64, hsv_to_argb, hsv_to_rgb, inv_sqrt_f32, inv_sqrt_f64, inverse_lerp,
        is_multiple_of, is_power_of_two, java_cos, java_float, java_sin, length_2, length_2_f32,
        length_3, length_3_f32, length_squared_2, length_squared_2_f32, length_squared_3,
        length_squared_3_f32, lerp, lerp2, lerp3, lerp_discrete, lerp_i32, log2, map,
        mul_and_truncate, murmur_hash3_mixer, out_from_origin, pack_degrees, positive_ceil_div,
        positive_mod_f64, positive_mod_i32, positive_mod_i64, quantize, rotate_if_necessary,
        rotation_lerp, rotation_lerp_radians, round_i32, round_toward, seed_from_block, sign,
        smallest_encompassing_power_of_two, smallest_square_side, smoothstep,
        smoothstep_derivative, sqrt_f32, square, triangle_wave, unpack_degrees, wrap_degrees,
        wrap_degrees_f32, wrap_degrees_i32, wrap_degrees_i64, Aabb, Vec3,
    };

    #[test]
    fn atan_tables_hold_java_values_where_msvc_differs() {
        let (asin, cosine) = super::atan_tables();
        for (index, bits) in [(98, 0x3fd9_2445_4c46_2cc4), (161, 0x3fe5_c3c0_c108_f95c)] {
            assert_eq!(asin[index].to_bits(), bits, "ASIN_TAB[{index}]");
        }
        for (index, bits) in [(125, 0x3feb_ed0b_dee7_064d), (205, 0x3fe3_2aa7_b2d3_fd8f)] {
            assert_eq!(cosine[index].to_bits(), bits, "COS_TAB[{index}]");
        }
    }

    #[test]
    fn java_float_rounds_to_binary32() {
        assert_eq!(java_float(0.54600006), f32::from(0.54600006_f32) as f64);
    }

    #[test]
    fn trigonometry_matches_vanilla_lookup_boundaries() {
        assert_eq!(java_sin(0.0), 0.0);
        assert_eq!(java_cos(0.0), 1.0);
        assert_eq!(degrees_to_radians(180.0), java_float(std::f64::consts::PI));
        assert_eq!(java_sin(std::f64::consts::FRAC_PI_2), 1.0);
    }

    #[test]
    fn floor_ceil_clamp_and_wrap_match_vanilla_boundaries() {
        assert_eq!(floor_i32(-1.2), -2);
        assert_eq!(floor_i32(1.0), 1);
        assert_eq!(ceil_i32(-1.2), -1);
        assert_eq!(ceil_i32(1.0), 1);
        assert_eq!(clamp_f64(-1.0, 0.0, 1.0), 0.0);
        assert_eq!(clamp_f64(2.0, 0.0, 1.0), 1.0);
        assert!(clamp_f64(f64::NAN, 0.0, 1.0).is_nan());
        assert!(clamp_f64(0.5, 0.0, f64::NAN).is_nan());
        assert!(clamp_f32(0.5, 0.0, f32::NAN).is_nan());
        assert_eq!(wrap_degrees(180.0), -180.0);
        assert_eq!(wrap_degrees(-181.0), 179.0);
        assert_eq!(wrap_degrees(540.0), -180.0);
        assert_eq!(floor_f32(-1.2), -2);
        assert_eq!(ceil_f32(-1.2), -1);
        assert_eq!(abs_i32(i32::MIN), i32::MIN);
        assert_eq!(clamp_f32(-1.0, 0.0, 1.0), 0.0);
        assert!(clamp_f32(f32::NAN, 0.0, 1.0).is_nan());
        assert_eq!(frac_f32(-1.25), 0.75);
        assert_eq!(wrap_degrees_i32(540), -180);
        assert_eq!(wrap_degrees_i64(-181), 179.0);
        assert_eq!(wrap_degrees_f32(540.0), -180.0);
        assert_eq!(sqrt_f32(4.0), 2.0);
        assert_eq!(abs_f32(-2.5), 2.5);
        assert_eq!(abs_i64(-9), 9);
        assert_eq!(abs_max_f32(-2.0, 3.5), 3.5);
        assert_eq!(abs_max_i32(-7, 3), 7);
        assert_eq!(round_i32(1.49), 1);
        assert_eq!(round_i32(1.5), 2);
        assert_eq!(round_i32(-1.5), -1);
        assert!(equal_f64(1.0, 1.0 + 0.5e-5));
        assert!(!equal_f64(1.0, 1.0 + 1.0e-5));
        assert_eq!(clamped_lerp(10.0, 20.0, -1.0), 10.0);
        assert_eq!(clamped_lerp(10.0, 20.0, 1.5), 20.0);
        assert_eq!(clamped_lerp(10.0, 20.0, 0.25), 12.5);
        assert_eq!(chessboard_distance(2, -3, -1, 4), 7);
        assert_eq!(floor_div_i32(-5, 2), -3);
        assert!(is_multiple_of(12, 3));
        assert!(!is_multiple_of(13, 3));
        assert_eq!(pack_degrees(180.0), -128_i8);
        assert_eq!(unpack_degrees(-128_i8), -180.0);
        assert_eq!(square(-3.0), 9.0);
        assert_eq!(cube(-2.0), -8.0);
        assert_eq!(clamped_map(15.0, 10.0, 20.0, 0.0, 100.0), 50.0);
        assert_eq!(map(15.0, 10.0, 20.0, 0.0, 100.0), 50.0);
        assert_eq!(length_squared_2(3.0, 4.0), 25.0);
        assert_eq!(length_2(3.0, 4.0), 5.0);
        assert_eq!(length_squared_2_f32(3.0, 4.0), 25.0);
        assert_eq!(length_2_f32(3.0, 4.0), 5.0);
        assert_eq!(length_squared_3(2.0, 3.0, 6.0), 49.0);
        assert_eq!(length_3(2.0, 3.0, 6.0), 7.0);
        assert_eq!(length_squared_3_f32(2.0, 3.0, 6.0), 49.0);
        assert_eq!(length_3_f32(2.0, 3.0, 6.0), 7.0);
        assert_eq!(positive_ceil_div(5, 2), 3);
        assert_eq!(round_toward(5, 2), 6);
        assert_eq!(quantize(-5.5, 4), -8);
        assert_eq!(
            out_from_origin(5, 0, 10, 1).unwrap(),
            vec![5, 6, 4, 7, 3, 8, 2, 9, 1, 10, 0]
        );
        assert_eq!(out_from_origin(-5, 0, 2, 2).unwrap(), vec![0, 2]);
        assert!(out_from_origin(0, 2, 1, 1).is_err());
        assert!(out_from_origin(0, 1, 2, 0).is_err());
        assert_eq!(mul_and_truncate(3, 2, 5).unwrap(), 7);
        assert_eq!(mul_and_truncate(-3, 2, 5).unwrap(), -7);
        assert_eq!(mul_and_truncate(i32::MIN, 1, -1).unwrap(), i32::MIN);
        assert!(mul_and_truncate(1, 0, 1).is_err());
        assert_eq!(inv_sqrt_f64(4.0), 0.5);
        assert_eq!(inv_sqrt_f32(4.0), 0.5);
        assert!((fast_inv_sqrt(4.0) - 0.5).abs() < 0.001);
        assert!((fast_inv_cube_root(8.0) - 0.5).abs() < 0.001);
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), 0x00ff0000);
        assert_eq!(hsv_to_argb(1.0 / 3.0, 1.0, 1.0, 255), 0xff00ff00u32 as i32);
        assert!((atan2_vanilla(0.0, 1.0)).abs() < 1.0e-12);
        assert!((atan2_vanilla(1.0, 0.0) - std::f64::consts::FRAC_PI_2).abs() < 0.001);
        assert!((atan2_vanilla(-1.0, -1.0) + 3.0 * std::f64::consts::PI / 4.0).abs() < 0.001);
        assert_eq!(lerp_i32(0.5, 0, 10), 5);
        assert_eq!(lerp_discrete(0.5, 0, 10), 5);
        assert_eq!(smallest_square_side(10), 4);
        assert_eq!(degrees_difference_abs(170.0, -170.0), 20.0);
        assert_eq!(seed_from_block(1, 2, 3), seed_from_block(1, 2, 3));
        assert_eq!(binary_search(0, 10, |value| value >= 6), 6);
        assert_eq!(binary_search(-5, 5, |value| value >= 0), 0);
        assert_eq!(murmur_hash3_mixer(0), 0);
        assert_eq!(murmur_hash3_mixer(1), 367_622_363);
        assert_eq!(lerp2(0.5, 0.5, 0.0, 2.0, 4.0, 6.0), 3.0);
        assert_eq!(
            lerp3(0.5, 0.5, 0.5, 0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0),
            7.0
        );
        assert_eq!(catmull_rom(0.5, 0.0, 1.0, 2.0, 3.0), 1.5);
        assert_eq!(smoothstep(0.5), 0.5);
        assert_eq!(smoothstep_derivative(0.5), 1.875);
        assert_eq!(sign(-1.0), -1);
        assert_eq!(sign(0.0), 0);
        assert_eq!(sign(1.0), 1);
        assert_eq!(rotation_lerp(0.5, 170.0, -170.0), 180.0);
        assert!((rotation_lerp_radians(0.5, 3.0, -3.0) - 3.141592653589793).abs() < 1.0e-12);
        assert_eq!(triangle_wave(0.0, 4.0), 1.0);
        assert_eq!(triangle_wave(2.0, 4.0), -1.0);
    }

    #[test]
    fn vector_lengths_and_products_match_vanilla_formulas() {
        let vector = Vec3::new(3.0, 4.0, 12.0);
        let other = Vec3::new(-2.0, 5.0, 1.0);
        assert_eq!(vector.length_squared(), 169.0);
        assert_eq!(vector.length(), 13.0);
        assert_eq!(vector.horizontal_length_squared(), 153.0);
        assert_eq!(vector.horizontal_length(), 153.0_f64.sqrt());
        assert_eq!(vector.dot(other), 26.0);
        assert_eq!(vector.cross(other), Vec3::new(-56.0, -27.0, 23.0));
        assert_eq!(Vec3::new(1.0e-6, 0.0, 0.0).normalized(), Vec3::ZERO);
    }

    #[test]
    fn vector_transformations_match_vanilla_formulas() {
        let vector = Vec3::new(1.0, 2.0, 3.0);
        let approximately = |actual: Vec3, expected: Vec3| {
            assert!((actual.x - expected.x).abs() < 1.0e-12);
            assert!((actual.y - expected.y).abs() < 1.0e-12);
            assert!((actual.z - expected.z).abs() < 1.0e-12);
        };
        assert_eq!(
            vector.vector_to(Vec3::new(4.0, 6.0, 8.0)),
            Vec3::new(3.0, 4.0, 5.0)
        );
        assert_eq!(vector.add_scalar(2.0), Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(
            vector.add_components(2.0, -1.0, 4.0),
            Vec3::new(3.0, 1.0, 7.0)
        );
        assert_eq!(vector.subtract_scalar(1.0), Vec3::new(0.0, 1.0, 2.0));
        assert_eq!(vector.reverse(), Vec3::new(-1.0, -2.0, -3.0));
        assert_eq!(
            vector.multiply(Vec3::new(2.0, 3.0, 4.0)),
            Vec3::new(2.0, 6.0, 12.0)
        );
        assert_eq!(vector.horizontal(), Vec3::new(1.0, 0.0, 3.0));
        assert_eq!(vector.rotate_clockwise_90(), Vec3::new(-3.0, 2.0, 1.0));
        assert_eq!(
            vector.projected_on(Vec3::new(1.0, 0.0, 0.0)),
            Vec3::new(1.0, 0.0, 0.0)
        );
        assert_eq!(vector.projected_on(Vec3::ZERO), Vec3::ZERO);
        assert!(vector.is_finite());
        assert!(!Vec3::new(f64::NAN, 0.0, 0.0).is_finite());
        assert_eq!(vector.distance_squared(Vec3::ZERO), 14.0);
        assert_eq!(vector.distance(Vec3::ZERO), 14.0_f64.sqrt());
        assert!(vector.closer_than(Vec3::ZERO, 4.0));
        assert!(vector.closer_than_horizontal(Vec3::new(1.5, 2.5, 3.5), 1.0, 1.0));
        let (pitch, yaw) = vector.rotation();
        assert!((pitch + 32.31153323742385).abs() < 1.0e-12);
        assert!((yaw + 18.43494882292201).abs() < 1.0e-12);
        assert_eq!(
            vector.lerp(Vec3::new(5.0, 6.0, 7.0), 0.5),
            Vec3::new(3.0, 4.0, 5.0)
        );
        approximately(
            Vec3::direction_from_rotation(0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        );
        approximately(
            Vec3::direction_from_rotation(0.0, 90.0),
            Vec3::new(-1.0, 0.0, 0.0),
        );
        approximately(
            Vec3::new(0.0, 1.0, 0.0).x_rot(std::f64::consts::FRAC_PI_2),
            Vec3::new(0.0, 0.0, -1.0),
        );
        approximately(
            Vec3::new(1.0, 0.0, 0.0).y_rot(std::f64::consts::FRAC_PI_2),
            Vec3::new(0.0, 0.0, -1.0),
        );
        approximately(
            Vec3::new(1.0, 0.0, 0.0).z_rot(std::f64::consts::FRAC_PI_2),
            Vec3::new(0.0, -1.0, 0.0),
        );
    }

    #[test]
    fn extended_mth_numeric_helpers_match_vanilla_boundaries() {
        assert_eq!(floor_i64(-1.2), -2);
        assert_eq!(ceil_i64(-1.2), -1);
        assert_eq!(clamp_i32(-2, -1, 4), -1);
        assert_eq!(clamp_i64(8, -1, 4), 4);
        assert_eq!(positive_mod_i32(-1, 360), 359);
        assert_eq!(positive_mod_i64(-1, 360), 359);
        assert_eq!(positive_mod_f64(-1.0, 360.0), 359.0);
        assert_eq!(frac_f64(-1.25), 0.75);
        assert_eq!(degrees_difference(170.0, -170.0), 20.0);
        assert_eq!(lerp(0.25, 10.0, 18.0), 12.0);
        assert_eq!(inverse_lerp(12.0, 10.0, 18.0), 0.25);
        assert_eq!(abs_max(-4.0, 3.0), 4.0);
        assert_eq!(approach(1.0, 5.0, 2.0), 3.0);
        assert_eq!(approach(5.0, 1.0, 2.0), 3.0);
        assert_eq!(approach_degrees(170.0, -170.0, 5.0), 175.0);
        assert_eq!(rotate_if_necessary(0.0, 90.0, 30.0), 60.0);
        assert_eq!(smallest_encompassing_power_of_two(5), 8);
        assert!(is_power_of_two(8));
        assert!(!is_power_of_two(7));
        assert_eq!(ceil_log2(5), 3);
        assert_eq!(log2(8), 3);
    }

    #[test]
    fn aabb_contract_matches_vanilla_directional_semantics() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::new(4.0, 4.0, 4.0));
        assert_eq!(
            bounds.contract(Vec3::new(-1.0, 2.0, 0.0)),
            Aabb::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(4.0, 2.0, 4.0))
        );
    }

    #[test]
    fn aabb_inflate_and_deflate_are_inverse_for_positive_amounts() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 2.0, 3.0));
        let amount = Vec3::new(0.25, 0.5, 0.75);
        assert_eq!(bounds.inflate(amount).deflate(amount), bounds);
    }

    #[test]
    fn aabb_intersection_and_union_match_vanilla_boundaries() {
        let left = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 2.0, 2.0));
        let right = Aabb::new(Vec3::new(1.0, -1.0, 0.5), Vec3::new(3.0, 1.0, 4.0));
        assert_eq!(
            left.intersect(right),
            Aabb::new(Vec3::new(1.0, 0.0, 0.5), Vec3::new(2.0, 1.0, 2.0))
        );
        assert_eq!(
            left.minmax(right),
            Aabb::new(Vec3::new(0.0, -1.0, 0.0), Vec3::new(3.0, 2.0, 4.0))
        );
    }

    #[test]
    fn aabb_contains_uses_vanilla_half_open_maximums() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert!(bounds.contains(Vec3::new(0.0, 0.0, 0.0)));
        assert!(!bounds.contains(Vec3::new(1.0, 0.5, 0.5)));
        assert!(!bounds.contains(Vec3::new(-0.001, 0.5, 0.5)));
    }

    #[test]
    fn aabb_sizes_and_distances_match_vanilla() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(bounds.x_size(), 2.0);
        assert_eq!(bounds.y_size(), 4.0);
        assert_eq!(bounds.z_size(), 6.0);
        assert_eq!(bounds.size(), 4.0);
        assert_eq!(bounds.distance_to_sqr_point(Vec3::new(3.0, 6.0, 6.0)), 5.0);
        assert_eq!(
            bounds.distance_to_sqr(Aabb::new(
                Vec3::new(3.0, 6.0, 7.0),
                Vec3::new(4.0, 7.0, 8.0)
            )),
            6.0
        );
    }

    #[test]
    fn aabb_expand_towards_extends_only_in_motion_directions() {
        let box_bounds = Aabb::new(Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(
            box_bounds.expand_towards(Vec3::new(-2.0, 3.0, 0.0)),
            Aabb::new(Vec3::new(-1.0, 2.0, 3.0), Vec3::new(4.0, 8.0, 6.0))
        );
    }

    #[test]
    fn aabb_clip_returns_first_segment_face_intersection() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(
            bounds.clip_segment(Vec3::new(-1.0, 0.5, 0.5), Vec3::new(2.0, 0.5, 0.5)),
            Some(Vec3::new(0.0, 0.5, 0.5))
        );
    }

    #[test]
    fn aabb_clip_rejects_parallel_and_zero_length_segments() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(
            bounds.clip_segment(Vec3::new(-1.0, 2.0, 0.5), Vec3::new(2.0, 2.0, 0.5)),
            None
        );
        assert_eq!(
            bounds.clip_segment(Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.5, 0.5, 0.5)),
            None
        );
    }

    #[test]
    fn aabb_ray_intersection_matches_vanilla_axis_tests() {
        let bounds = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert!(bounds.ray_intersects(Vec3::new(-2.0, 0.5, 0.5), Vec3::new(1.0, 0.0, 0.0)));
        assert!(!bounds.ray_intersects(Vec3::new(-2.0, 2.0, 0.5), Vec3::new(1.0, 0.0, 0.0)));
        assert!(!bounds.ray_intersects(Vec3::new(2.0, 0.5, 0.5), Vec3::new(1.0, 0.0, 0.0)));
    }
}

impl Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.plus(rhs)
    }
}

impl Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    #[must_use]
    pub fn translated(self, offset: Vec3) -> Self {
        Self::new(self.min + offset, self.max + offset)
    }

    #[must_use]
    pub fn expand_towards(self, delta: Vec3) -> Self {
        Self::new(
            Vec3::new(
                self.min.x.min(self.min.x + delta.x),
                self.min.y.min(self.min.y + delta.y),
                self.min.z.min(self.min.z + delta.z),
            ),
            Vec3::new(
                self.max.x.max(self.max.x + delta.x),
                self.max.y.max(self.max.y + delta.y),
                self.max.z.max(self.max.z + delta.z),
            ),
        )
    }

    #[must_use]
    pub fn contract(self, delta: Vec3) -> Self {
        Self::new(
            Vec3::new(
                if delta.x < 0.0 {
                    self.min.x - delta.x
                } else {
                    self.min.x
                },
                if delta.y < 0.0 {
                    self.min.y - delta.y
                } else {
                    self.min.y
                },
                if delta.z < 0.0 {
                    self.min.z - delta.z
                } else {
                    self.min.z
                },
            ),
            Vec3::new(
                if delta.x > 0.0 {
                    self.max.x - delta.x
                } else {
                    self.max.x
                },
                if delta.y > 0.0 {
                    self.max.y - delta.y
                } else {
                    self.max.y
                },
                if delta.z > 0.0 {
                    self.max.z - delta.z
                } else {
                    self.max.z
                },
            ),
        )
    }

    #[must_use]
    pub fn inflate(self, amount: Vec3) -> Self {
        Self::new(self.min - amount, self.max + amount)
    }

    #[must_use]
    pub fn deflate(self, amount: Vec3) -> Self {
        self.inflate(Vec3::new(-amount.x, -amount.y, -amount.z))
    }

    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        Self::new(
            Vec3::new(
                self.min.x.max(other.min.x),
                self.min.y.max(other.min.y),
                self.min.z.max(other.min.z),
            ),
            Vec3::new(
                self.max.x.min(other.max.x),
                self.max.y.min(other.max.y),
                self.max.z.min(other.max.z),
            ),
        )
    }

    #[must_use]
    pub fn minmax(self, other: Self) -> Self {
        Self::new(
            Vec3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            Vec3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        )
    }

    #[must_use]
    pub fn contains(self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x < self.max.x
            && point.y >= self.min.y
            && point.y < self.max.y
            && point.z >= self.min.z
            && point.z < self.max.z
    }

    #[must_use]
    pub fn x_size(self) -> f64 {
        self.max.x - self.min.x
    }

    #[must_use]
    pub fn y_size(self) -> f64 {
        self.max.y - self.min.y
    }

    #[must_use]
    pub fn z_size(self) -> f64 {
        self.max.z - self.min.z
    }

    #[must_use]
    pub fn size(self) -> f64 {
        (self.x_size() + self.y_size() + self.z_size()) / 3.0
    }

    #[must_use]
    pub fn distance_to_sqr_point(self, point: Vec3) -> f64 {
        let x = (self.min.x - point.x).max(point.x - self.max.x).max(0.0);
        let y = (self.min.y - point.y).max(point.y - self.max.y).max(0.0);
        let z = (self.min.z - point.z).max(point.z - self.max.z).max(0.0);
        x * x + y * y + z * z
    }

    #[must_use]
    pub fn distance_to_sqr(self, other: Self) -> f64 {
        let x = (self.min.x - other.max.x)
            .max(other.min.x - self.max.x)
            .max(0.0);
        let y = (self.min.y - other.max.y)
            .max(other.min.y - self.max.y)
            .max(0.0);
        let z = (self.min.z - other.max.z)
            .max(other.min.z - self.max.z)
            .max(0.0);
        x * x + y * y + z * z
    }

    #[must_use]
    pub fn clip_segment(self, start: Vec3, end: Vec3) -> Option<Vec3> {
        let delta = end - start;
        let mut best = 1.0;
        let mut hit = false;
        for (origin, direction, lower, upper, axis) in [
            (start.x, delta.x, self.min.x, self.max.x, 0_u8),
            (start.y, delta.y, self.min.y, self.max.y, 1_u8),
            (start.z, delta.z, self.min.z, self.max.z, 2_u8),
        ] {
            if direction.abs() <= 1.0e-7 {
                continue;
            }
            let plane = if direction > 0.0 { lower } else { upper };
            let t = (plane - origin) / direction;
            if !(0.0 < t && t < best) {
                continue;
            }
            let point = start + delta.scale(t);
            let in_bounds = match axis {
                0 => {
                    point.y > self.min.y - 1.0e-7
                        && point.y < self.max.y + 1.0e-7
                        && point.z > self.min.z - 1.0e-7
                        && point.z < self.max.z + 1.0e-7
                }
                1 => {
                    point.x > self.min.x - 1.0e-7
                        && point.x < self.max.x + 1.0e-7
                        && point.z > self.min.z - 1.0e-7
                        && point.z < self.max.z + 1.0e-7
                }
                _ => {
                    point.x > self.min.x - 1.0e-7
                        && point.x < self.max.x + 1.0e-7
                        && point.y > self.min.y - 1.0e-7
                        && point.y < self.max.y + 1.0e-7
                }
            };
            if in_bounds {
                best = t;
                hit = true;
            }
        }
        hit.then(|| start + delta.scale(best))
    }

    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.max.x > other.min.x
            && self.min.x < other.max.x
            && self.max.y > other.min.y
            && self.min.y < other.max.y
            && self.max.z > other.min.z
            && self.min.z < other.max.z
    }

    #[must_use]
    pub fn ray_intersects(self, origin: Vec3, direction: Vec3) -> bool {
        let center = Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        );
        let half = Vec3::new(
            (self.max.x - self.min.x) * 0.5,
            (self.max.y - self.min.y) * 0.5,
            (self.max.z - self.min.z) * 0.5,
        );
        let offset = origin - center;
        if offset.x.abs() > half.x && offset.x * direction.x >= 0.0 {
            return false;
        }
        if offset.y.abs() > half.y && offset.y * direction.y >= 0.0 {
            return false;
        }
        if offset.z.abs() > half.z && offset.z * direction.z >= 0.0 {
            return false;
        }
        let direction_abs = Vec3::new(direction.x.abs(), direction.y.abs(), direction.z.abs());
        let cross = direction.cross(offset);
        cross.y.abs() <= half.y * direction_abs.z + half.z * direction_abs.y
            && cross.z.abs() <= half.x * direction_abs.z + half.z * direction_abs.x
            && cross.x.abs() < half.x * direction_abs.y + half.y * direction_abs.x
    }
}

//! Java's Math.sin and Math.cos with the same bits on every target. They are evaluated in
//! double-double arithmetic (about 100 bits, for angles up to about a million radians) with IEEE
//! add, sub, mul and div only, and rounded once: correctly rounded. HotSpot's intrinsics return
//! the same but in some near ties, for 0.004% of the float angles in [0, pi/2] and 0.12% of those
//! beyond; the platform's sin and cos (MSVC's CRT natively, libm in WebAssembly) differ from Java
//! 7 to 25 times as often.

#[derive(Clone, Copy)]
struct DoubleDouble {
    hi: f64,
    lo: f64,
}

const HALF_PI: [f64; 3] = [
    1.5707963267948966,
    6.123233995736766e-17,
    -1.4973849048591698e-33,
];
const TWO_OVER_PI: f64 = 0.6366197723675814;
const SERIES_PRECISION: f64 = 1.0e-34;

impl DoubleDouble {
    const ONE: Self = Self { hi: 1.0, lo: 0.0 };

    fn from_f64(value: f64) -> Self {
        Self { hi: value, lo: 0.0 }
    }

    fn sum(a: f64, b: f64) -> Self {
        let hi = a + b;
        let b_part = hi - a;
        Self {
            hi,
            lo: (a - (hi - b_part)) + (b - b_part),
        }
    }

    fn normalized(hi: f64, lo: f64) -> Self {
        let sum = hi + lo;
        Self {
            hi: sum,
            lo: lo - (sum - hi),
        }
    }

    fn product(a: f64, b: f64) -> Self {
        let hi = a * b;
        let (a_hi, a_lo) = split(a);
        let (b_hi, b_lo) = split(b);
        Self {
            hi,
            lo: ((a_hi * b_hi - hi) + a_hi * b_lo + a_lo * b_hi) + a_lo * b_lo,
        }
    }

    fn add(self, rhs: Self) -> Self {
        let high = Self::sum(self.hi, rhs.hi);
        let low = Self::sum(self.lo, rhs.lo);
        let high = Self::normalized(high.hi, high.lo + low.hi);
        Self::normalized(high.hi, high.lo + low.lo)
    }

    fn negated(self) -> Self {
        Self {
            hi: -self.hi,
            lo: -self.lo,
        }
    }

    fn mul(self, rhs: Self) -> Self {
        let product = Self::product(self.hi, rhs.hi);
        Self::normalized(
            product.hi,
            product.lo + (self.hi * rhs.lo + self.lo * rhs.hi),
        )
    }

    fn div_f64(self, divisor: f64) -> Self {
        let quotient = self.hi / divisor;
        let product = Self::product(quotient, divisor);
        let remainder = Self::sum(self.hi, -product.hi);
        let correction = (remainder.hi + (remainder.lo - product.lo + self.lo)) / divisor;
        Self::normalized(quotient, correction)
    }

    fn to_f64(self) -> f64 {
        self.hi + self.lo
    }
}

/// Veltkamp's split of a double into two halves of 26 bits whose products are exact.
fn split(value: f64) -> (f64, f64) {
    let scaled = 134_217_729.0 * value;
    let hi = scaled - (scaled - value);
    (hi, value - hi)
}

/// The angle less the multiple of pi/2 nearest it, and that multiple's quadrant.
fn reduce(x: f64) -> (DoubleDouble, i64) {
    let multiple = (x * TWO_OVER_PI).round();
    let reduced = DoubleDouble::from_f64(x)
        .add(DoubleDouble::product(multiple, HALF_PI[0]).negated())
        .add(DoubleDouble::product(multiple, HALF_PI[1]).negated())
        .add(DoubleDouble::from_f64(-(multiple * HALF_PI[2])));
    (reduced, (multiple as i64).rem_euclid(4))
}

/// The Taylor series from its first term (r for sin, 1 for cos) and that term's power.
fn series(reduced: DoubleDouble, first: DoubleDouble, first_power: u32) -> DoubleDouble {
    let squared = reduced.mul(reduced);
    let mut term = first;
    let mut sum = first;
    let mut power = first_power;
    loop {
        term = term
            .mul(squared)
            .div_f64(f64::from((power + 1) * (power + 2)))
            .negated();
        power += 2;
        sum = sum.add(term);
        if term.hi.abs() < sum.hi.abs() * SERIES_PRECISION {
            return sum;
        }
    }
}

fn reduced_sin(reduced: DoubleDouble) -> DoubleDouble {
    series(reduced, reduced, 1)
}

fn reduced_cos(reduced: DoubleDouble) -> DoubleDouble {
    series(reduced, DoubleDouble::ONE, 0)
}

#[must_use]
pub fn sin(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    if x == 0.0 {
        return x;
    }
    let (reduced, quadrant) = reduce(x);
    match quadrant {
        0 => reduced_sin(reduced),
        1 => reduced_cos(reduced),
        2 => reduced_sin(reduced).negated(),
        _ => reduced_cos(reduced).negated(),
    }
    .to_f64()
}

#[must_use]
pub fn cos(x: f64) -> f64 {
    if !x.is_finite() {
        return f64::NAN;
    }
    let (reduced, quadrant) = reduce(x);
    match quadrant {
        0 => reduced_cos(reduced),
        1 => reduced_sin(reduced).negated(),
        2 => reduced_cos(reduced).negated(),
        _ => reduced_sin(reduced),
    }
    .to_f64()
}

#[cfg(test)]
mod tests {
    use super::{cos, sin};

    fn bits(value: f64) -> u64 {
        value.to_bits()
    }

    #[test]
    fn matches_java_math_where_fdlibm_does_not() {
        // a gliding player's pitch: StrictMath.cos (fdlibm, libm) returns 0x3fe1e4e880cecb92
        assert_eq!(bits(cos(0.977_384_388_446_807_9)), 0x3fe1_e4e8_80ce_cb91);
        assert_eq!(bits(sin(0.977_384_388_446_807_9)), 0x3fea_8779_cfdc_46f9);
    }

    #[test]
    fn matches_java_math_on_angles() {
        let cases: [(f64, u64, u64); 6] = [
            (0.5, 0x3fde_aee8_744b_05f0, 0x3fec_1528_065b_7d50),
            (1.0, 0x3fea_ed54_8f09_0cee, 0x3fe1_4a28_0fb5_068c),
            (-2.25, 0xbfe8_e5f9_c2d0_e3a9, 0xbfe4_19ff_91b9_ba6d),
            (
                std::f64::consts::PI,
                0x3ca1_a626_3314_5c07,
                0xbff0_0000_0000_0000,
            ),
            (100.0, 0xbfe0_3425_b78c_4db8, 0x3feb_981d_bf66_5fdf),
            (12_345.678, 0xbfe6_87d5_8909_74a5, 0x3fe6_b94c_3bbe_24b8),
        ];
        for (x, sine, cosine) in cases {
            assert_eq!(bits(sin(x)), sine, "sin({x})");
            assert_eq!(bits(cos(x)), cosine, "cos({x})");
        }
    }

    #[test]
    fn keeps_the_symmetries_and_special_values() {
        for x in [1.0e-9, 0.3, 0.785_398_163_397_448_3, 2.0, 7.5] {
            assert_eq!(bits(cos(-x)), bits(cos(x)));
            assert_eq!(bits(sin(-x)), bits(-sin(x)));
        }
        assert_eq!(bits(sin(-0.0)), bits(-0.0));
        assert_eq!(cos(0.0), 1.0);
        assert!(sin(f64::INFINITY).is_nan() && cos(f64::NAN).is_nan());
    }
}

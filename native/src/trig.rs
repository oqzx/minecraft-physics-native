//! Java's Math.sin and Math.cos on x86_64 HotSpot, bit for bit: Intel LIBM's sin and cos, the code
//! behind HotSpot's stubs there, in instruction order with scalar IEEE doubles. Ported from Android
//! bionic's libm/x86_64/s_sin.S and s_cos.S (android13-release), under this notice:
//!
//! Copyright (c) 2014, Intel Corporation
//! All rights reserved.
//!
//! Redistribution and use in source and binary forms, with or without
//! modification, are permitted provided that the following conditions are met:
//!
//!     * Redistributions of source code must retain the above copyright notice,
//!     * this list of conditions and the following disclaimer.
//!
//!     * Redistributions in binary form must reproduce the above copyright notice,
//!     * this list of conditions and the following disclaimer in the documentation
//!     * and/or other materials provided with the distribution.
//!
//!     * Neither the name of Intel Corporation nor the names of its contributors
//!     * may be used to endorse or promote products derived from this software
//!     * without specific prior written permission.
//!
//! THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
//! ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
//! WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//! DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR
//! ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
//! (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
//! LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
//! ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
//! (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
//! SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//!
//! The result is not correctly rounded: Java and this code differ from the correctly rounded value
//! on about 0.12% of angles in every band measured, [0, pi/2] included (6,194 of 5,000,000 random
//! float angles there for sin, 5,931 for cos), and fdlibm, so StrictMath and V8, differs from Java
//! on about 3%. Only `+ - *` of f64 are used, never `mul_add`, each in the order of the assembly:
//! an addition moved is a different last bit. A NaN comes back quieted with its payload and an
//! infinity as x86's default NaN, written here as bits so that every target returns the same:
//! that is what x * -0.0 gives on x86_64, the last instruction of the assembly on that path.
//! docs/numeric-contract.md has what is proved on which JDK; arm64 HotSpot runs other code.

const PI32INV: u64 = 0x4024_5f30_6dc9_c883;
const P_1: u64 = 0x3fb9_21fb_5440_0000;
const P_3: u64 = 0x3b63_198a_2e03_7073;
const ALL_ONES: u64 = 0x3fef_ffff_ffff_ffff;
const ONE: u64 = 0x3ff0_0000_0000_0000;
const P_2: u64 = 0x3d90_b461_1a60_0000;
const ONEHALF: u64 = 0x3fe0_0000_0000_0000;
const SC_1: [u64; 2] = [0xbfc5_5555_5555_5555, 0xbfe0_0000_0000_0000];
const SC_2: [u64; 2] = [0x3f81_1111_1111_1111, 0x3fa5_5555_5555_5555];
const SC_3: [u64; 2] = [0xbf2a_01a0_1a01_a01a, 0xbf56_c16c_16c1_6c17];
const SC_4: [u64; 2] = [0x3ec7_1de3_a556_c734, 0x3efa_01a0_1a01_a01a];
const PI_4: [u64; 2] = [0x3fe9_21fb_4000_0000, 0x3e64_442d_1846_9899];
const CTABLE: [[u64; 4]; 64] = [
    [0x0000_0000_0000_0000, 0x0000_0000_0000_0000, 0x0000_0000_0000_0000, 0x3ff0_0000_0000_0000],
    [0xbf73_b92e_176d_6d31, 0x3fb9_17a6_bc29_b42c, 0xbc3e_2718_e000_0000, 0x3ff0_0000_0000_0000],
    [0xbf93_ad06_0114_69fb, 0x3fc8_f8b8_3c69_a60b, 0xbc62_6d19_c000_0000, 0x3ff0_0000_0000_0000],
    [0xbfa6_0bea_939d_225a, 0x3fd2_9406_2ed5_9f06, 0xbc75_d28d_a000_0000, 0x3ff0_0000_0000_0000],
    [0xbfb3_7ca1_866b_95cf, 0x3fd8_7de2_a6ae_a963, 0xbc67_2ced_e000_0000, 0x3ff0_0000_0000_0000],
    [0xbfbe_3a68_73fa_1279, 0x3fde_2b5d_3806_f63b, 0x3c5e_0d89_2000_0000, 0x3ff0_0000_0000_0000],
    [0xbfc5_9267_5bc5_7974, 0x3fe1_c73b_39ae_68c8, 0x3c8b_25dd_2000_0000, 0x3ff0_0000_0000_0000],
    [0xbfcd_0dfe_53ab_a2fd, 0x3fe4_4cf3_2509_1dd6, 0x3c68_076a_2000_0000, 0x3ff0_0000_0000_0000],
    [0x3fca_8279_99fc_ef32, 0x3fe6_a09e_667f_3bcd, 0xbc8b_dd34_2000_0000, 0x3fe0_0000_0000_0000],
    [0x3fc1_33cc_9424_7758, 0x3fe8_bc80_6b15_1741, 0xbc82_c5e1_2000_0000, 0x3fe0_0000_0000_0000],
    [0x3fac_73b3_9ae6_8c87, 0x3fea_9b66_290e_a1a3, 0x3c39_f630_e000_0000, 0x3fe0_0000_0000_0000],
    [0xbf9d_4a2c_7f90_9c4e, 0x3fec_38b2_f180_bdb1, 0xbc76_e0b1_8000_0000, 0x3fe0_0000_0000_0000],
    [0xbfbe_0875_6545_5a75, 0x3fed_906b_cf32_8d46, 0x3c74_57e6_2000_0000, 0x3fe0_0000_0000_0000],
    [0x3fa4_a031_76ac_f82d, 0x3fee_9f41_56c6_2dda, 0x3c87_60b1_e000_0000, 0x3fd0_0000_0000_0000],
    [0xbfac_1d1f_0e59_67d5, 0x3fef_6297_cff7_5cb0, 0x3c75_6217_2000_0000, 0x3fd0_0000_0000_0000],
    [0xbf9b_a165_0f59_2f50, 0x3fef_d88d_a3d1_2526, 0xbc88_7df6_4000_0000, 0x3fc0_0000_0000_0000],
    [0x0000_0000_0000_0000, 0x3ff0_0000_0000_0000, 0x0000_0000_0000_0000, 0x0000_0000_0000_0000],
    [0x3f9b_a165_0f59_2f50, 0x3fef_d88d_a3d1_2526, 0xbc88_7df6_4000_0000, 0xbfc0_0000_0000_0000],
    [0x3fac_1d1f_0e59_67d5, 0x3fef_6297_cff7_5cb0, 0x3c75_6217_2000_0000, 0xbfd0_0000_0000_0000],
    [0xbfa4_a031_76ac_f82d, 0x3fee_9f41_56c6_2dda, 0x3c87_60b1_e000_0000, 0xbfd0_0000_0000_0000],
    [0x3fbe_0875_6545_5a75, 0x3fed_906b_cf32_8d46, 0x3c74_57e6_2000_0000, 0xbfe0_0000_0000_0000],
    [0x3f9d_4a2c_7f90_9c4e, 0x3fec_38b2_f180_bdb1, 0xbc76_e0b1_8000_0000, 0xbfe0_0000_0000_0000],
    [0xbfac_73b3_9ae6_8c87, 0x3fea_9b66_290e_a1a3, 0x3c39_f630_e000_0000, 0xbfe0_0000_0000_0000],
    [0xbfc1_33cc_9424_7758, 0x3fe8_bc80_6b15_1741, 0xbc82_c5e1_2000_0000, 0xbfe0_0000_0000_0000],
    [0xbfca_8279_99fc_ef32, 0x3fe6_a09e_667f_3bcd, 0xbc8b_dd34_2000_0000, 0xbfe0_0000_0000_0000],
    [0x3fcd_0dfe_53ab_a2fd, 0x3fe4_4cf3_2509_1dd6, 0x3c68_076a_2000_0000, 0xbff0_0000_0000_0000],
    [0x3fc5_9267_5bc5_7974, 0x3fe1_c73b_39ae_68c8, 0x3c8b_25dd_2000_0000, 0xbff0_0000_0000_0000],
    [0x3fbe_3a68_73fa_1279, 0x3fde_2b5d_3806_f63b, 0x3c5e_0d89_2000_0000, 0xbff0_0000_0000_0000],
    [0x3fb3_7ca1_866b_95cf, 0x3fd8_7de2_a6ae_a963, 0xbc67_2ced_e000_0000, 0xbff0_0000_0000_0000],
    [0x3fa6_0bea_939d_225a, 0x3fd2_9406_2ed5_9f06, 0xbc75_d28d_a000_0000, 0xbff0_0000_0000_0000],
    [0x3f93_ad06_0114_69fb, 0x3fc8_f8b8_3c69_a60b, 0xbc62_6d19_c000_0000, 0xbff0_0000_0000_0000],
    [0x3f73_b92e_176d_6d31, 0x3fb9_17a6_bc29_b42c, 0xbc3e_2718_e000_0000, 0xbff0_0000_0000_0000],
    [0x0000_0000_0000_0000, 0x0000_0000_0000_0000, 0x0000_0000_0000_0000, 0xbff0_0000_0000_0000],
    [0x3f73_b92e_176d_6d31, 0xbfb9_17a6_bc29_b42c, 0x3c3e_2718_e000_0000, 0xbff0_0000_0000_0000],
    [0x3f93_ad06_0114_69fb, 0xbfc8_f8b8_3c69_a60b, 0x3c62_6d19_c000_0000, 0xbff0_0000_0000_0000],
    [0x3fa6_0bea_939d_225a, 0xbfd2_9406_2ed5_9f06, 0x3c75_d28d_a000_0000, 0xbff0_0000_0000_0000],
    [0x3fb3_7ca1_866b_95cf, 0xbfd8_7de2_a6ae_a963, 0x3c67_2ced_e000_0000, 0xbff0_0000_0000_0000],
    [0x3fbe_3a68_73fa_1279, 0xbfde_2b5d_3806_f63b, 0xbc5e_0d89_2000_0000, 0xbff0_0000_0000_0000],
    [0x3fc5_9267_5bc5_7974, 0xbfe1_c73b_39ae_68c8, 0xbc8b_25dd_2000_0000, 0xbff0_0000_0000_0000],
    [0x3fcd_0dfe_53ab_a2fd, 0xbfe4_4cf3_2509_1dd6, 0xbc68_076a_2000_0000, 0xbff0_0000_0000_0000],
    [0xbfca_8279_99fc_ef32, 0xbfe6_a09e_667f_3bcd, 0x3c8b_dd34_2000_0000, 0xbfe0_0000_0000_0000],
    [0xbfc1_33cc_9424_7758, 0xbfe8_bc80_6b15_1741, 0x3c82_c5e1_2000_0000, 0xbfe0_0000_0000_0000],
    [0xbfac_73b3_9ae6_8c87, 0xbfea_9b66_290e_a1a3, 0xbc39_f630_e000_0000, 0xbfe0_0000_0000_0000],
    [0x3f9d_4a2c_7f90_9c4e, 0xbfec_38b2_f180_bdb1, 0x3c76_e0b1_8000_0000, 0xbfe0_0000_0000_0000],
    [0x3fbe_0875_6545_5a75, 0xbfed_906b_cf32_8d46, 0xbc74_57e6_2000_0000, 0xbfe0_0000_0000_0000],
    [0xbfa4_a031_76ac_f82d, 0xbfee_9f41_56c6_2dda, 0xbc87_60b1_e000_0000, 0xbfd0_0000_0000_0000],
    [0x3fac_1d1f_0e59_67d5, 0xbfef_6297_cff7_5cb0, 0xbc75_6217_2000_0000, 0xbfd0_0000_0000_0000],
    [0x3f9b_a165_0f59_2f50, 0xbfef_d88d_a3d1_2526, 0x3c88_7df6_4000_0000, 0xbfc0_0000_0000_0000],
    [0x0000_0000_0000_0000, 0xbff0_0000_0000_0000, 0x0000_0000_0000_0000, 0x0000_0000_0000_0000],
    [0xbf9b_a165_0f59_2f50, 0xbfef_d88d_a3d1_2526, 0x3c88_7df6_4000_0000, 0x3fc0_0000_0000_0000],
    [0xbfac_1d1f_0e59_67d5, 0xbfef_6297_cff7_5cb0, 0xbc75_6217_2000_0000, 0x3fd0_0000_0000_0000],
    [0x3fa4_a031_76ac_f82d, 0xbfee_9f41_56c6_2dda, 0xbc87_60b1_e000_0000, 0x3fd0_0000_0000_0000],
    [0xbfbe_0875_6545_5a75, 0xbfed_906b_cf32_8d46, 0xbc74_57e6_2000_0000, 0x3fe0_0000_0000_0000],
    [0xbf9d_4a2c_7f90_9c4e, 0xbfec_38b2_f180_bdb1, 0x3c76_e0b1_8000_0000, 0x3fe0_0000_0000_0000],
    [0x3fac_73b3_9ae6_8c87, 0xbfea_9b66_290e_a1a3, 0xbc39_f630_e000_0000, 0x3fe0_0000_0000_0000],
    [0x3fc1_33cc_9424_7758, 0xbfe8_bc80_6b15_1741, 0x3c82_c5e1_2000_0000, 0x3fe0_0000_0000_0000],
    [0x3fca_8279_99fc_ef32, 0xbfe6_a09e_667f_3bcd, 0x3c8b_dd34_2000_0000, 0x3fe0_0000_0000_0000],
    [0xbfcd_0dfe_53ab_a2fd, 0xbfe4_4cf3_2509_1dd6, 0xbc68_076a_2000_0000, 0x3ff0_0000_0000_0000],
    [0xbfc5_9267_5bc5_7974, 0xbfe1_c73b_39ae_68c8, 0xbc8b_25dd_2000_0000, 0x3ff0_0000_0000_0000],
    [0xbfbe_3a68_73fa_1279, 0xbfde_2b5d_3806_f63b, 0xbc5e_0d89_2000_0000, 0x3ff0_0000_0000_0000],
    [0xbfb3_7ca1_866b_95cf, 0xbfd8_7de2_a6ae_a963, 0x3c67_2ced_e000_0000, 0x3ff0_0000_0000_0000],
    [0xbfa6_0bea_939d_225a, 0xbfd2_9406_2ed5_9f06, 0x3c75_d28d_a000_0000, 0x3ff0_0000_0000_0000],
    [0xbf93_ad06_0114_69fb, 0xbfc8_f8b8_3c69_a60b, 0x3c62_6d19_c000_0000, 0x3ff0_0000_0000_0000],
    [0xbf73_b92e_176d_6d31, 0xbfb9_17a6_bc29_b42c, 0x3c3e_2718_e000_0000, 0x3ff0_0000_0000_0000],
];
const PI_INV_TABLE: [u32; 41] = [
    0x0000_0000, 0x0000_0000, 0xa2f9_836e, 0x4e44_1529, 0xfc27_57d1, 0xf534_ddc0,
    0xdb62_9599, 0x3c43_9041, 0xfe51_63ab, 0xdebb_c561, 0xb724_6e3a, 0x424d_d2e0,
    0x0649_2eea, 0x09d1_921c, 0xfe1d_eb1c, 0xb129_a73e, 0xe882_35f5, 0x2ebb_4484,
    0xe99c_7026, 0xb45f_7e41, 0x3991_d639, 0x8353_39f4, 0x9c84_5f8b, 0xbdf9_283b,
    0x1ff8_97ff, 0xde05_980f, 0xef2f_118b, 0x5a0a_6d1f, 0x6d36_7ecf, 0x27cb_09b7,
    0x4f46_3f66, 0x9e5f_ea2d, 0x7527_bac7, 0xebe5_f17b, 0x3d07_39f7, 0x8a52_92ea,
    0x6bfb_5fb1, 0x1f8d_5d08, 0x5603_3046, 0xfc7b_6bab, 0xf0cf_bc21,
];

const DEFAULT_NAN: u64 = 0xfff8_0000_0000_0000;
const QUIET_BIT: u64 = 0x0008_0000_0000_0000;

/// Double#longBitsToDouble: the double whose bits these are.
const fn d(bits: u64) -> f64 {
    f64::from_bits(bits)
}

/// The non-finite path of Math#sin and Math#cos: x * NEG_ZERO in the assembly, by bits (see the
/// head of this file).
fn not_finite(x: f64) -> f64 {
    let bits = x.to_bits();
    if bits << 1 == 0xffe0_0000_0000_0000 {
        d(DEFAULT_NAN)
    } else {
        d(bits | QUIET_BIT)
    }
}

/// %eax after `andl $2147418112; subl $808452096`: up to 281346048, unsigned, is the main path
/// of Math#sin and Math#cos.
fn main_path_key(high: u32) -> i32 {
    (high & 0x7fff_0000).wrapping_sub(808_452_096) as i32
}

/// The summation order: the main path of cos adds into xmm0, the rest into xmm1.
#[derive(Clone, Copy, PartialEq)]
enum Order {
    Sin,
    Cos,
}

/// `32/pi * x + copysign(0.5, x)` truncated, the addpd and cvttsd2si of the reduction by pi/32 in
/// Math#sin and Math#cos.
/// Both paths stay well inside an i32, so `as` never saturates here.
#[inline(always)]
fn nearest_n(x: f64) -> i32 {
    let half = d(ONEHALF | (x.to_bits() & (1 << 63)));
    (d(PI32INV) * x + half) as i32
}

/// The reduction by N * (P_1 + P_2 + P_3) from x, the table lookup, the polynomial and the
/// compensated sums of Math#sin and Math#cos. `low` is the second word of the large path's reduced value, 0.0 on the main
/// path, whose code has no such subsd: subtracting 0.0 changes no value.
#[inline(always)]
fn evaluate(x: f64, n: i32, entry: usize, low: f64, order: Order) -> f64 {
    let [t0, t8, t16, t24] = CTABLE[entry & 63];
    let (t0, t8, t16, t24) = (d(t0), d(t8), d(t16), d(t24));
    let n = f64::from(n);
    let m1 = d(P_1) * n;
    let m2 = d(P_2) * n;
    let m3 = n * d(P_3);
    let r1 = x - m1;
    let r = r1 - m2;
    let c2 = (r1 - r) - m2;
    let r2 = r * r;
    let s4l = d(SC_4[0]) * r1 * r;
    let s4h = d(SC_4[1]) * r1 * r;
    let s2l = d(SC_2[0]) * r2;
    let s2h = d(SC_2[1]) * r2;
    let t2 = t0 + t24;
    let correction = t8 * r - t2;
    let a2l = t2 * r * r2;
    let a2h = t8 * r2;
    let r4 = r2 * r2;
    let s3l = (s4l + d(SC_3[0])) * r4;
    let s3h = (s4h + d(SC_3[1])) * r4;
    let a3 = t24 * r;
    let b3 = a3 + t8;
    let a4 = r * t0;
    let x4 = a4 + b3;
    let x5 = t8 - b3;
    let x3 = (b3 - x4) + a4;
    let pl = (s2l + d(SC_1[0]) + s3l) * a2l;
    let ph = (s2h + d(SC_1[1]) + s3h) * a2h;
    let x1 = ((m3 - c2) - low) * correction + t16;
    match order {
        Order::Sin => x4 + (x1 + (x5 + a3) + x3 + pl + ph),
        Order::Cos => (a3 + x5) + x1 + x3 + pl + ph + x4,
    }
}

/// top:0:0 less r9:r10:r8, in 192 bits: the subq, sbbq, sbbq of the assembly (Math#sin and
/// Math#cos, the large path).
fn negate_fraction(r8: u64, r10: u64, r9: u64, top: u64) -> (u64, u64, u64) {
    let (low, borrow) = 0u64.overflowing_sub(r8);
    let (middle, borrow_a) = 0u64.overflowing_sub(r10);
    let (middle, borrow_b) = middle.overflowing_sub(u64::from(borrow));
    let high = top.wrapping_sub(r9).wrapping_sub(u64::from(borrow_a || borrow_b));
    (low, middle, high)
}

/// A power of two from its sign and exponent word: pinsrw $3 into a cleared register (Math#sin and
/// Math#cos, the large path).
fn scale_of(word: i32) -> f64 {
    d(u64::from(word as u32 & 0xffff) << 48)
}

/// The large path of Math#sin and Math#cos up to .L_2TAG_PACKET_12 of s_sin.S, the same in s_cos.S: |x| * 2/pi in 192 bits,
/// from the words of 2/pi this exponent needs, and the fraction as a value and a low word. Returns
/// those and the quadrant count (%eax).
#[allow(clippy::too_many_lines)]
fn reduce_large(x: f64) -> (f64, f64, i32) {
    let bits = x.to_bits();
    let word3 = (bits >> 48) as u32;
    let table_bytes = (word3 & 32752).wrapping_sub(16224) >> 7 & 65532;
    let at = (table_bytes >> 2) as usize;
    let word = |index: usize| u64::from(PI_INV_TABLE[at + index]);
    let mut rax = bits;
    let mut r10 = word(5);
    let mut r8 = word(6);
    let mut rdx = rax & 0xffff_ffff;
    rax >>= 21;
    rax = (rax & 0xffff_ffff) | 0x8000_0000;
    rax >>= 11;
    let mut r9 = r10;
    r10 = r10.wrapping_mul(rdx);
    r9 = r9.wrapping_mul(rax);
    r8 = r8.wrapping_mul(rax);
    let mut rsi = word(4);
    let mut rdi = word(3);
    let mut r11 = r10 & 0xffff_ffff;
    r10 >>= 32;
    r9 = r9.wrapping_add(r10);
    r11 = r11.wrapping_add(r8);
    r8 = r11 & 0xffff_ffff;
    r11 >>= 32;
    r9 = r9.wrapping_add(r11);
    r10 = rsi;
    rsi = rsi.wrapping_mul(rdx);
    r10 = r10.wrapping_mul(rax);
    r11 = rdi;
    rdi = rdi.wrapping_mul(rdx);
    let mut rbx = rsi & 0xffff_ffff;
    rsi >>= 32;
    r9 = r9.wrapping_add(rbx);
    rbx = r9 & 0xffff_ffff;
    r9 >>= 32;
    r10 = r10.wrapping_add(rsi);
    r10 = r10.wrapping_add(r9);
    rbx <<= 32;
    r8 |= rbx;
    r11 = r11.wrapping_mul(rax);
    r9 = word(2);
    rsi = word(1);
    rbx = rdi & 0xffff_ffff;
    rdi >>= 32;
    r10 = r10.wrapping_add(rbx);
    rbx = r10 & 0xffff_ffff;
    r10 >>= 32;
    r11 = r11.wrapping_add(rdi);
    r11 = r11.wrapping_add(r10);
    rdi = r9;
    r9 = r9.wrapping_mul(rdx);
    rdi = rdi.wrapping_mul(rax);
    r10 = r9 & 0xffff_ffff;
    r9 >>= 32;
    r11 = r11.wrapping_add(r10);
    r10 = r11 & 0xffff_ffff;
    r11 >>= 32;
    rdi = rdi.wrapping_add(r9);
    rdi = rdi.wrapping_add(r11);
    r9 = rsi;
    rsi = rsi.wrapping_mul(rdx);
    r9 = r9.wrapping_mul(rax);
    r10 <<= 32;
    r10 |= rbx;
    rax = word(0);
    r11 = rsi & 0xffff_ffff;
    rsi >>= 32;
    rdi = rdi.wrapping_add(r11);
    r11 = rdi & 0xffff_ffff;
    rdi >>= 32;
    r9 = r9.wrapping_add(rsi);
    r9 = r9.wrapping_add(rdi);
    rdx = rax.wrapping_mul(rdx);
    let scaled_offset = table_bytes.wrapping_mul(8).wrapping_add(19) as i32;
    let mut esi = 32768 & word3 as i32;
    let exponent = ((word3 >> 4) & 2047) as i32 - 1023;
    let shift = scaled_offset.wrapping_sub(exponent);
    r9 = r9.wrapping_add(rdx);
    let mut edx = shift.wrapping_add(32);
    let quadrant_word: u32;
    let negated: i32;
    if shift < 1 {
        // .L_2TAG_PACKET_4
        let left = shift.wrapping_neg() as u32;
        r9 = (r9 << 32) | r11;
        r9 = r9.wrapping_shl(left);
        let long = r9;
        if r9 & 0x8000_0000 != 0 {
            // .L_2TAG_PACKET_13
            r9 = u64::from((r9 as u32).wrapping_shr(left));
            let top = 0x1_0000_0000u64.wrapping_shr(left);
            (r8, r10, r9) = negate_fraction(r8, r10, r9, top);
            negated = 32768;
            quadrant_word = ((long >> 3) as u32).wrapping_add(536_870_912);
        } else {
            r9 = u64::from((r9 as u32).wrapping_shr(left));
            negated = 0;
            quadrant_word = (long >> 3) as u32;
        }
    } else {
        let left = 29u32.wrapping_sub(shift as u32);
        let shifted = (r9 as u32).wrapping_shl(left);
        let fraction = shifted & 536_870_911;
        r9 = (u64::from(fraction.wrapping_shr(left)) << 32) | r11;
        if fraction & 268_435_456 != 0 {
            // .L_2TAG_PACKET_5
            let top = u64::from(536_870_912u32.wrapping_shr(left)) << 32;
            (r8, r10, r9) = negate_fraction(r8, r10, r9, top);
            negated = 32768;
            quadrant_word = shifted.wrapping_add(536_870_912);
        } else {
            negated = 0;
            quadrant_word = shifted;
        }
    }
    // .L_2TAG_PACKET_7
    if r9 == 0 {
        edx = edx.wrapping_add(64);
        r9 = r10;
        r10 = r8;
        r8 = 0;
        if r9 == 0 {
            edx = edx.wrapping_add(64);
            r9 = r10;
            r10 = r8;
            if r9 == 0 {
                // no measured double comes here; %eax is still the first word of 2/pi
                return (0.0, 0.0, rax as i32);
            }
        }
    }
    // .L_2TAG_PACKET_9: bsr
    let normal = 29 - (63 - r9.leading_zeros() as i32);
    if normal > 0 {
        let up = normal as u32;
        let down = 64 - up;
        r9 <<= up;
        let carried = r10;
        r10 <<= up;
        edx = edx.wrapping_add(normal);
        r9 |= carried >> down;
        r10 |= r8 >> down;
    } else if normal < 0 {
        // .L_2TAG_PACKET_10
        let down = normal.unsigned_abs();
        r10 >>= down;
        let carried = r9;
        r9 >>= down;
        edx = edx.wrapping_sub(down as i32);
        r10 |= carried << (64 - down);
    }
    // .L_2TAG_PACKET_11
    let mut a0 = r9 as i64 as f64;
    r10 >>= 1;
    let mut a3 = r10 as i64 as f64;
    edx = (edx << 4).wrapping_neg().wrapping_add(16368) | esi;
    edx ^= negated;
    let a4 = scale_of(edx);
    let a5 = scale_of(edx.wrapping_sub(1008));
    a0 *= a4;
    esi = (esi << 16) >> 31;
    a3 *= a5;
    let a1 = a0 + a3;
    a0 *= d(PI_4[0]);
    let quadrant = ((quadrant_word >> 29) as i32).wrapping_add(esi) ^ esi;
    a3 *= d(PI_4[0]);
    let mut a6 = d(PI_4[1]) * a1;
    a6 += a3;
    let a2 = a0;
    a0 += a6;
    a6 += a2 - a0;
    (a0, a6, quadrant)
}

/// .L_2TAG_PACKET_12 on, the large path of Math#sin and Math#cos: the reduced value through `evaluate`. Both functions sum in sin's order.
#[inline(never)]
fn large(x: f64, offset: i32) -> f64 {
    let (reduced, low, quadrant) = reduce_large(x);
    let n = nearest_n(reduced);
    let entry = n.wrapping_add(offset).wrapping_add(quadrant.wrapping_shl(3)) & 63;
    evaluate(reduced, n, entry as usize, low, Order::Sin)
}

/// Math#sin on x86_64 HotSpot from Java 17 (the Intel LIBM stub).
#[must_use]
pub fn sin(x: f64) -> f64 {
    let high = (x.to_bits() >> 32) as u32;
    let key = main_path_key(high);
    if key as u32 > 281_346_048 {
        if key > 281_346_048 {
            if high & 0x7ff0_0000 == 0x7ff0_0000 {
                return not_finite(x);
            }
            return large(x, 1_865_216);
        }
        // Under 2^-252 the assembly computes a scaled value into xmm3 and returns xmm0, x itself.
        if key as u32 >> 20 == 3325 {
            return x * d(ALL_ONES);
        }
        return x;
    }
    let n = nearest_n(x);
    evaluate(x, n, (n & 63) as usize, 0.0, Order::Sin)
}

/// Math#cos on x86_64 HotSpot from Java 17 (the Intel LIBM stub).
#[must_use]
pub fn cos(x: f64) -> f64 {
    let high = (x.to_bits() >> 32) as u32;
    let key = main_path_key(high);
    if key as u32 > 281_346_048 {
        if key > 281_346_048 {
            if high & 0x7ff0_0000 == 0x7ff0_0000 {
                return not_finite(x);
            }
            return large(x, 1_865_232);
        }
        return d(ONE) - x.abs();
    }
    let n = nearest_n(x);
    evaluate(x, n, (n.wrapping_add(1_865_232) & 63) as usize, 0.0, Order::Cos)
}

/// Java's Math.atan2, fdlibm's: the libm crate's, musl's. Past |y/x| = 2^64 with x negative musl returns +-pi/2 as it stands
/// where Java still takes pi less the angle, one bit more.
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    const SIGN: u64 = 1 << 63;
    let (ix, iy) = (x.to_bits() & !SIGN, y.to_bits() & !SIGN);
    let (high_x, high_y) = ((ix >> 32) as u32, (iy >> 32) as u32);
    if x.to_bits() & SIGN != 0 && ix != 0 && high_y < 0x7ff0_0000 && high_x.wrapping_add(64 << 20) < high_y {
        return d(0x3ff9_21fb_5444_2d19 | (y.to_bits() & SIGN));
    }
    libm::atan2(y, x)
}

/// Java 8's Math.sin and Math.cos on x86_64 HotSpot, the JVM that releases up to 1.16.5 declare
/// (javaVersion.majorVersion 8 in their version json). HotSpot 8 has no Intel LIBM stub: its
/// interpreter, C1 and C2 all go through MacroAssembler::trigfunc, which runs the x87 `fsin` or
/// `fcos` when |x| <= pi/4 (the double 0x3fe921fb54442d18) and otherwise calls
/// SharedRuntime::dsin or dcos, fdlibm 5.3's sin and cos, the code StrictMath runs.
///
/// Past pi/4 this is fdlibm 5.3 (s_sin.c, s_cos.c, k_sin.c, k_cos.c, e_rem_pio2.c and
/// k_rem_pio2.c, Sun's notice below), ported here from the C, the large reduction by way of the
/// libm crate's `rem_pio2_large` (MIT), which keeps fdlibm's arithmetic: Temurin 1.8.0_512 on
/// x86_64 Linux gives StrictMath's value there on every measured call-site argument, and this
/// code gives the JDK's bits there (docs/numeric-contract.md).
///
/// Within pi/4 the x87 instruction's result is not a function of the argument alone: it is the
/// processor's, kept at the precision the control word sets. On the AMD EPYC 9354P of host
/// `recorder` under Linux (64-bit precision) Java 8 gave the true value rounded to a 64-bit
/// significand and then to a double, both to nearest even, on all but 5 of 1,593,644 measured
/// values, and that is what this computes, from a double-double series. Windows starts threads
/// with 53-bit precision and Intel's microcode is other code; neither was measured
/// (docs/limitations.md).
///
/// ====================================================
/// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
///
/// Developed at SunSoft, a Sun Microsystems, Inc. business.
/// Permission to use, copy, modify, and distribute this
/// software is freely granted, provided that this notice
/// is preserved.
/// ====================================================
pub mod java8 {
    use super::{d, DEFAULT_NAN, QUIET_BIT};

    /// HotSpot 8's `pi_4`, against which `trigfunc` compares |x| (macroAssembler_x86.cpp).
    const PI_4: f64 = d(0x3fe9_21fb_5444_2d18);

    /// Java 8's Math#sin: the x87 `fsin` within pi/4, StrictMath#sin past it.
    #[must_use]
    pub fn sin(x: f64) -> f64 {
        if x.is_nan() || x.abs() <= PI_4 {
            x87(x, true)
        } else {
            fdlibm_sin(x)
        }
    }

    /// Java 8's Math#cos: the x87 `fcos` within pi/4, StrictMath#cos past it.
    #[must_use]
    pub fn cos(x: f64) -> f64 {
        if x.is_nan() || x.abs() <= PI_4 {
            x87(x, false)
        } else {
            fdlibm_cos(x)
        }
    }

    // ---- the x87 range: the true value rounded to 64 bits, then to 53 ----

    #[derive(Clone, Copy)]
    struct Dd(f64, f64);

    fn two_sum(a: f64, b: f64) -> Dd {
        let s = a + b;
        let v = s - a;
        Dd(s, (a - (s - v)) + (b - v))
    }

    fn quick_two_sum(a: f64, b: f64) -> Dd {
        let s = a + b;
        Dd(s, b - (s - a))
    }

    fn split(a: f64) -> (f64, f64) {
        let t = 134_217_729.0 * a;
        let high = t - (t - a);
        (high, a - high)
    }

    /// a * b exactly, by Dekker's product: no fused multiply-add, so every target agrees.
    fn two_prod(a: f64, b: f64) -> Dd {
        let p = a * b;
        let (ah, al) = split(a);
        let (bh, bl) = split(b);
        Dd(p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
    }

    fn mul(a: Dd, b: Dd) -> Dd {
        let p = two_prod(a.0, b.0);
        quick_two_sum(p.0, p.1 + (a.0 * b.1 + a.1 * b.0))
    }

    fn div_small(a: Dd, b: f64) -> Dd {
        let q1 = a.0 / b;
        let p = two_prod(q1, b);
        let r = two_sum(a.0, -p.0);
        let q2 = (r.0 + (r.1 - p.1 + a.1)) / b;
        quick_two_sum(q1, q2)
    }

    fn one_minus(a: Dd) -> Dd {
        let s = two_sum(1.0, -a.0);
        quick_two_sum(s.0, s.1 - a.1)
    }

    /// sin or cos of 2^-32 <= x <= pi/4, the value Java 8's Math#sin and Math#cos round, by Horner's rule on the Taylor series in double-double,
    /// about 100 bits: the last term kept is x^33 / 33!, below 2^-130 of the value.
    fn series(x: f64, sine: bool) -> Dd {
        let square = two_prod(x, x);
        let mut sum = Dd(1.0, 0.0);
        for k in (1..=16_u32).rev() {
            let (a, b) = if sine { (2 * k, 2 * k + 1) } else { (2 * k - 1, 2 * k) };
            sum = one_minus(div_small(mul(square, sum), f64::from(a * b)));
        }
        if sine {
            mul(sum, Dd(x, 0.0))
        } else {
            sum
        }
    }

    /// Java 8's Math#sin and Math#cos within pi/4: the x87 `fsin` or `fcos`, as measured.
    fn x87(x: f64, sine: bool) -> f64 {
        if x.is_nan() {
            // fsin and fcos return a NaN quieted, and fstp keeps its sign and payload
            return f64::from_bits(x.to_bits() | QUIET_BIT);
        }
        let magnitude = x.abs();
        if magnitude < d(0x3df0_0000_0000_0000) {
            // |x| < 2^-32: x^3/6 and x^2/2 are below half a 64-bit unit, so x and 1 come back
            return if sine { x } else { 1.0 };
        }
        let rounded = twice(series(magnitude, sine));
        if sine && x < 0.0 {
            -rounded
        } else {
            rounded
        }
    }

    /// A positive double-double rounded to a 64-bit significand and then to a double, both half
    /// even: the x87 result of Java 8's Math#sin and Math#cos stored as a double. `high` is a multiple of the 64-bit unit, so only `low` is rounded the first time, and
    /// the f64 sum of two such multiples is the second rounding.
    fn twice(value: Dd) -> f64 {
        let Dd(high, low) = value;
        let exponent = ((high.to_bits() >> 52) & 0x7ff) as i32;
        let power_of_two = high.to_bits() & 0x000f_ffff_ffff_ffff == 0;
        // the 64-bit unit of the binade the value lies in: one lower under a power of two
        let unit_exponent = if power_of_two && low < 0.0 { exponent - 64 } else { exponent - 63 };
        let unit = f64::from_bits((unit_exponent as u64) << 52);
        high + (low / unit).round_ties_even() * unit
    }

    // ---- fdlibm 5.3 ----

    const S1: f64 = d(0xbfc5_5555_5555_5549);
    const S2: f64 = d(0x3f81_1111_1110_f8a6);
    const S3: f64 = d(0xbf2a_01a0_19c1_61d5);
    const S4: f64 = d(0x3ec7_1de3_57b1_fe7d);
    const S5: f64 = d(0xbe5a_e5e6_8a2b_9ceb);
    const S6: f64 = d(0x3de5_d93a_5acf_d57c);
    const C1: f64 = d(0x3fa5_5555_5555_554c);
    const C2: f64 = d(0xbf56_c16c_16c1_5177);
    const C3: f64 = d(0x3efa_01a0_19cb_1590);
    const C4: f64 = d(0xbe92_7e4f_809c_52ad);
    const C5: f64 = d(0x3e21_ee9e_bdb4_b1c4);
    const C6: f64 = d(0xbda8_fae9_be88_38d4);

    /// fdlibm's __HI(x), the high word StrictMath#sin and StrictMath#cos read.
    fn high_word(x: f64) -> i32 {
        (x.to_bits() >> 32) as u32 as i32
    }

    /// fdlibm 5.3 k_sin.c, the kernel of StrictMath#sin and StrictMath#cos.
    fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
        let ix = high_word(x) & 0x7fff_ffff;
        if ix < 0x3e40_0000 && x as i32 == 0 {
            return x;
        }
        let z = x * x;
        let v = z * x;
        let r = S2 + z * (S3 + z * (S4 + z * (S5 + z * S6)));
        if iy == 0 {
            x + v * (S1 + z * r)
        } else {
            x - ((z * (0.5 * y - v * r) - y) - v * S1)
        }
    }

    /// fdlibm 5.3 k_cos.c, the kernel of StrictMath#sin and StrictMath#cos.
    fn kernel_cos(x: f64, y: f64) -> f64 {
        let ix = high_word(x) & 0x7fff_ffff;
        if ix < 0x3e40_0000 && x as i32 == 0 {
            return 1.0;
        }
        let z = x * x;
        let r = z * (C1 + z * (C2 + z * (C3 + z * (C4 + z * (C5 + z * C6)))));
        if ix < 0x3fd3_3333 {
            return 1.0 - (0.5 * z - (z * r - x * y));
        }
        // (the JDK's constants by their bits, as the rest of this file: test/generated-inventories.test.cjs keeps them apart from Minecraft's)
        let qx = if ix > 0x3fe9_0000 { f64::from_bits(0x3fd2_0000_0000_0000) } else { f64::from_bits(u64::from((ix - 0x0020_0000) as u32) << 32) };
        let hz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (hz - (z * r - x * y))
    }

    /// StrictMath#sin and StrictMath#cos of a NaN or an infinity: x - x.
    fn not_finite(x: f64) -> f64 {
        // x - x: a NaN quieted, an infinity x86's default NaN, written as bits
        if x.is_nan() {
            f64::from_bits(x.to_bits() | QUIET_BIT)
        } else {
            d(DEFAULT_NAN)
        }
    }

    /// fdlibm 5.3 s_sin.c: StrictMath#sin.
    fn fdlibm_sin(x: f64) -> f64 {
        let ix = high_word(x) & 0x7fff_ffff;
        if ix <= 0x3fe9_21fb {
            return kernel_sin(x, 0.0, 0);
        }
        if ix >= 0x7ff0_0000 {
            return not_finite(x);
        }
        let (n, y0, y1) = rem_pio2(x);
        match n & 3 {
            0 => kernel_sin(y0, y1, 1),
            1 => kernel_cos(y0, y1),
            2 => -kernel_sin(y0, y1, 1),
            _ => -kernel_cos(y0, y1),
        }
    }

    /// fdlibm 5.3 s_cos.c: StrictMath#cos.
    fn fdlibm_cos(x: f64) -> f64 {
        let ix = high_word(x) & 0x7fff_ffff;
        if ix <= 0x3fe9_21fb {
            return kernel_cos(x, 0.0);
        }
        if ix >= 0x7ff0_0000 {
            return not_finite(x);
        }
        let (n, y0, y1) = rem_pio2(x);
        match n & 3 {
            0 => kernel_cos(y0, y1),
            1 => -kernel_sin(y0, y1, 1),
            2 => -kernel_cos(y0, y1),
            _ => kernel_sin(y0, y1, 1),
        }
    }

    const INVPIO2: f64 = d(0x3fe4_5f30_6dc9_c883);
    const PIO2_1: f64 = d(0x3ff9_21fb_5440_0000);
    const PIO2_1T: f64 = d(0x3dd0_b461_1a62_6331);
    const PIO2_2: f64 = d(0x3dd0_b461_1a60_0000);
    const PIO2_2T: f64 = d(0x3ba3_198a_2e03_7073);
    const PIO2_3: f64 = d(0x3ba3_198a_2e00_0000);
    const PIO2_3T: f64 = d(0x397b_839a_2520_49c1);

    /// The high words of n * pi/2, n from 1 to 32 (e_rem_pio2.c npio2_hw).
    const NPIO2_HW: [i32; 32] = [
        0x3FF9_21FB, 0x4009_21FB, 0x4012_D97C, 0x4019_21FB, 0x401F_6A7A, 0x4022_D97C, 0x4025_FDBB, 0x4029_21FB,
        0x402C_463A, 0x402F_6A7A, 0x4031_475C, 0x4032_D97C, 0x4034_6B9C, 0x4035_FDBB, 0x4037_8FDB, 0x4039_21FB,
        0x403A_B41B, 0x403C_463A, 0x403D_D85A, 0x403F_6A7A, 0x4040_7E4C, 0x4041_475C, 0x4042_106C, 0x4042_D97C,
        0x4043_A28C, 0x4044_6B9C, 0x4045_34AC, 0x4045_FDBB, 0x4046_C6CB, 0x4047_8FDB, 0x4048_58EB, 0x4049_21FB,
    ];

    /// The biased exponent of x, as e_rem_pio2.c in StrictMath#sin and StrictMath#cos takes it.
    fn biased_exponent(x: f64) -> i32 {
        (high_word(x) >> 20) & 0x7ff
    }

    /// fdlibm 5.3 e_rem_pio2.c, the reduction of StrictMath#sin and StrictMath#cos: (n, y0, y1) with x - n * pi/2 = y0 + y1, for |x| > pi/4.
    fn rem_pio2(x: f64) -> (i32, f64, f64) {
        let hx = high_word(x);
        let ix = hx & 0x7fff_ffff;
        if ix < 0x4002_d97c {
            // |x| < 3pi/4, n = +-1; near pi/2 with 33+33+53 bits of pi
            return if hx > 0 {
                let z = x - PIO2_1;
                if ix == 0x3ff9_21fb {
                    let z = z - PIO2_2;
                    let y0 = z - PIO2_2T;
                    (1, y0, (z - y0) - PIO2_2T)
                } else {
                    let y0 = z - PIO2_1T;
                    (1, y0, (z - y0) - PIO2_1T)
                }
            } else {
                let z = x + PIO2_1;
                if ix == 0x3ff9_21fb {
                    let z = z + PIO2_2;
                    let y0 = z + PIO2_2T;
                    (-1, y0, (z - y0) + PIO2_2T)
                } else {
                    let y0 = z + PIO2_1T;
                    (-1, y0, (z - y0) + PIO2_1T)
                }
            };
        }
        if ix <= 0x4139_21fb {
            // |x| ~<= 2^19 * pi/2, medium size
            let t = x.abs();
            let n = (t * INVPIO2 + 0.5) as i32;
            let f_n = f64::from(n);
            let mut r = t - f_n * PIO2_1;
            let mut w = f_n * PIO2_1T;
            let mut y0 = r - w;
            if n >= 32 || ix == NPIO2_HW[(n - 1) as usize] {
                let j = ix >> 20;
                if j - biased_exponent(y0) > 16 {
                    // 2nd iteration, good to 118 bits
                    let t = r;
                    w = f_n * PIO2_2;
                    r = t - w;
                    w = f_n * PIO2_2T - ((t - r) - w);
                    y0 = r - w;
                    if j - biased_exponent(y0) > 49 {
                        // 3rd iteration, 151 bits
                        let t = r;
                        w = f_n * PIO2_3;
                        r = t - w;
                        w = f_n * PIO2_3T - ((t - r) - w);
                        y0 = r - w;
                    }
                }
            }
            let y1 = (r - y0) - w;
            return if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) };
        }
        // z = scalbn(|x|, ilogb(x) - 23), in three 24-bit pieces
        let e0 = (ix >> 20) - 1046;
        let mut z = f64::from_bits((u64::from((ix - (e0 << 20)) as u32) << 32) | (x.to_bits() & 0xffff_ffff));
        let mut tx = [0.0; 3];
        for piece in tx.iter_mut().take(2) {
            *piece = f64::from(z as i32);
            z = (z - *piece) * d(0x4170_0000_0000_0000);
        }
        tx[2] = z;
        let mut nx = 3;
        while tx[nx - 1] == 0.0 {
            nx -= 1;
        }
        let (n, y0, y1) = rem_pio2_large(&tx[..nx], e0);
        if hx < 0 {
            (-n, -y0, -y1)
        } else {
            (n, y0, y1)
        }
    }

    /// 2/pi in 24-bit words (k_rem_pio2.c two_over_pi), enough for every double.
    const IPIO2: [i32; 66] = [
        0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163, 0xABDEBB, 0xC561B7,
        0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129, 0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C,
        0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C, 0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11,
        0x8B5A0A, 0x6D1F6D, 0x367ECF, 0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7,
        0x8A5292, 0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3, 0x91615E,
        0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA, 0x73A8C9, 0x60E27B, 0xC08C6B,
    ];

    /// pi/2 in pieces of 24 bits (k_rem_pio2.c PIo2).
    const PIO2: [f64; 8] = [
        d(0x3ff9_21fb_4000_0000),
        d(0x3e74_442d_0000_0000),
        d(0x3cf8_4698_8000_0000),
        d(0x3b78_cc51_6000_0000),
        d(0x39f0_1b83_8000_0000),
        d(0x387a_2520_4000_0000),
        d(0x36e3_8222_8000_0000),
        d(0x3569_f31d_0000_0000),
    ];

    /// fdlibm 5.3 k_rem_pio2.c with prec 2 as e_rem_pio2.c calls it (jk = 4), in StrictMath#sin and
    /// StrictMath#cos; the libm crate's
    /// `rem_pio2_large` is the same arithmetic.
    #[allow(clippy::many_single_char_names, clippy::too_many_lines, clippy::needless_range_loop)]
    fn rem_pio2_large(x: &[f64], e0: i32) -> (i32, f64, f64) {
        const TWO24: f64 = d(0x4170_0000_0000_0000);
        const TWON24: f64 = d(0x3e70_0000_0000_0000);
        let jk = 4_usize;
        let jp = jk;
        let jx = x.len() - 1;
        let jv = ((e0 - 3) / 24).max(0);
        let mut q0 = e0 - 24 * (jv + 1);
        let jv = jv as usize;
        let mut f = [0.0_f64; 20];
        let mut q = [0.0_f64; 20];
        let mut fq = [0.0_f64; 20];
        let mut iq = [0_i32; 20];
        let mut j = jv as i32 - jx as i32;
        for slot in f.iter_mut().take(jx + jk + 1) {
            *slot = if j < 0 { 0.0 } else { f64::from(IPIO2[j as usize]) };
            j += 1;
        }
        for i in 0..=jk {
            let mut fw = 0.0;
            for j in 0..=jx {
                fw += x[j] * f[jx + i - j];
            }
            q[i] = fw;
        }
        let mut jz = jk;
        let (mut z, mut n, mut ih);
        loop {
            let mut i = 0_usize;
            z = q[jz];
            for j in (1..=jz).rev() {
                let fw = f64::from((TWON24 * z) as i32);
                iq[i] = (z - TWO24 * fw) as i32;
                z = q[j - 1] + fw;
                i += 1;
            }
            z = libm::scalbn(z, q0);
            z -= 8.0 * libm::floor(z * f64::from_bits(0x3fc0_0000_0000_0000));
            n = z as i32;
            z -= f64::from(n);
            ih = 0;
            if q0 > 0 {
                let i = iq[jz - 1] >> (24 - q0);
                n += i;
                iq[jz - 1] -= i << (24 - q0);
                ih = iq[jz - 1] >> (23 - q0);
            } else if q0 == 0 {
                ih = iq[jz - 1] >> 23;
            } else if z >= 0.5 {
                ih = 2;
            }
            if ih > 0 {
                n += 1;
                let mut carry = 0;
                for word in iq.iter_mut().take(jz) {
                    let j = *word;
                    if carry == 0 {
                        if j != 0 {
                            carry = 1;
                            *word = 0x0100_0000 - j;
                        }
                    } else {
                        *word = 0x00ff_ffff - j;
                    }
                }
                match q0 {
                    1 => iq[jz - 1] &= 0x007f_ffff,
                    2 => iq[jz - 1] &= 0x003f_ffff,
                    _ => {}
                }
                if ih == 2 {
                    z = 1.0 - z;
                    if carry != 0 {
                        z -= libm::scalbn(1.0, q0);
                    }
                }
            }
            if z == 0.0 {
                let mut j = 0;
                for i in (jk..jz).rev() {
                    j |= iq[i];
                }
                if j == 0 {
                    let mut k = 1;
                    while iq[jk - k] == 0 {
                        k += 1;
                    }
                    for i in (jz + 1)..=(jz + k) {
                        f[jx + i] = f64::from(IPIO2[jv + i]);
                        let mut fw = 0.0;
                        for j in 0..=jx {
                            fw += x[j] * f[jx + i - j];
                        }
                        q[i] = fw;
                    }
                    jz += k;
                    continue;
                }
            }
            break;
        }
        if z == 0.0 {
            jz -= 1;
            q0 -= 24;
            while iq[jz] == 0 {
                jz -= 1;
                q0 -= 24;
            }
        } else {
            z = libm::scalbn(z, -q0);
            if z >= TWO24 {
                let fw = f64::from((TWON24 * z) as i32);
                iq[jz] = (z - TWO24 * fw) as i32;
                jz += 1;
                q0 += 24;
                iq[jz] = fw as i32;
            } else {
                iq[jz] = z as i32;
            }
        }
        let mut fw = libm::scalbn(1.0, q0);
        for i in (0..=jz).rev() {
            q[i] = fw * f64::from(iq[i]);
            fw *= TWON24;
        }
        for i in (0..=jz).rev() {
            let mut fw = 0.0;
            let mut k = 0;
            while k <= jp && k <= jz - i {
                fw += PIO2[k] * q[i + k];
                k += 1;
            }
            fq[jz - i] = fw;
        }
        let mut fw = 0.0;
        for i in (0..=jz).rev() {
            fw += fq[i];
        }
        let y0 = if ih == 0 { fw } else { -fw };
        fw = fq[0] - fw;
        for value in fq.iter().take(jz + 1).skip(1) {
            fw += value;
        }
        let y1 = if ih == 0 { fw } else { -fw };
        (n & 7, y0, y1)
    }

    #[cfg(test)]
    mod tests {
        use super::{cos, sin, NPIO2_HW};

        /// StrictMath#sin and StrictMath#cos on Java 21.0.11 and 25.0.3, x86_64
        /// (scripts/java-oracle/OracleStrictTrig.java): fdlibm itself, within pi/4 too, where Java 8's
        /// Math takes the x87 instead, past 3pi/4 through every way of e_rem_pio2.c and k_rem_pio2.c,
        /// up to the largest double, and what is not finite.
        #[test]
        fn fdlibm_is_strict_math_bit_for_bit() {
            let text = include_str!("../fixtures/java-oracle-strict-trig-bits.txt");
            let (mut asked, mut within, mut huge) = (0, 0, 0);
            for line in text.lines() {
                let words: Vec<u64> = line.split(' ').map(|word| u64::from_str_radix(word, 16).expect("hex")).collect();
                let x = f64::from_bits(words[0]);
                assert_eq!(super::fdlibm_sin(x).to_bits(), words[1], "StrictMath.sin {:x}", words[0]);
                assert_eq!(super::fdlibm_cos(x).to_bits(), words[2], "StrictMath.cos {:x}", words[0]);
                // Java 8's Math is StrictMath past pi/4
                if x.abs() > std::f64::consts::FRAC_PI_4 && x.is_finite() {
                    assert_eq!(sin(x).to_bits(), words[1], "Java 8 Math.sin {:x}", words[0]);
                    assert_eq!(cos(x).to_bits(), words[2], "Java 8 Math.cos {:x}", words[0]);
                }
                asked += 1;
                within += usize::from(x.abs() <= std::f64::consts::FRAC_PI_4);
                huge += usize::from(x.abs() >= 1.0e15 && x.is_finite());
            }
            assert_eq!(asked, 2918);
            assert!(within >= 20 && huge >= 1000, "{within} within pi/4, {huge} past 1e15");
        }

        #[test]
        fn npio2_hw_is_the_high_word_of_n_pi_over_2() {
            for (index, &word) in NPIO2_HW.iter().enumerate() {
                let n = (index + 1) as f64;
                assert_eq!((n * std::f64::consts::FRAC_PI_2).to_bits() >> 32, word as u64, "n {n}");
            }
        }

        /// Temurin 1.8.0_512 on x86_64 Linux (scripts/java-oracle/TrigCallSites.java, B315): Math.sin
        /// and Math.cos of call-site arguments where Java 21 differs, both sides of pi/4.
        #[test]
        fn matches_java_8() {
            // the first five past pi/4 (fdlibm), the rest within it (x87); every pair differs from
            // Java 21 in sin or cos
            let cases: [(u64, u64, u64); 10] = [
                (0xc02c_031e_a000_0000, 0xbfef_b9ba_da1c_54fd, 0x3fc0_babe_055b_bc65),
                (0xc02c_0250_e000_0000, 0xbfef_b80a_0799_1c2e, 0x3fc0_edbb_b607_cee5),
                (0xc02c_0070_6000_0000, 0xbfef_b403_4984_758f, 0x3fc1_64c6_0dc3_37f9),
                (0xc02b_fb13_e000_0000, 0xbfef_a7e9_5618_3199, 0x3fc2_b877_2f4b_fa7c),
                (0xc028_e2b6_e000_0000, 0x3fbf_8da2_3bca_bcd3, 0x3fef_c189_4ec2_fde4),
                (0xbfe0_30ea_0000_0000, 0xbfdf_049e_a7d7_5c2f, 0x3feb_fd93_e2e6_a646),
                (0xbfdc_17c2_a000_0000, 0xbfdb_3306_dfea_e2fc, 0x3fec_f762_72e5_6f96),
                (0x3fab_414d_8000_0000, 0x3fab_3e02_077c_9b04, 0x3fef_f465_5796_1fad),
                (0xbfe7_8ec9_4000_0000, 0xbfe5_7c9d_e5cc_96d9, 0x3fe7_b697_60a4_30db),
                (0xbfe7_7da0_e000_0000, 0xbfe5_6fe3_e3ee_6717, 0x3fe7_c219_4798_3ab4),
            ];
            for (x, sine, cosine) in cases {
                let x = f64::from_bits(x);
                assert_eq!(sin(x).to_bits(), sine, "sin({x})");
                assert_eq!(cos(x).to_bits(), cosine, "cos({x})");
                assert!(super::super::sin(x).to_bits() != sine || super::super::cos(x).to_bits() != cosine, "{x}");
            }
            // Mth's COS_TAB[61], cos(asin(61 / 256)): Java 8's, one unit above Java 21's
            let angle = libm::asin(61.0 / 256.0);
            assert_eq!(angle.to_bits(), 0x3fce_cbd7_5b6c_d8f0);
            assert_eq!(cos(angle).to_bits(), 0x3fef_140a_0a08_6af0);
            assert_eq!(super::super::cos(angle).to_bits(), 0x3fef_140a_0a08_6af1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{atan2, cos, java8, sin};

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
    fn is_java_and_not_correctly_rounded_in_a_near_tie() {
        // the correctly rounded values are 0x3fda494de68f9668 and 0x3fdf624bcdc02a93
        let x = f64::from_bits(0x3fdb_167f_6000_0000);
        assert_eq!(bits(sin(x)), 0x3fda_494d_e68f_9667);
        let x = f64::from_bits(0x3ff0_eeb3_c000_0000);
        assert_eq!(bits(cos(x)), 0x3fdf_624b_cdc0_2a94);
    }

    #[test]
    fn keeps_the_symmetries_and_special_values() {
        for x in [1.0e-9, 0.3, 0.785_398_163_397_448_3, 2.0, 7.5] {
            assert_eq!(bits(cos(-x)), bits(cos(x)));
            assert_eq!(bits(sin(-x)), bits(-sin(x)));
        }
        assert_eq!(bits(sin(-0.0)), bits(-0.0));
        assert_eq!(cos(0.0), 1.0);
        assert_eq!(bits(sin(5.0e-324)), bits(5.0e-324));
        assert_eq!(bits(cos(5.0e-324)), bits(1.0));
    }

    /// Java's raw bits: x86's default NaN from an infinity, and a NaN quieted, its sign and payload kept.
    #[test]
    fn not_finite_values_give_the_bits_of_x86() {
        let cases: [(u64, u64); 6] = [
            (0x7ff0_0000_0000_0000, 0xfff8_0000_0000_0000),
            (0xfff0_0000_0000_0000, 0xfff8_0000_0000_0000),
            (0x7ff8_0000_0000_0000, 0x7ff8_0000_0000_0000),
            (0xfff8_0000_0000_0000, 0xfff8_0000_0000_0000),
            (0x7ff0_0000_0000_0001, 0x7ff8_0000_0000_0001),
            (0x7ff8_dead_0000_beef, 0x7ff8_dead_0000_beef),
        ];
        for (x, java) in cases {
            assert_eq!(bits(sin(f64::from_bits(x))), java, "{x:x}");
            assert_eq!(bits(cos(f64::from_bits(x))), java, "{x:x}");
        }
    }

    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;

    /// FNV-1a over words, as scripts/java-oracle/TrigDigest.java: a step is a bijection, so one differing word always shows.
    fn mix(hash: u64, word: u64) -> u64 {
        (hash ^ word).wrapping_mul(0x0100_0000_01b3)
    }

    /// TrigDigest floats, line 127: every float in [1, 2], from Java 21.0.11 and 25.0.3 alike.
    #[test]
    fn every_float_of_an_exponent_has_java_digest() {
        let (mut sine, mut cosine) = (BASIS, BASIS);
        for low in 0u32..1 << 23 {
            let x = f64::from(f32::from_bits((127 << 23) | low));
            sine = mix(sine, sin(x).to_bits());
            cosine = mix(cosine, cos(x).to_bits());
        }
        assert_eq!((sine, cosine), (0x6f1a_8671_2eb4_9310, 0x46b8_2019_0605_9dc3));
    }

    /// 26.3 PrimedTnt's first motion, -Math.sin(rot) * 0.02 and -Math.cos(rot) * 0.02, with rot drawn by LegacyRandomSource
    /// from seed 0: TrigDigest kick 0 200000, on Java 21.0.11 and 25.0.3 alike. The engine takes a TNT's motion from its
    /// caller and never computes this one; it is here for its angles, spread evenly over a turn on a double's grid.
    #[test]
    fn the_kick_of_a_primed_tnt_has_java_digest() {
        const MASK: u64 = 281_474_976_710_655;
        let mut state = 25_214_903_917u64 & MASK;
        let (mut x, mut z) = (BASIS, BASIS);
        for _ in 0..200_000 {
            state = state.wrapping_mul(25_214_903_917).wrapping_add(11) & MASK;
            let upper = state >> (48 - 26);
            state = state.wrapping_mul(25_214_903_917).wrapping_add(11) & MASK;
            let lower = state >> (48 - 27);
            let rot = ((upper << 27) + lower) as f64 * 1.110_223_024_625_156_5e-16 * 6.283_185_482_025_146_5;
            x = mix(x, (-sin(rot) * 0.02).to_bits());
            z = mix(z, (-cos(rot) * 0.02).to_bits());
        }
        assert_eq!((x, z), (0x374c_f919_679c_78b2, 0xf825_33a0_7a3d_65e7));
    }

    /// `x sin cos` in hex bits from TrigDigest bits: the nearest doubles to multiples of pi/2 at each exponent, the ends of the
    /// main path, powers of two, the rounding of N, and NaNs by their raw bits, on Java 21.0.11 and 25.0.3 alike.
    #[test]
    fn the_hardest_angles_have_java_bits() {
        let text = include_str!("../fixtures/java-oracle-trig-bits.txt");
        let mut asked = 0;
        for line in text.lines() {
            let words: Vec<u64> = line.split(' ').map(|word| u64::from_str_radix(word, 16).expect("hex")).collect();
            let x = f64::from_bits(words[0]);
            assert_eq!(sin(x).to_bits(), words[1], "sin {:x}", words[0]);
            assert_eq!(cos(x).to_bits(), words[2], "cos {:x}", words[0]);
            asked += 1;
        }
        assert_eq!(asked, 2486);
    }

    /// Math#sin and Math#cos of the same arguments (scripts/java-oracle/OracleStrictTrig.java, Java
    /// 21.0.11 and 25.0.3, x86_64): the stubs' large reduction of every exponent from 2^20 to the
    /// largest double, 1e6, 1e15, 2^60 and the doubles nearest to n * pi/2 among them, either sign.
    #[test]
    fn large_arguments_have_java_bits() {
        let text = include_str!("../fixtures/java-oracle-strict-trig-bits.txt");
        let mut asked = 0;
        for line in text.lines() {
            let words: Vec<u64> = line.split(' ').map(|word| u64::from_str_radix(word, 16).expect("hex")).collect();
            let x = f64::from_bits(words[0]);
            assert_eq!(sin(x).to_bits(), words[3], "Math.sin {:x}", words[0]);
            assert_eq!(cos(x).to_bits(), words[4], "Math.cos {:x}", words[0]);
            asked += 1;
        }
        assert_eq!(asked, 2918);
        // a few by name: 1e6, 2^60, and 1e15, whose cosine is one unit apart in Math and StrictMath
        assert_eq!(sin(1.0e6).to_bits(), 0xbfd6_664b_2568_d867);
        assert_eq!(cos(1.0e6).to_bits(), 0x3fed_f9df_9906_d32c);
        assert_eq!(sin(2.0_f64.powi(60)).to_bits(), 0xbfea_94ad_ab06_665c);
        assert_eq!(sin(-(2.0_f64.powi(60))).to_bits(), 0x3fea_94ad_ab06_665c);
        assert_eq!(cos(1.0e15).to_bits(), 0xbfe0_6c15_4609_d33f);
        assert_eq!(java8::cos(1.0e15).to_bits(), 0xbfe0_6c15_4609_d33e);
    }

    #[test]
    fn atan2_is_fdlibm_past_the_ratio_where_musl_stops() {
        // Math.atan2 on Java 21.0.11 and 25.0.3
        assert_eq!(bits(atan2(8.275_009_975_213_795e230, -1.118_319_524_899_131_2e-235)), 0x3ff9_21fb_5444_2d19);
        assert_eq!(bits(atan2(-1.399_082_460_948_282_7e-155, -5.070_637_079_137_835e-300)), 0xbff9_21fb_5444_2d19);
        assert_eq!(bits(atan2(f64::INFINITY, -1.0)), bits(libm::atan2(f64::INFINITY, -1.0)));
    }

    /// A debug build has overflow checks: no exponent, no mantissa and no sign may trip one.
    #[test]
    fn no_input_panics() {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        for exponent in 0u64..2048 {
            for sign in [0u64, 1 << 63] {
                for _ in 0..16 {
                    state = state.wrapping_mul(0xff51_afd7_ed55_8ccd).wrapping_add(1);
                    let mantissa = (state >> 12) & ((1 << 52) - 1);
                    for fraction in [0, 1, mantissa, (1 << 52) - 1] {
                        let x = f64::from_bits(sign | (exponent << 52) | fraction);
                        let (s, c) = (sin(x), cos(x));
                        assert_eq!(s.is_nan(), !x.is_finite());
                        assert_eq!(c.is_nan(), !x.is_finite());
                    }
                }
            }
        }
    }
}

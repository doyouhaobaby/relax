// On x86_64 hosts, agree.sh compares these digests from rustc's core::arch
// with the digests Relax produces from its own intrinsics on the same CPU.
#![cfg(target_arch = "x86_64")]
#![allow(non_snake_case)]
include!("arch/common.rs");
use core::arch::x86_64 as real;

const CASES: usize = 17 + 512;
const _: () = assert!(CASES < 17 + RANDOM_CASES);

// Include the integer boundaries and their distinct f32 neighbors.
// At this precision i32::MAX and i32::MIN - 1 round to powers of two.
const EXTRA_FLOATS: &[f32] = &[
    0.5, -0.5, 1.5, 2.5, -2.5,
    2_147_483_647.0, 2_147_483_648.0, -2_147_483_649.0, 9.3e18,
    2_147_483_520.0, -2_147_483_904.0,
    0.0, -0.0, 0.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN, -1.0, 1.0,
];
const FLOAT_CASES: usize = CASES + EXTRA_FLOATS.len();

unsafe fn float_lanes<const LANES: usize>(rng: &mut Rng, case: usize, arg: usize) -> [f32; LANES] {
    if case < CASES {
        return sample(rng, case, arg);
    }
    let mut lanes = [0.0; LANES];
    for (i, lane) in lanes.iter_mut().enumerate() {
        *lane = EXTRA_FLOATS[(case - CASES + arg + i) % EXTRA_FLOATS.len()];
    }
    lanes
}

unsafe fn floats4(rng: &mut Rng, case: usize, arg: usize) -> real::__m128 {
    cast(float_lanes::<4>(rng, case, arg))
}

unsafe fn floats8(rng: &mut Rng, case: usize, arg: usize) -> real::__m256 {
    cast(float_lanes::<8>(rng, case, arg))
}

fn add_estimate(d: &mut Digest, x: f32, result: f32, rsqrt: bool) {
    let magnitude = x.to_bits() & 0x7fff_ffff;
    if magnitude == 0 || magnitude >= f32::INFINITY.to_bits() {
        let class = match result.to_bits() {
            0x0000_0000 => 0,
            0x8000_0000 => 1,
            0x7f80_0000 => 2,
            0xff80_0000 => 3,
            _ if result.is_nan() => 4,
            _ => 5,
        };
        d.add(&[class]);
        return;
    }

    let exponent = (magnitude >> 23) as i32 - 127;
    if !(-120..=120).contains(&exponent) {
        return;
    }
    if rsqrt && x < 0.0 {
        d.add(&[result.is_nan() as u8]);
        return;
    }
    let x = f64::from(x);
    let exact = if rsqrt { 1.0 / x.sqrt() } else { 1.0 / x };
    let accurate = (f64::from(result) - exact).abs() <= exact.abs() * (1.5 / 4096.0);
    d.add(&[accurate as u8]);
}

macro_rules! require_feature {
    ($name:ident, $feature:tt) => {
        if !std::is_x86_feature_detected!($feature) {
            eprintln!("{}: {} unavailable", stringify!($name), $feature);
            return;
        }
    };
}

// The test number selects a reproducible random stream for its operands.
macro_rules! cases {
    ($name:ident, $feature:tt, $seed:literal, $count:expr, $tag:expr, |$rng:ident, $case:ident, $d:ident| $body:block) => {{
        require_feature!($name, $feature);
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ $seed);
        let mut digest = Digest::new();
        for $case in 0..$count {
            let $rng = &mut rng;
            let $d = &mut digest;
            unsafe { $body }
        }
        digest.line(stringify!($name), $tag);
    }};
}

macro_rules! sampled {
    ($name:ident, $feature:tt, $seed:literal, $count:expr, $input:ident, [$($arg:literal),+]) => {
        cases!($name, $feature, $seed, $count, "none", |rng, case, d| {
            let result = real::$name($($input(rng, case, $arg)),+);
            d.add(bytes(&result));
        });
    };
}

// Each immediate gets its own call boundary so LLVM needn't optimize hundreds
// of specialized intrinsics inside a single sampling loop.
macro_rules! check_immediates {
    ([$first:literal $(, $imm:literal)*], $name:ident, $feature:tt, $seed:literal,
     $count:expr, $input:ident, [$($arg:ident: $ty:ty = $index:literal),+]) => {{
        #[inline(never)]
        unsafe fn compare<const N: i32>(($($arg,)+ d): ($($ty,)+ &mut Digest)) {
            d.add(bytes(&N));
            let result = real::$name::<N>($($arg),+);
            d.add(bytes(&result));
        }
        cases!($name, $feature, $seed, $count, concat!($first $(, ",", $imm)*), |rng, case, d| {
            $(let $arg = $input(rng, case, $index);)+
            immediate_cases!(compare, ($($arg,)+ d), [$first $(, $imm)*]);
        });
    }};
}

// stdarch checks these operands as unsigned integers of the indicated width.
// The same literal list drives the calls and labels their digest.
macro_rules! immediates {
    (1, $($test:tt)*) => { check_immediates!([0, 1], $($test)*); };
    (2, $($test:tt)*) => { check_immediates!([0, 1, 2, 3], $($test)*); };
    (3, $($test:tt)*) => { check_immediates!([0, 1, 2, 3, 4, 5, 6, 7], $($test)*); };
    (4, $($test:tt)*) => { check_immediates!([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15], $($test)*); };
    (5, $($test:tt)*) => {
        check_immediates!([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
            16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31
        ], $($test)*);
    };
    (8, $($test:tt)*) => {
        check_immediates!([
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
            16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
            32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
            48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
            64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
            80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95,
            96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111,
            112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127,
            128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143,
            144, 145, 146, 147, 148, 149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159,
            160, 161, 162, 163, 164, 165, 166, 167, 168, 169, 170, 171, 172, 173, 174, 175,
            176, 177, 178, 179, 180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191,
            192, 193, 194, 195, 196, 197, 198, 199, 200, 201, 202, 203, 204, 205, 206, 207,
            208, 209, 210, 211, 212, 213, 214, 215, 216, 217, 218, 219, 220, 221, 222, 223,
            224, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 235, 236, 237, 238, 239,
            240, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 251, 252, 253, 254, 255
        ], $($test)*);
    };
}

const SHIFT_COUNTS: &[u64] = &[
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 31, 32, 63, 64, 65,
];

macro_rules! shifts {
    ($name:ident, $seed:literal) => {
        cases!($name, "sse2", $seed, CASES, "none", |rng, case, d| {
            let a = sample(rng, case, 0);
            let mut count: [u64; 2] = sample(rng, case, 1);
            d.add(bytes(&real::$name(a, cast(count))));
            // Only the low quadword selects the shift distance.
            count[0] = SHIFT_COUNTS[case % SHIFT_COUNTS.len()];
            d.add(bytes(&real::$name(a, cast(count))));
        });
    };
}

// Independent random bit masks almost always intersect without either being a
// subset. Also exercise strict subsets, disjoint masks and strict supersets.
macro_rules! bit_tests {
    ($name:ident, $feature:tt, $seed:literal, $lanes:literal) => {
        cases!($name, $feature, $seed, CASES, "none", |rng, case, d| {
            let a: [u32; $lanes] = sample(rng, case, 0);
            let mut mask: [u32; $lanes] = sample(rng, case, 1);
            d.add(bytes(&real::$name(cast(a), cast(mask))));
            for i in 0..$lanes {
                mask[i] = match case % 3 {
                    0 => mask[i] & a[i],
                    1 => mask[i] & !a[i],
                    _ => mask[i] | a[i],
                };
            }
            d.add(bytes(&real::$name(cast(a), cast(mask))));
        });
    };
}

// SSE

#[test]
fn b001__mm_cmpeq_ss() {
    sampled!(_mm_cmpeq_ss, "sse", 1, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b002__mm_cmpge_ss() {
    sampled!(_mm_cmpge_ss, "sse", 2, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b003__mm_cmpgt_ss() {
    sampled!(_mm_cmpgt_ss, "sse", 3, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b004__mm_cmple_ss() {
    sampled!(_mm_cmple_ss, "sse", 4, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b005__mm_cmplt_ss() {
    sampled!(_mm_cmplt_ss, "sse", 5, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b006__mm_cmpneq_ss() {
    sampled!(_mm_cmpneq_ss, "sse", 6, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b007__mm_cmpnge_ps() {
    sampled!(_mm_cmpnge_ps, "sse", 7, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b008__mm_cmpnge_ss() {
    sampled!(_mm_cmpnge_ss, "sse", 8, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b009__mm_cmpngt_ps() {
    sampled!(_mm_cmpngt_ps, "sse", 9, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b010__mm_cmpngt_ss() {
    sampled!(_mm_cmpngt_ss, "sse", 10, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b011__mm_cmpnle_ps() {
    sampled!(_mm_cmpnle_ps, "sse", 11, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b012__mm_cmpnle_ss() {
    sampled!(_mm_cmpnle_ss, "sse", 12, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b013__mm_cmpnlt_ps() {
    sampled!(_mm_cmpnlt_ps, "sse", 13, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b014__mm_cmpnlt_ss() {
    sampled!(_mm_cmpnlt_ss, "sse", 14, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b015__mm_cmpord_ss() {
    sampled!(_mm_cmpord_ss, "sse", 15, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b016__mm_cmpunord_ss() {
    sampled!(_mm_cmpunord_ss, "sse", 16, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b017__mm_comieq_ss() {
    sampled!(_mm_comieq_ss, "sse", 17, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b018__mm_comige_ss() {
    sampled!(_mm_comige_ss, "sse", 18, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b019__mm_comigt_ss() {
    sampled!(_mm_comigt_ss, "sse", 19, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b020__mm_comile_ss() {
    sampled!(_mm_comile_ss, "sse", 20, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b021__mm_comilt_ss() {
    sampled!(_mm_comilt_ss, "sse", 21, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b022__mm_comineq_ss() {
    sampled!(_mm_comineq_ss, "sse", 22, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b023__mm_cvt_si2ss() {
    sampled!(_mm_cvt_si2ss, "sse", 23, CASES, sample, [0, 1]);
}

#[test]
fn b024__mm_cvt_ss2si() {
    sampled!(_mm_cvt_ss2si, "sse", 24, FLOAT_CASES, floats4, [0]);
}

#[test]
fn b025__mm_cvtsi32_ss() {
    sampled!(_mm_cvtsi32_ss, "sse", 25, CASES, sample, [0, 1]);
}

#[test]
fn b026__mm_cvtsi64_ss() {
    sampled!(_mm_cvtsi64_ss, "sse", 26, CASES, sample, [0, 1]);
}

#[test]
fn b027__mm_cvtss_si32() {
    sampled!(_mm_cvtss_si32, "sse", 27, FLOAT_CASES, floats4, [0]);
}

#[test]
fn b028__mm_cvtss_si64() {
    sampled!(_mm_cvtss_si64, "sse", 28, FLOAT_CASES, floats4, [0]);
}

#[test]
fn b029__mm_cvtt_ss2si() {
    sampled!(_mm_cvtt_ss2si, "sse", 29, FLOAT_CASES, floats4, [0]);
}

#[test]
fn b030__mm_cvttss_si32() {
    sampled!(_mm_cvttss_si32, "sse", 30, FLOAT_CASES, floats4, [0]);
}

#[test]
fn b031__mm_cvttss_si64() {
    sampled!(_mm_cvttss_si64, "sse", 31, FLOAT_CASES, floats4, [0]);
}

#[test]
fn b032__mm_div_ss() {
    sampled!(_mm_div_ss, "sse", 32, CASES, sample, [0, 1]);
}

#[test]
fn b033__mm_load_ps1() {
    cases!(_mm_load_ps1, "sse", 33, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 4).cast();
        d.add(bytes(&real::_mm_load_ps1(ptr)));
    });
}

#[test]
fn b034__mm_load_ss() {
    cases!(_mm_load_ss, "sse", 34, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 4).cast();
        d.add(bytes(&real::_mm_load_ss(ptr)));
    });
}

#[test]
fn b035__mm_load1_ps() {
    cases!(_mm_load1_ps, "sse", 35, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 4).cast();
        d.add(bytes(&real::_mm_load1_ps(ptr)));
    });
}

#[test]
fn b036__mm_loadr_ps() {
    cases!(_mm_loadr_ps, "sse", 36, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 16).cast();
        d.add(bytes(&real::_mm_loadr_ps(ptr)));
    });
}

#[test]
fn b037__mm_max_ss() {
    sampled!(_mm_max_ss, "sse", 37, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b038__mm_min_ss() {
    sampled!(_mm_min_ss, "sse", 38, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b039__mm_move_ss() {
    sampled!(_mm_move_ss, "sse", 39, CASES, sample, [0, 1]);
}

#[test]
fn b040__mm_movelh_ps() {
    sampled!(_mm_movelh_ps, "sse", 40, CASES, sample, [0, 1]);
}

#[test]
fn b041__mm_mul_ss() {
    sampled!(_mm_mul_ss, "sse", 41, CASES, sample, [0, 1]);
}

#[test]
fn b042__mm_rcp_ss() {
    cases!(_mm_rcp_ss, "sse", 42, FLOAT_CASES, "none", |rng, case, d| {
        let input = float_lanes::<4>(rng, case, 0);
        let output: [f32; 4] = cast(real::_mm_rcp_ss(cast(input)));
        add_estimate(d, input[0], output[0], false);
        d.add(&bytes(&output)[4..]);
    });
}

#[test]
fn b043__mm_rsqrt_ss() {
    cases!(_mm_rsqrt_ss, "sse", 43, FLOAT_CASES, "none", |rng, case, d| {
        let input = float_lanes::<4>(rng, case, 0);
        let output: [f32; 4] = cast(real::_mm_rsqrt_ss(cast(input)));
        add_estimate(d, input[0], output[0], true);
        d.add(&bytes(&output)[4..]);
    });
}

#[test]
fn b044__mm_sfence() {
    require_feature!(_mm_sfence, "sse");
    unsafe { real::_mm_sfence() };
    let mut d = Digest::new();
    d.add(&[1]);
    d.line("_mm_sfence", "none");
}

#[test]
fn b045__mm_sqrt_ss() {
    sampled!(_mm_sqrt_ss, "sse", 45, CASES, sample, [0]);
}

#[test]
fn b046__mm_store_ps1() {
    cases!(_mm_store_ps1, "sse", 46, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_store_ps1(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b047__mm_store_ss() {
    cases!(_mm_store_ss, "sse", 47, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 4).cast();
        let a = sample(rng, case, 1);
        real::_mm_store_ss(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b048__mm_store1_ps() {
    cases!(_mm_store1_ps, "sse", 48, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_store1_ps(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b049__mm_storer_ps() {
    cases!(_mm_storer_ps, "sse", 49, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_storer_ps(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b050__mm_stream_ps() {
    cases!(_mm_stream_ps, "sse", 50, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_stream_ps(ptr, a);
        real::_mm_sfence();
        d.add(&dst.0);
    });
}

#[test]
fn b051__mm_sub_ss() {
    sampled!(_mm_sub_ss, "sse", 51, CASES, sample, [0, 1]);
}

#[test]
fn b052__mm_ucomieq_ss() {
    sampled!(_mm_ucomieq_ss, "sse", 52, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b053__mm_ucomige_ss() {
    sampled!(_mm_ucomige_ss, "sse", 53, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b054__mm_ucomigt_ss() {
    sampled!(_mm_ucomigt_ss, "sse", 54, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b055__mm_ucomile_ss() {
    sampled!(_mm_ucomile_ss, "sse", 55, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b056__mm_ucomilt_ss() {
    sampled!(_mm_ucomilt_ss, "sse", 56, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b057__mm_ucomineq_ss() {
    sampled!(_mm_ucomineq_ss, "sse", 57, FLOAT_CASES, floats4, [0, 1]);
}

// SSE2

#[test]
fn b058__mm_bslli_si128() {
    immediates!(8, _mm_bslli_si128, "sse2", 58, CASES, sample,
        [a: real::__m128i = 0]);
}

#[test]
fn b059__mm_bsrli_si128() {
    immediates!(8, _mm_bsrli_si128, "sse2", 59, CASES, sample,
        [a: real::__m128i = 0]);
}

#[test]
fn b060__mm_clflush() {
    require_feature!(_mm_clflush, "sse2");
    let data = Buffer([0; 192]);
    unsafe { real::_mm_clflush(data.0.as_ptr()) };
    let mut d = Digest::new();
    d.add(&[1]);
    d.line("_mm_clflush", "none");
}

#[test]
fn b061__mm_insert_epi16() {
    immediates!(3, _mm_insert_epi16, "sse2", 61, CASES, sample,
        [a: real::__m128i = 0, i: i32 = 1]);
}

#[test]
fn b062__mm_lfence() {
    require_feature!(_mm_lfence, "sse2");
    unsafe { real::_mm_lfence() };
    let mut d = Digest::new();
    d.add(&[1]);
    d.line("_mm_lfence", "none");
}

#[test]
fn b063__mm_loadu_si16() {
    cases!(_mm_loadu_si16, "sse2", 63, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add(case % 16).cast();
        d.add(bytes(&real::_mm_loadu_si16(ptr)));
    });
}

#[test]
fn b064__mm_loadu_si32() {
    cases!(_mm_loadu_si32, "sse2", 64, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add(case % 16).cast();
        d.add(bytes(&real::_mm_loadu_si32(ptr)));
    });
}

#[test]
fn b065__mm_loadu_si64() {
    cases!(_mm_loadu_si64, "sse2", 65, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add(case % 16).cast();
        d.add(bytes(&real::_mm_loadu_si64(ptr)));
    });
}

#[test]
fn b066__mm_maskmoveu_si128() {
    cases!(_mm_maskmoveu_si128, "sse2", 66, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add(case % 16).cast();
        let a = sample(rng, case, 0);
        let mask = sample(rng, case, 1);
        real::_mm_maskmoveu_si128(a, mask, ptr);
        real::_mm_sfence();
        d.add(&dst.0);
    });
}

#[test]
fn b067__mm_mfence() {
    require_feature!(_mm_mfence, "sse2");
    unsafe { real::_mm_mfence() };
    let mut d = Digest::new();
    d.add(&[1]);
    d.line("_mm_mfence", "none");
}

#[test]
fn b068__mm_mulhi_epi16() {
    sampled!(_mm_mulhi_epi16, "sse2", 68, CASES, sample, [0, 1]);
}

#[test]
fn b069__mm_pause() {
    require_feature!(_mm_pause, "sse2");
    real::_mm_pause();
    let mut d = Digest::new();
    d.add(&[1]);
    d.line("_mm_pause", "none");
}

#[test]
fn b070__mm_setr_epi16() {
    sampled!(_mm_setr_epi16, "sse2", 70, CASES, sample, [0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn b071__mm_sll_epi16() {
    shifts!(_mm_sll_epi16, 71);
}

#[test]
fn b072__mm_sll_epi64() {
    shifts!(_mm_sll_epi64, 72);
}

#[test]
fn b073__mm_sra_epi16() {
    shifts!(_mm_sra_epi16, 73);
}

#[test]
fn b074__mm_sra_epi32() {
    shifts!(_mm_sra_epi32, 74);
}

#[test]
fn b075__mm_srl_epi64() {
    shifts!(_mm_srl_epi64, 75);
}

#[test]
fn b076__mm_storeu_si16() {
    cases!(_mm_storeu_si16, "sse2", 76, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add(case % 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_storeu_si16(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b077__mm_storeu_si32() {
    cases!(_mm_storeu_si32, "sse2", 77, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add(case % 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_storeu_si32(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b078__mm_storeu_si64() {
    cases!(_mm_storeu_si64, "sse2", 78, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add(case % 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_storeu_si64(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b079__mm_stream_si128() {
    cases!(_mm_stream_si128, "sse2", 79, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 16).cast();
        let a = sample(rng, case, 1);
        real::_mm_stream_si128(ptr, a);
        real::_mm_sfence();
        d.add(&dst.0);
    });
}

#[test]
fn b080__mm_stream_si32() {
    cases!(_mm_stream_si32, "sse2", 80, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 4).cast();
        let a = sample(rng, case, 1);
        real::_mm_stream_si32(ptr, a);
        real::_mm_sfence();
        d.add(&dst.0);
    });
}

#[test]
fn b081__mm_stream_si64() {
    cases!(_mm_stream_si64, "sse2", 81, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 8).cast();
        let a = sample(rng, case, 1);
        real::_mm_stream_si64(ptr, a);
        real::_mm_sfence();
        d.add(&dst.0);
    });
}

// SSE3

#[test]
fn b082__mm_addsub_ps() {
    sampled!(_mm_addsub_ps, "sse3", 82, CASES, sample, [0, 1]);
}

#[test]
fn b083__mm_hadd_ps() {
    sampled!(_mm_hadd_ps, "sse3", 83, CASES, sample, [0, 1]);
}

#[test]
fn b084__mm_hsub_ps() {
    sampled!(_mm_hsub_ps, "sse3", 84, CASES, sample, [0, 1]);
}

#[test]
fn b085__mm_moveldup_ps() {
    sampled!(_mm_moveldup_ps, "sse3", 85, CASES, sample, [0]);
}

// SSSE3

#[test]
fn b086__mm_hadd_epi16() {
    sampled!(_mm_hadd_epi16, "ssse3", 86, CASES, sample, [0, 1]);
}

#[test]
fn b087__mm_hadd_epi32() {
    sampled!(_mm_hadd_epi32, "ssse3", 87, CASES, sample, [0, 1]);
}

#[test]
fn b088__mm_hadds_epi16() {
    sampled!(_mm_hadds_epi16, "ssse3", 88, CASES, sample, [0, 1]);
}

#[test]
fn b089__mm_hsub_epi16() {
    sampled!(_mm_hsub_epi16, "ssse3", 89, CASES, sample, [0, 1]);
}

#[test]
fn b090__mm_hsub_epi32() {
    sampled!(_mm_hsub_epi32, "ssse3", 90, CASES, sample, [0, 1]);
}

#[test]
fn b091__mm_hsubs_epi16() {
    sampled!(_mm_hsubs_epi16, "ssse3", 91, CASES, sample, [0, 1]);
}

#[test]
fn b092__mm_mulhrs_epi16() {
    sampled!(_mm_mulhrs_epi16, "ssse3", 92, CASES, sample, [0, 1]);
}

#[test]
fn b093__mm_sign_epi16() {
    sampled!(_mm_sign_epi16, "ssse3", 93, CASES, sample, [0, 1]);
}

#[test]
fn b094__mm_sign_epi32() {
    sampled!(_mm_sign_epi32, "ssse3", 94, CASES, sample, [0, 1]);
}

#[test]
fn b095__mm_sign_epi8() {
    sampled!(_mm_sign_epi8, "ssse3", 95, CASES, sample, [0, 1]);
}

// SSE4.1

#[test]
fn b096__mm_blend_ps() {
    immediates!(4, _mm_blend_ps, "sse4.1", 96, CASES, sample,
        [a: real::__m128 = 0, b: real::__m128 = 1]);
}

#[test]
fn b097__mm_ceil_ss() {
    sampled!(_mm_ceil_ss, "sse4.1", 97, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b098__mm_cvtepi16_epi32() {
    sampled!(_mm_cvtepi16_epi32, "sse4.1", 98, CASES, sample, [0]);
}

#[test]
fn b099__mm_cvtepi16_epi64() {
    sampled!(_mm_cvtepi16_epi64, "sse4.1", 99, CASES, sample, [0]);
}

#[test]
fn b100__mm_cvtepi32_epi64() {
    sampled!(_mm_cvtepi32_epi64, "sse4.1", 100, CASES, sample, [0]);
}

#[test]
fn b101__mm_cvtepi8_epi16() {
    sampled!(_mm_cvtepi8_epi16, "sse4.1", 101, CASES, sample, [0]);
}

#[test]
fn b102__mm_cvtepi8_epi32() {
    sampled!(_mm_cvtepi8_epi32, "sse4.1", 102, CASES, sample, [0]);
}

#[test]
fn b103__mm_cvtepi8_epi64() {
    sampled!(_mm_cvtepi8_epi64, "sse4.1", 103, CASES, sample, [0]);
}

#[test]
fn b104__mm_cvtepu16_epi32() {
    sampled!(_mm_cvtepu16_epi32, "sse4.1", 104, CASES, sample, [0]);
}

#[test]
fn b105__mm_cvtepu16_epi64() {
    sampled!(_mm_cvtepu16_epi64, "sse4.1", 105, CASES, sample, [0]);
}

#[test]
fn b106__mm_cvtepu32_epi64() {
    sampled!(_mm_cvtepu32_epi64, "sse4.1", 106, CASES, sample, [0]);
}

#[test]
fn b107__mm_cvtepu8_epi32() {
    sampled!(_mm_cvtepu8_epi32, "sse4.1", 107, CASES, sample, [0]);
}

#[test]
fn b108__mm_cvtepu8_epi64() {
    sampled!(_mm_cvtepu8_epi64, "sse4.1", 108, CASES, sample, [0]);
}

#[test]
fn b109__mm_dp_ps() {
    immediates!(8, _mm_dp_ps, "sse4.1", 109, CASES, sample,
        [a: real::__m128 = 0, b: real::__m128 = 1]);
}

#[test]
fn b110__mm_extract_epi8() {
    immediates!(4, _mm_extract_epi8, "sse4.1", 110, CASES, sample,
        [a: real::__m128i = 0]);
}

#[test]
fn b111__mm_extract_ps() {
    immediates!(2, _mm_extract_ps, "sse4.1", 111, CASES, sample,
        [a: real::__m128 = 0]);
}

#[test]
fn b112__mm_floor_ss() {
    sampled!(_mm_floor_ss, "sse4.1", 112, FLOAT_CASES, floats4, [0, 1]);
}

#[test]
fn b113__mm_insert_epi8() {
    immediates!(4, _mm_insert_epi8, "sse4.1", 113, CASES, sample,
        [a: real::__m128i = 0, i: i32 = 1]);
}

#[test]
fn b114__mm_insert_ps() {
    immediates!(8, _mm_insert_ps, "sse4.1", 114, CASES, sample,
        [a: real::__m128 = 0, b: real::__m128 = 1]);
}

#[test]
fn b115__mm_minpos_epu16() {
    sampled!(_mm_minpos_epu16, "sse4.1", 115, CASES, sample, [0]);
}

#[test]
fn b116__mm_mpsadbw_epu8() {
    immediates!(3, _mm_mpsadbw_epu8, "sse4.1", 116, CASES, sample,
        [a: real::__m128i = 0, b: real::__m128i = 1]);
}

#[test]
fn b117__mm_mul_epi32() {
    sampled!(_mm_mul_epi32, "sse4.1", 117, CASES, sample, [0, 1]);
}

#[test]
fn b118__mm_round_ss() {
    immediates!(4, _mm_round_ss, "sse4.1", 118, FLOAT_CASES, floats4,
        [a: real::__m128 = 0, b: real::__m128 = 1]);
}

#[test]
fn b119__mm_stream_load_si128() {
    cases!(_mm_stream_load_si128, "sse4.1", 119, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 16).cast();
        d.add(bytes(&real::_mm_stream_load_si128(ptr)));
    });
}

#[test]
fn b120__mm_test_all_ones() {
    sampled!(_mm_test_all_ones, "sse4.1", 120, CASES, sample, [0]);
}

#[test]
fn b121__mm_test_all_zeros() {
    bit_tests!(_mm_test_all_zeros, "sse4.1", 121, 4);
}

#[test]
fn b122__mm_test_mix_ones_zeros() {
    bit_tests!(_mm_test_mix_ones_zeros, "sse4.1", 122, 4);
}

#[test]
fn b123__mm_testc_si128() {
    bit_tests!(_mm_testc_si128, "sse4.1", 123, 4);
}

#[test]
fn b124__mm_testnzc_si128() {
    bit_tests!(_mm_testnzc_si128, "sse4.1", 124, 4);
}

// FMA

#[test]
fn b125__mm_fmadd_ss() {
    sampled!(_mm_fmadd_ss, "fma", 125, CASES, sample, [0, 1, 2]);
}

#[test]
fn b126__mm_fmaddsub_ps() {
    sampled!(_mm_fmaddsub_ps, "fma", 126, CASES, sample, [0, 1, 2]);
}

#[test]
fn b127__mm_fmsub_ss() {
    sampled!(_mm_fmsub_ss, "fma", 127, CASES, sample, [0, 1, 2]);
}

#[test]
fn b128__mm_fmsubadd_ps() {
    sampled!(_mm_fmsubadd_ps, "fma", 128, CASES, sample, [0, 1, 2]);
}

#[test]
fn b129__mm_fnmadd_ss() {
    sampled!(_mm_fnmadd_ss, "fma", 129, CASES, sample, [0, 1, 2]);
}

#[test]
fn b130__mm_fnmsub_ss() {
    sampled!(_mm_fnmsub_ss, "fma", 130, CASES, sample, [0, 1, 2]);
}

#[test]
fn b131__mm256_fmaddsub_ps() {
    sampled!(_mm256_fmaddsub_ps, "fma", 131, CASES, sample, [0, 1, 2]);
}

#[test]
fn b132__mm256_fmsubadd_ps() {
    sampled!(_mm256_fmsubadd_ps, "fma", 132, CASES, sample, [0, 1, 2]);
}

// AVX

#[test]
fn b133__mm_broadcast_ss() {
    cases!(_mm_broadcast_ss, "avx", 133, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 4).cast();
        d.add(bytes(&real::_mm_broadcast_ss(&*ptr)));
    });
}

#[test]
fn b134__mm_cmp_ss() {
    immediates!(5, _mm_cmp_ss, "avx", 134, FLOAT_CASES, floats4,
        [a: real::__m128 = 0, b: real::__m128 = 1]);
}

#[test]
fn b135__mm_maskload_ps() {
    cases!(_mm_maskload_ps, "avx", 135, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add(case % 16).cast();
        let mask = sample(rng, case, 1);
        d.add(bytes(&real::_mm_maskload_ps(ptr, mask)));
    });
}

#[test]
fn b136__mm_maskstore_ps() {
    cases!(_mm_maskstore_ps, "avx", 136, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add(case % 16).cast();
        let mask = sample(rng, case, 1);
        let a = sample(rng, case, 2);
        real::_mm_maskstore_ps(ptr, mask, a);
        d.add(&dst.0);
    });
}

#[test]
fn b137__mm_permute_ps() {
    immediates!(8, _mm_permute_ps, "avx", 137, CASES, sample,
        [a: real::__m128 = 0]);
}

#[test]
fn b138__mm_permutevar_ps() {
    sampled!(_mm_permutevar_ps, "avx", 138, CASES, sample, [0, 1]);
}

#[test]
fn b139__mm_testc_ps() {
    bit_tests!(_mm_testc_ps, "avx", 139, 4);
}

#[test]
fn b140__mm_testnzc_ps() {
    bit_tests!(_mm_testnzc_ps, "avx", 140, 4);
}

#[test]
fn b141__mm_testz_ps() {
    bit_tests!(_mm_testz_ps, "avx", 141, 4);
}

#[test]
fn b142__mm256_addsub_ps() {
    sampled!(_mm256_addsub_ps, "avx", 142, CASES, sample, [0, 1]);
}

#[test]
fn b143__mm256_blend_ps() {
    immediates!(8, _mm256_blend_ps, "avx", 143, CASES, sample,
        [a: real::__m256 = 0, b: real::__m256 = 1]);
}

#[test]
fn b144__mm256_broadcast_ps() {
    cases!(_mm256_broadcast_ps, "avx", 144, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 16).cast();
        d.add(bytes(&real::_mm256_broadcast_ps(&*ptr)));
    });
}

#[test]
fn b145__mm256_broadcast_ss() {
    cases!(_mm256_broadcast_ss, "avx", 145, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add((case % 4) * 4).cast();
        d.add(bytes(&real::_mm256_broadcast_ss(&*ptr)));
    });
}

#[test]
fn b146__mm256_castps128_ps256() {
    cases!(_mm256_castps128_ps256, "avx", 146, CASES, "none", |rng, case, d| {
        let a = sample(rng, case, 0);
        let result = real::_mm256_castps128_ps256(a);
        // Only the low four lanes have specified values.
        d.add(&bytes(&result)[..16]);
    });
}

#[test]
fn b147__mm256_cvtps_epi32() {
    sampled!(_mm256_cvtps_epi32, "avx", 147, FLOAT_CASES, floats8, [0]);
}

#[test]
fn b148__mm256_cvtsd_f64() {
    sampled!(_mm256_cvtsd_f64, "avx", 148, CASES, sample, [0]);
}

#[test]
fn b149__mm256_cvtsi256_si32() {
    sampled!(_mm256_cvtsi256_si32, "avx", 149, CASES, sample, [0]);
}

#[test]
fn b150__mm256_cvtss_f32() {
    sampled!(_mm256_cvtss_f32, "avx", 150, CASES, sample, [0]);
}

#[test]
fn b151__mm256_dp_ps() {
    immediates!(8, _mm256_dp_ps, "avx", 151, CASES, sample,
        [a: real::__m256 = 0, b: real::__m256 = 1]);
}

#[test]
fn b152__mm256_hadd_ps() {
    sampled!(_mm256_hadd_ps, "avx", 152, CASES, sample, [0, 1]);
}

#[test]
fn b153__mm256_hsub_ps() {
    sampled!(_mm256_hsub_ps, "avx", 153, CASES, sample, [0, 1]);
}

#[test]
fn b154__mm256_insert_epi16() {
    immediates!(4, _mm256_insert_epi16, "avx", 154, CASES, sample,
        [a: real::__m256i = 0, i: i16 = 1]);
}

#[test]
fn b155__mm256_insert_epi32() {
    immediates!(3, _mm256_insert_epi32, "avx", 155, CASES, sample,
        [a: real::__m256i = 0, i: i32 = 1]);
}

#[test]
fn b156__mm256_insert_epi64() {
    immediates!(2, _mm256_insert_epi64, "avx", 156, CASES, sample,
        [a: real::__m256i = 0, i: i64 = 1]);
}

#[test]
fn b157__mm256_insert_epi8() {
    immediates!(5, _mm256_insert_epi8, "avx", 157, CASES, sample,
        [a: real::__m256i = 0, i: i8 = 1]);
}

#[test]
fn b158__mm256_insertf128_ps() {
    immediates!(1, _mm256_insertf128_ps, "avx", 158, CASES, sample,
        [a: real::__m256 = 0, b: real::__m128 = 1]);
}

#[test]
fn b159__mm256_loadu2_m128() {
    cases!(_mm256_loadu2_m128, "avx", 159, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let base = src.0.as_ptr();
        let hi = base.add(96 + case % 16).cast();
        let lo = base.add((case + 5) % 16).cast();
        d.add(bytes(&real::_mm256_loadu2_m128(hi, lo)));
    });
}

#[test]
fn b160__mm256_loadu2_m128d() {
    cases!(_mm256_loadu2_m128d, "avx", 160, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let base = src.0.as_ptr();
        let hi = base.add(96 + case % 16).cast();
        let lo = base.add((case + 5) % 16).cast();
        d.add(bytes(&real::_mm256_loadu2_m128d(hi, lo)));
    });
}

#[test]
fn b161__mm256_loadu2_m128i() {
    cases!(_mm256_loadu2_m128i, "avx", 161, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let base = src.0.as_ptr();
        let hi = base.add(96 + case % 16).cast();
        let lo = base.add((case + 5) % 16).cast();
        d.add(bytes(&real::_mm256_loadu2_m128i(hi, lo)));
    });
}

#[test]
fn b162__mm256_maskload_ps() {
    cases!(_mm256_maskload_ps, "avx", 162, CASES, "none", |rng, case, d| {
        let src: Buffer = sample(rng, case, 0);
        let ptr = src.0.as_ptr().add(case % 16).cast();
        let mask = sample(rng, case, 1);
        d.add(bytes(&real::_mm256_maskload_ps(ptr, mask)));
    });
}

#[test]
fn b163__mm256_maskstore_ps() {
    cases!(_mm256_maskstore_ps, "avx", 163, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add(case % 16).cast();
        let mask = sample(rng, case, 1);
        let a = sample(rng, case, 2);
        real::_mm256_maskstore_ps(ptr, mask, a);
        d.add(&dst.0);
    });
}

#[test]
fn b164__mm256_movehdup_ps() {
    sampled!(_mm256_movehdup_ps, "avx", 164, CASES, sample, [0]);
}

#[test]
fn b165__mm256_moveldup_ps() {
    sampled!(_mm256_moveldup_ps, "avx", 165, CASES, sample, [0]);
}

#[test]
fn b166__mm256_permute_ps() {
    immediates!(8, _mm256_permute_ps, "avx", 166, CASES, sample,
        [a: real::__m256 = 0]);
}

#[test]
fn b167__mm256_permute2f128_si256() {
    immediates!(8, _mm256_permute2f128_si256, "avx", 167, CASES, sample,
        [a: real::__m256i = 0, b: real::__m256i = 1]);
}

#[test]
fn b168__mm256_permutevar_ps() {
    sampled!(_mm256_permutevar_ps, "avx", 168, CASES, sample, [0, 1]);
}

#[test]
fn b169__mm256_rcp_ps() {
    cases!(_mm256_rcp_ps, "avx", 169, FLOAT_CASES, "none", |rng, case, d| {
        let input = float_lanes::<8>(rng, case, 0);
        let output: [f32; 8] = cast(real::_mm256_rcp_ps(cast(input)));
        for i in 0..8 {
            add_estimate(d, input[i], output[i], false);
        }
    });
}

#[test]
fn b170__mm256_rsqrt_ps() {
    cases!(_mm256_rsqrt_ps, "avx", 170, FLOAT_CASES, "none", |rng, case, d| {
        let input = float_lanes::<8>(rng, case, 0);
        let output: [f32; 8] = cast(real::_mm256_rsqrt_ps(cast(input)));
        for i in 0..8 {
            add_estimate(d, input[i], output[i], true);
        }
    });
}

#[test]
fn b171__mm256_set_m128() {
    sampled!(_mm256_set_m128, "avx", 171, CASES, sample, [0, 1]);
}

#[test]
fn b172__mm256_set_m128d() {
    sampled!(_mm256_set_m128d, "avx", 172, CASES, sample, [0, 1]);
}

#[test]
fn b173__mm256_setr_epi16() {
    sampled!(_mm256_setr_epi16, "avx", 173, CASES, sample,
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
}

#[test]
fn b174__mm256_setr_m128d() {
    sampled!(_mm256_setr_m128d, "avx", 174, CASES, sample, [0, 1]);
}

#[test]
fn b175__mm256_store_si256() {
    cases!(_mm256_store_si256, "avx", 175, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 32).cast();
        let a = sample(rng, case, 1);
        real::_mm256_store_si256(ptr, a);
        d.add(&dst.0);
    });
}

#[test]
fn b176__mm256_storeu2_m128() {
    cases!(_mm256_storeu2_m128, "avx", 176, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let base = dst.0.as_mut_ptr();
        let hi = base.add(96 + case % 16).cast();
        let lo = base.add((case + 5) % 16).cast();
        let a = sample(rng, case, 2);
        real::_mm256_storeu2_m128(hi, lo, a);
        d.add(&dst.0);
    });
}

#[test]
fn b177__mm256_storeu2_m128d() {
    cases!(_mm256_storeu2_m128d, "avx", 177, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let base = dst.0.as_mut_ptr();
        let hi = base.add(96 + case % 16).cast();
        let lo = base.add((case + 5) % 16).cast();
        let a = sample(rng, case, 2);
        real::_mm256_storeu2_m128d(hi, lo, a);
        d.add(&dst.0);
    });
}

#[test]
fn b178__mm256_storeu2_m128i() {
    cases!(_mm256_storeu2_m128i, "avx", 178, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let base = dst.0.as_mut_ptr();
        let hi = base.add(96 + case % 16).cast();
        let lo = base.add((case + 5) % 16).cast();
        let a = sample(rng, case, 2);
        real::_mm256_storeu2_m128i(hi, lo, a);
        d.add(&dst.0);
    });
}

#[test]
fn b179__mm256_stream_ps() {
    cases!(_mm256_stream_ps, "avx", 179, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 32).cast();
        let a = sample(rng, case, 1);
        real::_mm256_stream_ps(ptr, a);
        real::_mm_sfence();
        d.add(&dst.0);
    });
}

#[test]
fn b180__mm256_stream_si256() {
    cases!(_mm256_stream_si256, "avx", 180, CASES, "none", |rng, case, d| {
        let mut dst = Buffer([0; 192]);
        let ptr = dst.0.as_mut_ptr().add((case % 4) * 32).cast();
        let a = sample(rng, case, 1);
        real::_mm256_stream_si256(ptr, a);
        real::_mm_sfence();
        d.add(&dst.0);
    });
}

#[test]
fn b181__mm256_testc_ps() {
    bit_tests!(_mm256_testc_ps, "avx", 181, 8);
}

#[test]
fn b182__mm256_testc_si256() {
    bit_tests!(_mm256_testc_si256, "avx", 182, 8);
}

#[test]
fn b183__mm256_testnzc_ps() {
    bit_tests!(_mm256_testnzc_ps, "avx", 183, 8);
}

#[test]
fn b184__mm256_testnzc_si256() {
    bit_tests!(_mm256_testnzc_si256, "avx", 184, 8);
}

#[test]
fn b185__mm256_testz_ps() {
    bit_tests!(_mm256_testz_ps, "avx", 185, 8);
}

#[test]
fn b186__mm256_undefined_ps() {
    require_feature!(_mm256_undefined_ps, "avx");
    let _ = unsafe { real::_mm256_undefined_ps() };
    Digest::new().line("_mm256_undefined_ps", "none");
}

#[test]
fn b187__mm256_undefined_si256() {
    require_feature!(_mm256_undefined_si256, "avx");
    let _ = unsafe { real::_mm256_undefined_si256() };
    Digest::new().line("_mm256_undefined_si256", "none");
}

#[test]
fn b188__mm256_zextps128_ps256() {
    sampled!(_mm256_zextps128_ps256, "avx", 188, CASES, sample, [0]);
}

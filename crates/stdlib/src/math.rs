//! Mathematical functions.

/// Pi constant.
pub const PI: f64 = std::f64::consts::PI;

/// E constant.
pub const E: f64 = std::f64::consts::E;

/// Infinity.
pub const INF: f64 = f64::INFINITY;

/// Not a number.
pub const NAN: f64 = f64::NAN;

/// Square root.
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

/// Power.
pub fn pow(base: f64, exp: f64) -> f64 {
    base.powf(exp)
}

/// Natural logarithm.
pub fn log(x: f64) -> f64 {
    x.ln()
}

/// Base-10 logarithm.
pub fn log10(x: f64) -> f64 {
    x.log10()
}

/// Base-2 logarithm.
pub fn log2(x: f64) -> f64 {
    x.log2()
}

/// Exponential.
pub fn exp(x: f64) -> f64 {
    x.exp()
}

/// Sine.
pub fn sin(x: f64) -> f64 {
    x.sin()
}

/// Cosine.
pub fn cos(x: f64) -> f64 {
    x.cos()
}

/// Tangent.
pub fn tan(x: f64) -> f64 {
    x.tan()
}

/// Arc sine.
pub fn asin(x: f64) -> f64 {
    x.asin()
}

/// Arc cosine.
pub fn acos(x: f64) -> f64 {
    x.acos()
}

/// Arc tangent.
pub fn atan(x: f64) -> f64 {
    x.atan()
}

/// Arc tangent of y/x.
pub fn atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}

/// Hyperbolic sine.
pub fn sinh(x: f64) -> f64 {
    x.sinh()
}

/// Hyperbolic cosine.
pub fn cosh(x: f64) -> f64 {
    x.cosh()
}

/// Hyperbolic tangent.
pub fn tanh(x: f64) -> f64 {
    x.tanh()
}

/// Floor.
pub fn floor(x: f64) -> f64 {
    x.floor()
}

/// Ceiling.
pub fn ceil(x: f64) -> f64 {
    x.ceil()
}

/// Round.
pub fn round(x: f64) -> f64 {
    x.round()
}

/// Truncate.
pub fn trunc(x: f64) -> f64 {
    x.trunc()
}

/// Absolute value.
pub fn fabs(x: f64) -> f64 {
    x.abs()
}

/// Check if finite.
pub fn isfinite(x: f64) -> bool {
    x.is_finite()
}

/// Check if infinite.
pub fn isinf(x: f64) -> bool {
    x.is_infinite()
}

/// Check if NaN.
pub fn isnan(x: f64) -> bool {
    x.is_nan()
}

/// Greatest common divisor.
pub fn gcd(a: i64, b: i64) -> i64 {
    let mut a = a.abs();
    let mut b = b.abs();
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Least common multiple.
pub fn lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        0
    } else {
        (a.abs() / gcd(a, b)) * b.abs()
    }
}

/// Factorial.
pub fn factorial(n: u64) -> u64 {
    (1..=n).product()
}

/// Degrees to radians.
pub fn radians(degrees: f64) -> f64 {
    degrees * PI / 180.0
}

/// Radians to degrees.
pub fn degrees(radians: f64) -> f64 {
    radians * 180.0 / PI
}

/// Hypotenuse.
pub fn hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}


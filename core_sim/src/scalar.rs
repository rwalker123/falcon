use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

/// Fixed-point scalar with 6 decimal places of precision.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Scalar(pub i64);

impl Scalar {
    pub const SCALE: i64 = 1_000_000;

    pub fn from_f32(value: f32) -> Self {
        Self((value * Self::SCALE as f32).round() as i64)
    }

    pub fn from_i64(value: i64) -> Self {
        Self(value * Self::SCALE)
    }

    pub fn from_u32(value: u32) -> Self {
        Self((value as i64) * Self::SCALE)
    }

    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }

    pub fn zero() -> Self {
        Self(0)
    }

    pub fn one() -> Self {
        Self(Self::SCALE)
    }

    pub fn raw(self) -> i64 {
        self.0
    }

    pub fn from_raw(value: i64) -> Self {
        Self(value)
    }

    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    pub fn clamp(self, min: Self, max: Self) -> Self {
        match self.cmp(&min) {
            Ordering::Less => min,
            Ordering::Equal | Ordering::Greater => {
                if self > max {
                    max
                } else {
                    self
                }
            }
        }
    }

    pub fn round(self) -> Self {
        let half = if self.0 >= 0 {
            Self::SCALE / 2
        } else {
            -(Self::SCALE / 2)
        };
        Self(((self.0 + half) / Self::SCALE) * Self::SCALE)
    }

    pub fn to_u32(self) -> u32 {
        self.round().0.div_euclid(Self::SCALE) as u32
    }
}

impl Add for Scalar {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Scalar {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Scalar {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Scalar {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul for Scalar {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self((self.0 * rhs.0) / Self::SCALE)
    }
}

impl MulAssign for Scalar {
    fn mul_assign(&mut self, rhs: Self) {
        self.0 = (self.0 * rhs.0) / Self::SCALE;
    }
}

impl Div for Scalar {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self((self.0 * Self::SCALE) / rhs.0)
    }
}

impl DivAssign for Scalar {
    fn div_assign(&mut self, rhs: Self) {
        self.0 = (self.0 * Self::SCALE) / rhs.0;
    }
}

impl Neg for Scalar {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl fmt::Debug for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}", self.to_f32())
    }
}

impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}", self.to_f32())
    }
}

pub fn scalar_from_f32(value: f32) -> Scalar {
    Scalar::from_f32(value)
}

pub fn scalar_from_u32(value: u32) -> Scalar {
    Scalar::from_u32(value)
}

pub fn scalar_zero() -> Scalar {
    Scalar::zero()
}

pub fn scalar_one() -> Scalar {
    Scalar::one()
}

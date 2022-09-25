use std::fmt::{Display, Formatter};
use std::ops::{Add, Sub};

#[derive(Copy, Clone, Debug)]
pub struct Decibels {
    inner: f64,
}

impl Decibels {
    pub fn as_f64(&self) -> f64 { self.inner }
}

impl Default for Decibels {
    fn default() -> Decibels { Decibels::from(0.0) }
}

impl const From<f64> for Decibels {
    fn from(value: f64) -> Decibels { Decibels { inner: value } }
}

impl Display for Decibels {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(formatter, "{} dB", self.inner)
    }
}

impl Sub for Decibels {
    type Output = Decibels;

    fn sub(self, other: Decibels) -> Decibels { Decibels { inner: self.inner - other.inner } }
}

impl Add for Decibels {
    type Output = Decibels;

    fn add(self, other: Decibels) -> Decibels { Decibels { inner: self.inner + other.inner } }
}

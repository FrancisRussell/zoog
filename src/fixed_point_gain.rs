use crate::{Decibels, Error};
use std::convert::TryFrom;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub struct FixedPointGain {
    value: i16,
}

impl FixedPointGain {
    pub fn as_fixed_point(self) -> i16 { self.value }

    pub fn as_decibels(self) -> Decibels {
        Decibels::from(self.value as f64 / 256.0)
    }

    pub fn from_integer(value: i16) -> FixedPointGain {
        FixedPointGain {
            value,
        }
    }

    pub fn is_zero(self) -> bool { self.value == 0 }

    pub fn checked_add(self, rhs: FixedPointGain) -> Option<FixedPointGain> {
        self.value.checked_add(rhs.value).map(|value| FixedPointGain { value })
    }

    pub fn checked_neg(self) -> Option<FixedPointGain> { self.value.checked_neg().map(|value| FixedPointGain { value }) }
}

impl TryFrom<Decibels> for FixedPointGain {
    type Error = Error;

    fn try_from(value: Decibels) -> Result<FixedPointGain, Error> {
        let fixed = (value.as_f64() * 256.0).round();
        let value = fixed as i16;
        if ((value as f64) - fixed).abs() < std::f64::EPSILON {
            Ok(FixedPointGain { value })
        } else {
            Err(Error::GainOutOfBounds)
        }
    }
}

impl FromStr for FixedPointGain {
    type Err = <i16 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> { s.parse::<i16>().map(|value| FixedPointGain { value }) }
}

impl Display for FixedPointGain {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(formatter, "{}", self.as_decibels())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_db_is_none() {
        assert!(FixedPointGain::try_from(Decibels::default()).unwrap().is_zero());
    }

    #[test]
    fn positive_overflow() {
        let max_gain = FixedPointGain { value: std::i16::MAX };
        let one = FixedPointGain { value: 1 };
        assert_eq!(max_gain.checked_add(one), None);
        assert_eq!(one.checked_add(max_gain), None);
    }

    #[test]
    fn negative_overflow() {
        let min_gain = FixedPointGain { value: std::i16::MIN };
        let neg_one = FixedPointGain { value: -1 };
        assert_eq!(min_gain.checked_add(neg_one), None);
        assert_eq!(neg_one.checked_add(min_gain), None);
    }

    #[test]
    fn negate_lowest_value() {
        let min_gain = FixedPointGain { value: std::i16::MIN };
        assert_eq!(min_gain.checked_neg(), None);
    }

    #[test]
    fn decibel_conversion() {
        for value in std::i16::MIN..=std::i16::MAX {
            let gain = FixedPointGain { value };
            let decibels = gain.as_decibels();
            let gain2 = FixedPointGain::try_from(decibels).unwrap();
            assert_eq!(gain, gain2);
        }
    }

    #[test]
    fn parse_valid() {
        assert_eq!("-32768".parse::<FixedPointGain>(), Ok(FixedPointGain { value: -32768 }));
        assert_eq!("-1".parse::<FixedPointGain>(), Ok(FixedPointGain { value: -1 }));
        assert_eq!("0".parse::<FixedPointGain>(), Ok(FixedPointGain { value: 0 }));
        assert_eq!("1".parse::<FixedPointGain>(), Ok(FixedPointGain { value: 1 }));
        assert_eq!("32767".parse::<FixedPointGain>(), Ok(FixedPointGain { value: 32767 }));
    }

    #[test]
    fn parse_invalid() {
        assert!("-32769".parse::<FixedPointGain>().is_err());
        assert!("32768".parse::<FixedPointGain>().is_err());
        assert!("0.0".parse::<FixedPointGain>().is_err());
        assert!("".parse::<FixedPointGain>().is_err());
    }
}

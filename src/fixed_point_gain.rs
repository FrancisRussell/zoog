use std::str::FromStr;

#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub struct FixedPointGain {
    pub(crate) value: i16,
}

impl FixedPointGain {
    pub fn as_decibels(self) -> f64 { self.value as f64 / 256.0 }

    pub fn as_fixed_point(self) -> i16 { self.value }

    pub fn from_decibels(value: f64) -> Option<FixedPointGain> {
        let fixed = (value * 256.0).round();
        let value = fixed as i16;
        if ((value as f64) - fixed).abs() < std::f64::EPSILON {
            Some(FixedPointGain { value })
        } else {
            None
        }
    }

    pub fn is_zero(self) -> bool { self.value == 0 }

    pub fn checked_add(self, rhs: FixedPointGain) -> Option<FixedPointGain> {
        self.value.checked_add(rhs.value).map(|value| FixedPointGain { value })
    }

    pub fn checked_neg(self) -> Option<FixedPointGain> { self.value.checked_neg().map(|value| FixedPointGain { value }) }
}

impl FromStr for FixedPointGain {
    type Err = <i16 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> { s.parse::<i16>().map(|value| FixedPointGain { value }) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_db_is_none() {
        assert!(FixedPointGain::from_decibels(0.0).unwrap().is_zero());
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
            let gain2 = FixedPointGain::from_decibels(decibels).unwrap();
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

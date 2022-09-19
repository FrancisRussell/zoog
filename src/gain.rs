use std::str::FromStr;

#[derive(Default, Copy, Clone, Debug, Eq, PartialEq)]
pub struct Gain {
    pub(crate) value: i16,
}

impl Gain {
    pub fn as_decibels(self) -> f64 { self.value as f64 / 256.0 }

    pub fn as_fixed_point(self) -> i16 { self.value }

    pub fn from_decibels(value: f64) -> Option<Gain> {
        let fixed = (value * 256.0).round();
        let value = fixed as i16;
        if ((value as f64) - fixed).abs() < std::f64::EPSILON {
            Some(Gain { value })
        } else {
            None
        }
    }

    pub fn is_zero(self) -> bool { self.value == 0 }

    pub fn checked_add(self, rhs: Gain) -> Option<Gain> {
        self.value.checked_add(rhs.value).map(|value| Gain { value })
    }

    pub fn checked_neg(self) -> Option<Gain> { self.value.checked_neg().map(|value| Gain { value }) }
}

impl FromStr for Gain {
    type Err = <i16 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> { s.parse::<i16>().map(|value| Gain { value }) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_db_is_none() {
        assert!(Gain::from_decibels(0.0).unwrap().is_zero());
    }

    #[test]
    fn positive_overflow() {
        let max_gain = Gain { value: std::i16::MAX };
        let one = Gain { value: 1 };
        assert_eq!(max_gain.checked_add(one), None);
        assert_eq!(one.checked_add(max_gain), None);
    }

    #[test]
    fn negative_overflow() {
        let min_gain = Gain { value: std::i16::MIN };
        let neg_one = Gain { value: -1 };
        assert_eq!(min_gain.checked_add(neg_one), None);
        assert_eq!(neg_one.checked_add(min_gain), None);
    }

    #[test]
    fn negate_lowest_value() {
        let min_gain = Gain { value: std::i16::MIN };
        assert_eq!(min_gain.checked_neg(), None);
    }

    #[test]
    fn decibel_conversion() {
        for value in std::i16::MIN..=std::i16::MAX {
            let gain = Gain { value };
            let decibels = gain.as_decibels();
            let gain2 = Gain::from_decibels(decibels).unwrap();
            assert_eq!(gain, gain2);
        }
    }

    #[test]
    fn parse_valid() {
        assert_eq!("-32768".parse::<Gain>(), Ok(Gain { value: -32768 }));
        assert_eq!("-1".parse::<Gain>(), Ok(Gain { value: -1 }));
        assert_eq!("0".parse::<Gain>(), Ok(Gain { value: 0 }));
        assert_eq!("1".parse::<Gain>(), Ok(Gain { value: 1 }));
        assert_eq!("32767".parse::<Gain>(), Ok(Gain { value: 32767 }));
    }

    #[test]
    fn parse_invalid() {
        assert!("-32769".parse::<Gain>().is_err());
        assert!("32768".parse::<Gain>().is_err());
        assert!("0.0".parse::<Gain>().is_err());
        assert!("".parse::<Gain>().is_err());
    }
}

use std::str::FromStr;

#[derive(Default, Copy, Clone, Debug)]
pub struct Gain {
    pub (crate) value: i16,
}

impl Gain {
    pub fn as_decibels(&self) -> f64 {
        self.value as f64 / 256.0
    }

    pub fn as_fixed_point(&self) -> i16 {
        self.value
    }

    pub fn from_decibels(value: f64) -> Gain {
        Gain {
            value: (value * 256.0) as i16,
        }
    }

    pub fn is_none(&self) -> bool {
        self.value == 0
    }

    pub fn checked_add(&self, rhs: &Gain) -> Option<Gain> {
        self.value.checked_add(rhs.value).map(|value| Gain { value })
    }

    pub fn checked_neg(&self) -> Option<Gain> {
        self.value.checked_neg().map(|value| Gain { value })
    }

}

impl FromStr for Gain {
    type Err = <i16 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.parse::<i16>()?;
        Ok(Gain {
            value,
        })
    }
}



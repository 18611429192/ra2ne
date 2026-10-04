//! Standard RA2 armor slots. Custom Ares/Phobos ArmorTypes remain unsupported.
use crate::rules::FixedDecimal;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Armor {
    #[default]
    None,
    Flak,
    Plate,
    Light,
    Medium,
    Heavy,
    Wood,
    Steel,
    Concrete,
    Special1,
    Special2,
}
impl Armor {
    pub const NAMES: [&'static str; 11] = [
        "none",
        "flak",
        "plate",
        "light",
        "medium",
        "heavy",
        "wood",
        "steel",
        "concrete",
        "special_1",
        "special_2",
    ];
    pub const ALL: [Self; 11] = [
        Self::None,
        Self::Flak,
        Self::Plate,
        Self::Light,
        Self::Medium,
        Self::Heavy,
        Self::Wood,
        Self::Steel,
        Self::Concrete,
        Self::Special1,
        Self::Special2,
    ];
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        Self::NAMES
            .iter()
            .position(|name| name.eq_ignore_ascii_case(text.trim()))
            .map(|i| Self::ALL[i])
            .ok_or("unknown or custom armor type")
    }
    pub fn from_index(index: u32) -> Result<Self, &'static str> {
        Self::ALL
            .get(index as usize)
            .copied()
            .ok_or("invalid armor index")
    }
}

/// Thousandths of a percent: 100% = 100_000. Damage uses integer floor and
/// saturates at u32::MAX. Original rounding/minimum-damage behavior is unverified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Verses(pub [u32; 11]);
impl Default for Verses {
    fn default() -> Self {
        Self([100_000; 11])
    }
}
impl Verses {
    pub const SCALE: u64 = 100_000;
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        let items: Vec<_> = text.split(',').collect();
        if items.len() != 11 {
            return Err("Verses requires exactly eleven percentage values");
        }
        let mut values = [0; 11];
        for (i, item) in items.iter().enumerate() {
            let percent = item
                .trim()
                .strip_suffix('%')
                .ok_or("Verses values require percent suffix")?;
            values[i] = u32::try_from(FixedDecimal::parse(percent)?.0)
                .map_err(|_| "negative or overflowing Verses value")?;
        }
        Ok(Self(values))
    }
    pub fn damage(self, base: u32, armor: Armor) -> u32 {
        (u64::from(base) * u64::from(self.0[armor as usize]) / Self::SCALE).min(u64::from(u32::MAX))
            as u32
    }
    pub fn can_target(self, armor: Armor) -> bool {
        self.0[armor as usize] != 0
    }
    pub fn passive_acquire(self, armor: Armor) -> bool {
        ![0, 1000, 2000].contains(&self.0[armor as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn armor_order_and_exact_percentages_match_slots() {
        for (i, name) in Armor::NAMES.iter().enumerate() {
            assert_eq!(
                Armor::parse(&name.to_ascii_uppercase()).unwrap() as usize,
                i
            );
            assert_eq!(Armor::from_index(i as u32).unwrap() as usize, i);
        }
        let v = Verses::parse("0%,1%,2%,50%,100%,150%,0.5%,0.001%,33.333%,200%,100%").unwrap();
        assert_eq!(v.damage(21, Armor::Light), 10);
        assert_eq!(v.damage(20, Armor::Heavy), 30);
        assert!(!v.can_target(Armor::None));
        assert!(!v.passive_acquire(Armor::Flak));
        assert!(!v.passive_acquire(Armor::Plate));
        assert!(v.passive_acquire(Armor::Wood));
        assert_eq!(
            Verses([u32::MAX; 11]).damage(u32::MAX, Armor::None),
            u32::MAX
        );
    }
    #[test]
    fn malformed_verses_and_custom_armor_fail() {
        for s in [
            "100%",
            "100%,100%,100%,100%,100%,100%,100%,100%,100%,100%,-1%",
            "100%,100%,100%,100%,100%,100%,100%,100%,100%,100%,1.0001%",
            "100%,100%,100%,100%,100%,100%,100%,100%,100%,100%,1",
        ] {
            assert!(Verses::parse(s).is_err());
        }
        assert!(Armor::parse("custom").is_err());
        assert!(Armor::from_index(11).is_err());
    }
}

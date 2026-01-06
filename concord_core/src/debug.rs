#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
#[derive(Default)]
pub enum DebugLevel {
    #[default]
    None = 0,
    V = 1,
    VV = 2,
}

impl DebugLevel {
    #[inline]
    pub fn is_enabled(self) -> bool {
        self != DebugLevel::None
    }

    #[inline]
    pub fn is_verbose(self) -> bool {
        self >= DebugLevel::V
    }

    #[inline]
    pub fn is_very_verbose(self) -> bool {
        self >= DebugLevel::VV
    }
}

impl core::fmt::Display for DebugLevel {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DebugLevel::None => f.write_str("none"),
            DebugLevel::V => f.write_str("v"),
            DebugLevel::VV => f.write_str("vv"),
        }
    }
}

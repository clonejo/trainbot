use std::fmt;

/// A FourCC pixel-format identifier (V4L2-compatible, stored little-endian).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCC(u32);

impl FourCC {
    pub const MJPG: Self = Self::from_le_bytes(b"MJPG");
    pub const YUYV: Self = Self::from_le_bytes(b"YUYV");
    /// YUV 4:2:0 planar (V4L2 YU12 / rpicam yuv420).
    pub const YU12: Self = Self::from_le_bytes(b"YU12");

    pub const fn from_le_bytes(b: &[u8; 4]) -> Self {
        Self(u32::from_le_bytes(*b))
    }

    pub fn parse(s: &str) -> Option<Self> {
        let arr: [u8; 4] = s.as_bytes().try_into().ok()?;
        Some(Self::from_le_bytes(&arr))
    }

    pub fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for FourCC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(&self.to_le_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mjpg_roundtrip() {
        let fcc = FourCC::parse("MJPG").unwrap();
        assert_eq!(fcc, FourCC::MJPG);
        assert_eq!(fcc.to_string(), "MJPG");
    }

    #[test]
    fn yuyv_roundtrip() {
        let fcc = FourCC::parse("YUYV").unwrap();
        assert_eq!(fcc, FourCC::YUYV);
        assert_eq!(fcc.to_string(), "YUYV");
    }

    #[test]
    fn bad_length_returns_none() {
        assert!(FourCC::parse("MJP").is_none());
        assert!(FourCC::parse("MJPGG").is_none());
    }
}

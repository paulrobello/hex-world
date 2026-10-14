//! Minimal KTX2 header reader for the asset gate (the runtime loads KTX2 through Bevy).

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
pub const VK_BC7_UNORM: u32 = 145;
pub const VK_BC7_SRGB: u32 = 146;
pub const VK_ASTC_4X4_UNORM: u32 = 157;
pub const VK_ASTC_4X4_SRGB: u32 = 158;
pub const VK_R16G16B16A16_SFLOAT: u32 = 97;
pub const SUPERCOMPRESSION_ZSTD: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ktx2Info {
    pub vk_format: u32,
    pub width: u32,
    pub height: u32,
    pub layers: u32,
    pub faces: u32,
    pub levels: u32,
    pub supercompression: u32,
}

pub fn ktx2_info(b: &[u8]) -> Option<Ktx2Info> {
    if b.len() < 48 || b[..12] != IDENTIFIER {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    Some(Ktx2Info {
        vk_format: u(12),
        width: u(20),
        height: u(24),
        layers: u(32),
        faces: u(36),
        levels: u(40),
        supercompression: u(44),
    })
}

/// `Some(true)` for the sRGB block formats the kit ships, `Some(false)` for their UNORM twins.
pub fn ktx2_is_srgb(vk: u32) -> Option<bool> {
    match vk {
        VK_BC7_SRGB | VK_ASTC_4X4_SRGB => Some(true),
        VK_BC7_UNORM | VK_ASTC_4X4_UNORM => Some(false),
        _ => None,
    }
}

pub fn full_mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(vk: u32, w: u32, levels: u32) -> Vec<u8> {
        let mut b = IDENTIFIER.to_vec();
        for v in [vk, 1, w, w, 0, 0, 1, levels, SUPERCOMPRESSION_ZSTD] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_fields() {
        let i = ktx2_info(&header(VK_BC7_SRGB, 1024, 11)).unwrap();
        assert_eq!((i.vk_format, i.width, i.levels), (VK_BC7_SRGB, 1024, 11));
        assert_eq!(i.supercompression, SUPERCOMPRESSION_ZSTD);
    }

    #[test]
    fn rejects_non_ktx2_and_short_input() {
        assert!(ktx2_info(b"\x89PNG\r\n\x1a\n").is_none());
        assert!(ktx2_info(&header(VK_BC7_SRGB, 4, 3)[..40]).is_none());
    }

    #[test]
    fn color_space_table() {
        assert_eq!(ktx2_is_srgb(146), Some(true));
        assert_eq!(ktx2_is_srgb(158), Some(true));
        assert_eq!(ktx2_is_srgb(145), Some(false));
        assert_eq!(ktx2_is_srgb(157), Some(false));
        assert_eq!(ktx2_is_srgb(37), None);
    }

    #[test]
    fn mip_counts() {
        assert_eq!(full_mip_count(1024, 1024), 11);
        assert_eq!(full_mip_count(64, 32), 7);
        assert_eq!(full_mip_count(1, 1), 1);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureFamily {
    Bc7,
    Astc,
    Png,
}

/// Picks the runtime texture variant from the device's compression features (PRD §9.3).
pub fn select_family(bc: bool, astc: bool) -> TextureFamily {
    if bc {
        TextureFamily::Bc7
    } else if astc {
        TextureFamily::Astc
    } else {
        TextureFamily::Png
    }
}

pub fn family_suffix(f: TextureFamily) -> &'static str {
    match f {
        TextureFamily::Bc7 => "bc7.ktx2",
        TextureFamily::Astc => "astc.ktx2",
        TextureFamily::Png => "png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_bc7_on_desktop() {
        assert_eq!(select_family(true, false), TextureFamily::Bc7);
        assert_eq!(select_family(true, true), TextureFamily::Bc7);
    }

    #[test]
    fn falls_back_to_astc_then_png() {
        assert_eq!(select_family(false, true), TextureFamily::Astc);
        assert_eq!(select_family(false, false), TextureFamily::Png);
    }

    #[test]
    fn suffixes() {
        assert_eq!(family_suffix(TextureFamily::Bc7), "bc7.ktx2");
        assert_eq!(family_suffix(TextureFamily::Astc), "astc.ktx2");
        assert_eq!(family_suffix(TextureFamily::Png), "png");
    }
}

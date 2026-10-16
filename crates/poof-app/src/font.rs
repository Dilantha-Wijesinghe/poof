//! Bundled typeface: Inter (SIL Open Font License, see `assets/fonts/OFL.txt`).

use iced::Font;
use iced::font::{Family, Weight};

/// Font files to register with iced at startup.
pub const FILES: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
];

const fn inter(weight: Weight) -> Font {
    Font { family: Family::Name("Inter"), weight, ..Font::DEFAULT }
}

pub const REGULAR: Font = inter(Weight::Normal);
pub const MEDIUM: Font = inter(Weight::Medium);
pub const SEMIBOLD: Font = inter(Weight::Semibold);
pub const BOLD: Font = inter(Weight::Bold);

use cosmic_config::CosmicConfigEntry;
use cosmic_theme::palette::{self, FromColor};

fn main() {
    //TODO: read dark/light
    let theme_config = cosmic_theme::Theme::dark_config().unwrap();
    let theme = cosmic_theme::Theme::get_entry(&theme_config).unwrap();
    let hsv = palette::Hsv::from_color(theme.accent.base.color).into_format::<u8>();
    let rgb = palette::Srgb::from_color(theme.accent.base.color).into_format::<u8>();
    println!("{} {} {} {:02X}{:02X}{:02X}", hsv.hue.into_inner(), hsv.saturation, hsv.value, rgb.red, rgb.green, rgb.blue);
}

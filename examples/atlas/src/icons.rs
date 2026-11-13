//! The icons of the app: the design's own inline SVGs, one file per
//! distinct drawing in `assets/svg`, loaded with the SVG loader (ADR 0027).
//!
//! `design/extract-icons.py` wrote the files from `design/html/*.html`:
//! the markup as the design has it, with `sc-camel-view-box` spelt
//! `viewBox`, the namespace added and the colour made `currentColor`, so an
//! icon takes its colour where it is drawn (`Icon::svg(X.get()).color(..)`).
//! The same outline at two stroke widths is two files, as the loader draws
//! a file as written. `library-active` and the two halves of the spinner
//! are the app's own (see their notes); everything else is the design's.

use morningstar::ui::prelude::*;

/// The back arrow of a pushed screen (22 dp, `stroke-width="1.9"`).
pub static BACK: SvgAsset = SvgAsset::new(include_str!("../assets/svg/back.svg"));
/// The camera of live voice.
pub static CAMERA: SvgAsset = SvgAsset::new(include_str!("../assets/svg/camera.svg"));
/// The tick of a finished step and of a plan's benefits (`stroke-width="2.4"`).
pub static CHECK: SvgAsset = SvgAsset::new(include_str!("../assets/svg/check.svg"));
/// The tick in the selected model's disc (`stroke-width="3"`).
pub static CHECK_BOLD: SvgAsset = SvgAsset::new(include_str!("../assets/svg/check-bold.svg"));
/// The chevron after the model's name and on the step card.
pub static CHEVRON_DOWN: SvgAsset = SvgAsset::new(include_str!("../assets/svg/chevron-down.svg"));
/// The chevron of a row that leads somewhere.
pub static CHEVRON_RIGHT: SvgAsset = SvgAsset::new(include_str!("../assets/svg/chevron-right.svg"));
/// The cross that ends live voice.
pub static CLOSE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/close.svg"));
/// The Explore tab.
pub static COMPASS: SvgAsset = SvgAsset::new(include_str!("../assets/svg/compass.svg"));
/// The Explore tab when selected (`stroke-width="1.9"`).
pub static COMPASS_ACTIVE: SvgAsset =
    SvgAsset::new(include_str!("../assets/svg/compass-active.svg"));
/// A document: "Summarise a PDF", "Summarise documents".
pub static FILE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/file.svg"));
/// The document of an attachment chip (`stroke-width="1.8"`).
pub static FILE_CHIP: SvgAsset = SvgAsset::new(include_str!("../assets/svg/file-chip.svg"));
/// The web: "Research a topic", "Web research".
pub static GLOBE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/globe.svg"));
/// The Home tab.
pub static HOME: SvgAsset = SvgAsset::new(include_str!("../assets/svg/home.svg"));
/// The Home tab when selected (`stroke-width="1.9"`).
pub static HOME_ACTIVE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/home-active.svg"));
/// A picture: "Create an image", "Generate images".
pub static IMAGE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/image.svg"));
/// The Library tab.
pub static LIBRARY: SvgAsset = SvgAsset::new(include_str!("../assets/svg/library.svg"));
/// The Library tab when selected: the design never shows it, so this is its file with the selected tabs' `stroke-width="1.9"`.
pub static LIBRARY_ACTIVE: SvgAsset =
    SvgAsset::new(include_str!("../assets/svg/library-active.svg"));
/// The password field.
pub static LOCK: SvgAsset = SvgAsset::new(include_str!("../assets/svg/lock.svg"));
/// The email field.
pub static MAIL: SvgAsset = SvgAsset::new(include_str!("../assets/svg/mail.svg"));
/// The microphone of the composer (`stroke-width="1.7"`).
pub static MIC: SvgAsset = SvgAsset::new(include_str!("../assets/svg/mic.svg"));
/// The microphone of live voice (`stroke-width="1.6"`).
pub static MIC_LARGE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/mic-large.svg"));
/// The chevron that minimises live voice (`stroke-width="1.9"`).
pub static MINIMISE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/minimise.svg"));
/// The "+" of the composer.
pub static PLUS: SvgAsset = SvgAsset::new(include_str!("../assets/svg/plus.svg"));
/// The magnifier of Explore's search field.
pub static SEARCH: SvgAsset = SvgAsset::new(include_str!("../assets/svg/search.svg"));
/// The arrow of the send button.
pub static SEND: SvgAsset = SvgAsset::new(include_str!("../assets/svg/send.svg"));
/// The spinner's dark quarter: CSS `border-top-color` on a round box with a 2.5 px border, which a browser draws as the arc from -135 to -45 degrees.
pub static SPINNER_ARC: SvgAsset = SvgAsset::new(include_str!("../assets/svg/spinner-arc.svg"));
/// The spinner's other three quarters.
pub static SPINNER_TRACK: SvgAsset = SvgAsset::new(include_str!("../assets/svg/spinner-track.svg"));
/// A table: "Analyse spreadsheets".
pub static TABLE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/table.svg"));
/// "Translate".
pub static TRANSLATE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/translate.svg"));
/// The profile button.
pub static USER: SvgAsset = SvgAsset::new(include_str!("../assets/svg/user.svg"));
/// The bars beside "Listening" (`stroke-width="2"`).
pub static WAVEFORM: SvgAsset = SvgAsset::new(include_str!("../assets/svg/waveform.svg"));
/// The bars of "Transcribe audio" (`stroke-width="1.6"`).
pub static WAVEFORM_LIGHT: SvgAsset =
    SvgAsset::new(include_str!("../assets/svg/waveform-light.svg"));

/// Every icon, with its file's name: for the test that each loads.
pub static ALL: [(&str, &SvgAsset); 32] = [
    ("back", &BACK),
    ("camera", &CAMERA),
    ("check", &CHECK),
    ("check-bold", &CHECK_BOLD),
    ("chevron-down", &CHEVRON_DOWN),
    ("chevron-right", &CHEVRON_RIGHT),
    ("close", &CLOSE),
    ("compass", &COMPASS),
    ("compass-active", &COMPASS_ACTIVE),
    ("file", &FILE),
    ("file-chip", &FILE_CHIP),
    ("globe", &GLOBE),
    ("home", &HOME),
    ("home-active", &HOME_ACTIVE),
    ("image", &IMAGE),
    ("library", &LIBRARY),
    ("library-active", &LIBRARY_ACTIVE),
    ("lock", &LOCK),
    ("mail", &MAIL),
    ("mic", &MIC),
    ("mic-large", &MIC_LARGE),
    ("minimise", &MINIMISE),
    ("plus", &PLUS),
    ("search", &SEARCH),
    ("send", &SEND),
    ("spinner-arc", &SPINNER_ARC),
    ("spinner-track", &SPINNER_TRACK),
    ("table", &TABLE),
    ("translate", &TRANSLATE),
    ("user", &USER),
    ("waveform", &WAVEFORM),
    ("waveform-light", &WAVEFORM_LIGHT),
];

/// An icon `size` dp square in `color`.
pub fn icon(asset: &'static SvgAsset, size: f32, color: Color) -> Icon {
    Icon::svg(asset.get()).size(size).color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads_and_takes_one_colour() {
        for (name, asset) in ALL {
            assert!(asset.check().is_ok(), "{name}: {:?}", asset.check());
        }
    }
}

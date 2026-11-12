//! The one-colour icons of the app, each the design's own inline SVG.
//!
//! An icon takes its colour where it is drawn (`Icon::vector(X).color(..)`),
//! so the colours in the markup below do not matter. Only a subset of SVG
//! is understood and an icon outside it draws nothing: the test at the
//! bottom checks every one.

use morningstar::ui::prelude::*;

/// The back arrow: `<svg viewBox="0 0 24 24" fill="none" stroke="#FFFFFF"
/// stroke-width="2" stroke-linecap="round" stroke-linejoin="round">`.
pub const BACK: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 24 24" fill="none" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12H5M11 5l-7 7 7 7"></path></svg>"##,
);

/// The cross that closes the offer: `stroke-width="2.2"`.
pub const CLOSE: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 24 24" fill="none" stroke="#FFFFFF" stroke-width="2.2" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"></path></svg>"##,
);

/// The tick before each benefit of the offer: `stroke-width="2.6"`.
pub const CHECK: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 24 24" fill="none" stroke="#F6C445" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12l5 5L20 6"></path></svg>"##,
);

/// The chevron after the name on home, and after every row that leads
/// somewhere: `<svg width="16" height="16" viewBox="0 0 16 16">`.
pub const CHEVRON: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3l5 5-5 5"/></svg>"#,
);

/// The bell.
pub const BELL: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9a6 6 0 0 1 12 0c0 6 2.5 7.5 2.5 7.5h-17S6 15 6 9z"/><path d="M10 20a2 2 0 0 0 4 0"/></svg>"#,
);

/// The handset: on home's call button, and turned by 135 degrees on the
/// button that ends a call.
pub const HANDSET: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="currentColor"><path d="M6.6 10.8a15.1 15.1 0 0 0 6.6 6.6l2.2-2.2a1 1 0 0 1 1-.25 11.4 11.4 0 0 0 3.6.57 1 1 0 0 1 1 1V20a1 1 0 0 1-1 1A17 17 0 0 1 3 4a1 1 0 0 1 1-1h3.5a1 1 0 0 1 1 1c0 1.25.2 2.45.57 3.57a1 1 0 0 1-.25 1z"/></svg>"#,
);

/// The gear of "Scenario settings". The design's path carries
/// `transform="translate(-1 0)"`, which an icon cannot; the icon is moved
/// one dp left where it is drawn (24 dp for a view box of 24).
pub const GEAR: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="currentColor"><path d="M19.4 13a7.5 7.5 0 0 0 0-2l2.1-1.6-2-3.5-2.5 1a7.4 7.4 0 0 0-1.7-1L15 3h-4l-.4 2.9a7.4 7.4 0 0 0-1.7 1l-2.5-1-2 3.5L6.5 11a7.5 7.5 0 0 0 0 2l-2.1 1.6 2 3.5 2.5-1a7.4 7.4 0 0 0 1.7 1L11 21h4l.4-2.9a7.4 7.4 0 0 0 1.7-1l2.5 1 2-3.5zM13 15.5A3.5 3.5 0 1 1 13 8.5a3.5 3.5 0 0 1 0 7z"/></svg>"#,
);

/// The tab bar's house.
pub const TAB_HOME: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 3l9 7.5V21h-6v-6H9v6H3V10.5z"/></svg>"#,
);

/// The tab bar's chart.
pub const TAB_PROGRESS: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M5 20v-6M12 20V6M19 20v-9M3 21h18"/></svg>"#,
);

/// The tab bar's list.
pub const TAB_HISTORY: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M4 6h12M4 11h12M4 16h7"/><circle cx="17" cy="17" r="3"/><path d="M20 17V8"/></svg>"#,
);

/// The tab bar's three dots.
pub const TAB_MORE: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>"#,
);

/// The loudspeaker of "Replay line": a filled cone and two stroked waves.
pub const SPEAKER: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 24 24" fill="#FFFFFF"><path d="M4 9h4l5-4v14l-5-4H4z"></path><path d="M16 8.5a5 5 0 0 1 0 7M18.5 6a8.5 8.5 0 0 1 0 12" fill="none" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"></path></svg>"##,
);

/// The microphone of "Mute microphone".
pub const MICROPHONE: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 24 24" fill="none" stroke="#FFFFFF" stroke-width="2" stroke-linecap="round"><rect x="9" y="3" width="6" height="11" rx="3"></rect><path d="M5 11a7 7 0 0 0 14 0M12 18v3"></path></svg>"##,
);

/// The tick after a line that was spoken well. The design writes the
/// character `✓`, which Chrome drew with a system font (design/README.md)
/// and which looks unlike Outfit's; this is the system glyph's shape, two
/// square-ended strokes, measured from the reference.
pub const TICK: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 20 20" fill="none" stroke="#4CAF50" stroke-width="2.3"><path d="M3.25 10.15L9 17.4L18 2.9"></path></svg>"##,
);

/// The arrow after "tip". The design writes the character `→`, which
/// Chrome drew with a system font (Caveat has none): a shaft and a solid
/// head, measured from the reference.
pub const TIP_ARROW: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 20 20" fill="#F6C445"><path d="M0 8.6h13.4v-4.4l6.2 5.8-6.2 5.8v-4.4H0z"></path></svg>"##,
);

/// The raised, underlined "o" of `№`. Outfit has no such character and
/// Chrome drew it with a system font, whose "N" is close to Outfit's; so
/// the "N" is text and this is the rest, measured from the reference.
pub const ORDINAL: VectorIcon = VectorIcon::svg(
    r##"<svg viewBox="0 0 8 8" fill="none" stroke="#C7C7CC" stroke-width="1.4"><circle cx="4.1" cy="2.0" r="2.15"></circle><rect x="1.5" y="6.5" width="5" height="1.5" fill="#C7C7CC" stroke="none"></rect></svg>"##,
);

/// Every icon with its name, for the test below.
pub const ALL: [(&str, VectorIcon); 16] = [
    ("back", BACK),
    ("close", CLOSE),
    ("check", CHECK),
    ("chevron", CHEVRON),
    ("bell", BELL),
    ("handset", HANDSET),
    ("gear", GEAR),
    ("tab home", TAB_HOME),
    ("tab progress", TAB_PROGRESS),
    ("tab history", TAB_HISTORY),
    ("tab more", TAB_MORE),
    ("speaker", SPEAKER),
    ("microphone", MICROPHONE),
    ("tick", TICK),
    ("tip arrow", TIP_ARROW),
    ("ordinal", ORDINAL),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_in_the_supported_subset() {
        for (name, icon) in ALL {
            assert_eq!(icon.check(), Ok(()), "{name}");
        }
    }
}

//! FundiDesk's icons, one constant per drawing, taken from the inline
//! `<svg>` elements of `design/html/*.html`.
//!
//! Each is the design's markup with its `width`/`height` dropped (the size
//! is given where the icon is used) and its colour written as
//! `currentColor` (the colour is given where the icon is used). The
//! `stroke-width` kept here is the one the design uses most for that icon;
//! where a screen draws the same icon heavier or lighter, it says so with
//! `.stroke_width(..)`, and the doc comment lists the widths the design
//! uses.
//!
//! `ALL` lists every icon; a test parses each of them.
//!
//! **Frozen for the parallel phase.** See `SCREENS.md`.

use morningstar::ui::VectorIcon;

macro_rules! outline {
    ($width:literal, $($shape:literal),+ $(,)?) => {
        VectorIcon::svg(concat!(
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width=""#,
            $width,
            r#"" stroke-linecap="round" stroke-linejoin="round">"#,
            $($shape,)+
            "</svg>"
        ))
    };
}

/// The FundiDesk mark without its rounded plate: a phone outline with a
/// tick, in an 88-unit box. Draw it over a plate of the same size with
/// radius `24/88` of the side ([`components::logo`](crate::components::logo)
/// does). Stroke 6 here; the splash screen uses 5.
pub const LOGO_MARK: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 88 88" fill="none" stroke="currentColor" stroke-width="6">
<rect x="29" y="18" width="30" height="52" rx="7"/>
<path d="M37 45l5 5 10-11" stroke-linecap="round" stroke-linejoin="round"/>
</svg>"#,
);

/// A tick. Stroke 2.4 ("Synced"); 3 in the selected payment method; 3.2 in
/// the timeline's done dots.
pub const CHECK: VectorIcon = outline!("2.4", r#"<path d="M5 12l5 5 9-10"/>"#);

/// Back: a chevron pointing left. Stroke 2.
pub const CHEVRON_LEFT: VectorIcon = outline!("2", r#"<path d="M15 5l-7 7 7 7"/>"#);

/// Next: an arrow pointing right. Stroke 2.2.
pub const ARROW_RIGHT: VectorIcon = outline!(
    "2.2",
    r#"<path d="M5 12h14"/>"#,
    r#"<path d="M13 6l6 6-6 6"/>"#
);

/// Add: a plus with round caps. Stroke 2.4.
pub const PLUS: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M12 5v14"/><path d="M5 12h14"/></svg>"#,
);

/// Search: a magnifying glass. Stroke 2.
pub const SEARCH: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="M16.5 16.5L21 21"/></svg>"#,
);

/// A phone: the phone-number field.
pub const PHONE: VectorIcon = outline!(
    "1.8",
    r#"<rect x="6" y="2" width="12" height="20" rx="3"/>"#,
    r#"<path d="M11 18h2"/>"#
);

/// A phone with a tick: "ready, not collected". Stroke 1.9.
pub const PHONE_CHECK: VectorIcon = outline!(
    "1.9",
    r#"<rect x="6" y="2" width="12" height="20" rx="3"/>"#,
    r#"<path d="M9.5 12l2 2 3.5-4"/>"#
);

/// A padlock: the password field.
pub const LOCK: VectorIcon = outline!(
    "1.8",
    r#"<rect x="4" y="10" width="16" height="11" rx="3"/>"#,
    r#"<path d="M8 10V7a4 4 0 0 1 8 0v3"/>"#
);

/// An eye: show password.
pub const EYE: VectorIcon = outline!(
    "1.8",
    r#"<path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/>"#,
    r#"<circle cx="12" cy="12" r="3"/>"#
);

/// A fingerprint. Stroke 1.6.
pub const FINGERPRINT: VectorIcon = outline!(
    "1.6",
    r#"<path d="M12 11v3a8 8 0 0 1-1.2 4.2"/>"#,
    r#"<path d="M8.5 20a11 11 0 0 0 .5-3.5V11a3 3 0 0 1 6 0v2.5a12 12 0 0 1-1.3 5.5"/>"#,
    r#"<path d="M5.5 17.5A8 8 0 0 0 6 14.5V11a6 6 0 0 1 12 0v3"/>"#,
    r#"<path d="M3.5 9.5A9 9 0 0 1 12 3a9 9 0 0 1 6 2.3"/>"#
);

/// A shop front: shop setup.
pub const SHOP: VectorIcon = outline!(
    "1.8",
    r#"<path d="M4 10v10h16V10"/>"#,
    r#"<path d="M3 10l2-6h14l2 6a3 3 0 0 1-6 0 3 3 0 0 1-6 0 3 3 0 0 1-6 0z"/>"#,
    r#"<path d="M10 20v-5h4v5"/>"#
);

/// A clock: delayed repairs. Stroke 1.9.
pub const CLOCK: VectorIcon = outline!(
    "1.9",
    r#"<circle cx="12" cy="12" r="9"/>"#,
    r#"<path d="M12 7v5l3 2"/>"#
);

/// A speech bubble with a line: awaiting the customer's approval. Stroke
/// 1.9.
pub const MESSAGE: VectorIcon = outline!(
    "1.9",
    r#"<path d="M4 5h16v11H9l-5 4z"/>"#,
    r#"<path d="M9 10.5h6"/>"#
);

/// A box: waiting for parts. Stroke 1.9.
pub const PACKAGE: VectorIcon = outline!(
    "1.9",
    r#"<path d="M3 8l9-5 9 5v8l-9 5-9-5z"/>"#,
    r#"<path d="M3 8l9 5 9-5"/>"#,
    r#"<path d="M12 13v8"/>"#
);

/// A round chat bubble: WhatsApp actions.
pub const CHAT: VectorIcon = outline!("1.8", r#"<path d="M4 20l1.3-4A8 8 0 1 1 8 18.7z"/>"#);

/// A telephone handset: call actions.
pub const CALL: VectorIcon = outline!(
    "1.8",
    r#"<path d="M5 4h4l2 5-2.5 1.5a11 11 0 0 0 5 5L15 13l5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2z"/>"#
);

/// Share: three joined dots.
pub const SHARE: VectorIcon = outline!(
    "1.8",
    r#"<circle cx="6" cy="12" r="2.5"/>"#,
    r#"<circle cx="18" cy="6" r="2.5"/>"#,
    r#"<circle cx="18" cy="18" r="2.5"/>"#,
    r#"<path d="M8.2 10.9l7.6-3.8"/>"#,
    r#"<path d="M8.2 13.1l7.6 3.8"/>"#
);

/// A calendar: ready date.
pub const CALENDAR: VectorIcon = outline!(
    "1.8",
    r#"<rect x="3" y="5" width="18" height="16" rx="3"/>"#,
    r#"<path d="M3 10h18"/>"#,
    r#"<path d="M8 3v4"/>"#,
    r#"<path d="M16 3v4"/>"#
);

/// A camera: add photos.
pub const CAMERA: VectorIcon = outline!(
    "1.8",
    r#"<path d="M3 8h4l2-3h6l2 3h4v11H3z"/>"#,
    r#"<circle cx="12" cy="13" r="3.5"/>"#
);

/// Bars of a barcode: add IMEI.
pub const BARCODE: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><path d="M4 6v12"/><path d="M8 6v12"/><path d="M11 6v12"/><path d="M15 6v12"/><path d="M20 6v12"/></svg>"#,
);

/// An arrow into a tray: saved on this phone. Stroke 2.
pub const DOWNLOAD: VectorIcon = outline!(
    "2",
    r#"<path d="M12 3v12"/>"#,
    r#"<path d="M7 10l5 5 5-5"/>"#,
    r#"<path d="M4 20h16"/>"#
);

/// A spanner: the Repairs tab (stroke 1.8) and the onboarding tile (2).
pub const WRENCH: VectorIcon = outline!(
    "1.8",
    r#"<path d="M14.7 6.3a4 4 0 0 0-5.2 5.2L3 18v3h3l6.5-6.5a4 4 0 0 0 5.2-5.2l-2.6 2.6-2.4-.6-.6-2.4z"/>"#
);

/// The spanner filled in: the Repairs tab when selected.
pub const WRENCH_FILLED: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a4 4 0 0 0-5.2 5.2L3 18v3h3l6.5-6.5a4 4 0 0 0 5.2-5.2l-2.6 2.6-2.4-.6-.6-2.4z"/></svg>"#,
);

/// A house: the Home tab.
pub const HOME: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M4 11l8-7 8 7v9h-5v-6h-6v6H4z"/></svg>"#,
);

/// The house filled in: the Home tab when selected.
pub const HOME_FILLED: VectorIcon = VectorIcon::svg(
    r#"<svg viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M4 11l8-7 8 7v9h-5v-6h-6v6H4z"/></svg>"#,
);

/// Two people: the Customers tab.
pub const PEOPLE: VectorIcon = outline!(
    "1.8",
    r#"<circle cx="9" cy="8" r="3.5"/>"#,
    r#"<path d="M2.5 20a6.5 6.5 0 0 1 13 0"/>"#,
    r#"<path d="M16 4.8a3.5 3.5 0 0 1 0 6.4"/>"#,
    r#"<path d="M18 14.5a6.5 6.5 0 0 1 3.5 5.5"/>"#
);

/// One person: the Account tab.
pub const PERSON: VectorIcon = outline!(
    "1.8",
    r#"<circle cx="12" cy="8" r="4"/>"#,
    r#"<path d="M4 21a8 8 0 0 1 16 0"/>"#
);

/// Every icon with its name, for tests and for an icon sheet.
pub const ALL: &[(&str, VectorIcon)] = &[
    ("LOGO_MARK", LOGO_MARK),
    ("CHECK", CHECK),
    ("CHEVRON_LEFT", CHEVRON_LEFT),
    ("ARROW_RIGHT", ARROW_RIGHT),
    ("PLUS", PLUS),
    ("SEARCH", SEARCH),
    ("PHONE", PHONE),
    ("PHONE_CHECK", PHONE_CHECK),
    ("LOCK", LOCK),
    ("EYE", EYE),
    ("FINGERPRINT", FINGERPRINT),
    ("SHOP", SHOP),
    ("CLOCK", CLOCK),
    ("MESSAGE", MESSAGE),
    ("PACKAGE", PACKAGE),
    ("CHAT", CHAT),
    ("CALL", CALL),
    ("SHARE", SHARE),
    ("CALENDAR", CALENDAR),
    ("CAMERA", CAMERA),
    ("BARCODE", BARCODE),
    ("DOWNLOAD", DOWNLOAD),
    ("WRENCH", WRENCH),
    ("WRENCH_FILLED", WRENCH_FILLED),
    ("HOME", HOME),
    ("HOME_FILLED", HOME_FILLED),
    ("PEOPLE", PEOPLE),
    ("PERSON", PERSON),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_within_the_supported_svg_subset() {
        for (name, icon) in ALL {
            if let Err(error) = icon.check() {
                panic!("{name}: {error}");
            }
            assert_eq!(icon.aspect_ratio(), Some(1.0), "{name}");
        }
    }

    #[test]
    fn icons_are_distinct() {
        for (index, (name, icon)) in ALL.iter().enumerate() {
            for (other_name, other) in &ALL[index + 1..] {
                assert_ne!(icon, other, "{name} and {other_name}");
            }
        }
    }
}

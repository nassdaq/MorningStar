//! The icons of the app: the design's own inline SVGs, one file per
//! distinct drawing in `assets/svg`, loaded with the SVG loader (ADR 0027).
//!
//! `design/extract-icons.py` wrote the files from `design/html/*.html`:
//! the markup as the design has it, with `sc-camel-view-box` spelt
//! `viewBox`, the namespace added and the colour made `currentColor`, so an
//! icon takes its colour where it is drawn (`Icon::svg(X.get()).color(..)`).
//! The same outline at two stroke widths is two files, as the loader draws
//! a file as written. The last five are the app's own, drawn in the
//! design's manner for controls the design does not have (see their notes).

use morningstar::ui::prelude::*;

/// "Add someone to the call".
pub static ADD_PERSON: SvgAsset = SvgAsset::new(include_str!("../assets/svg/add-person.svg"));
/// The paperclip of the composer.
pub static ATTACH: SvgAsset = SvgAsset::new(include_str!("../assets/svg/attach.svg"));
/// The back arrow (22 dp, `stroke-width="2"`).
pub static BACK: SvgAsset = SvgAsset::new(include_str!("../assets/svg/back.svg"));
/// The dial pad's delete key. The app's own.
pub static BACKSPACE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/backspace.svg"));
/// The Notifications tab.
pub static BELL: SvgAsset = SvgAsset::new(include_str!("../assets/svg/bell.svg"));
/// The camera with a stroke through it: the camera is off. The app's own (the design's camera and a line).
pub static CAMERA_OFF: SvgAsset = SvgAsset::new(include_str!("../assets/svg/camera-off.svg"));
/// A tick: "sent" under a message, the chosen row. The app's own.
pub static CHECK: SvgAsset = SvgAsset::new(include_str!("../assets/svg/check.svg"));
/// The cross that closes a sheet or a search. The app's own.
pub static CLOSE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/close.svg"));
/// The Contacts tab.
pub static CONTACTS: SvgAsset = SvgAsset::new(include_str!("../assets/svg/contacts.svg"));
/// "Dial a phone number": four squares.
pub static DIAL: SvgAsset = SvgAsset::new(include_str!("../assets/svg/dial.svg"));
/// The handset of decline and hang up.
pub static HANG_UP: SvgAsset = SvgAsset::new(include_str!("../assets/svg/hang-up.svg"));
/// The Home tab, filled.
pub static HOME: SvgAsset = SvgAsset::new(include_str!("../assets/svg/home.svg"));
/// "Open keypad": nine dots.
pub static KEYPAD: SvgAsset = SvgAsset::new(include_str!("../assets/svg/keypad.svg"));
/// "Reply with a message instead".
pub static MESSAGE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/message.svg"));
/// The microphone of mute.
pub static MIC: SvgAsset = SvgAsset::new(include_str!("../assets/svg/mic.svg"));
/// The microphone with a stroke through it: muted. The app's own (the design's microphone and a line).
pub static MIC_OFF: SvgAsset = SvgAsset::new(include_str!("../assets/svg/mic-off.svg"));
/// The handset of the Calls tab and of a room's header.
pub static PHONE: SvgAsset = SvgAsset::new(include_str!("../assets/svg/phone.svg"));
/// The handset beside "Phone call" (`stroke-width="2"`, drawn at 14 dp).
pub static PHONE_SMALL: SvgAsset = SvgAsset::new(include_str!("../assets/svg/phone-small.svg"));
/// The triangle of a voice message.
pub static PLAY: SvgAsset = SvgAsset::new(include_str!("../assets/svg/play.svg"));
/// The plus of "New room".
pub static PLUS: SvgAsset = SvgAsset::new(include_str!("../assets/svg/plus.svg"));
/// The plus of "Start a call or room" (`stroke-width="2.4"`).
pub static PLUS_BOLD: SvgAsset = SvgAsset::new(include_str!("../assets/svg/plus-bold.svg"));
/// The ring of the record button; the app draws the dot inside it.
pub static RECORD: SvgAsset = SvgAsset::new(include_str!("../assets/svg/record.svg"));
/// The magnifier.
pub static SEARCH: SvgAsset = SvgAsset::new(include_str!("../assets/svg/search.svg"));
/// The eye before "Seen by 3".
pub static SEEN: SvgAsset = SvgAsset::new(include_str!("../assets/svg/seen.svg"));
/// The paper plane of the send button, filled.
pub static SEND: SvgAsset = SvgAsset::new(include_str!("../assets/svg/send.svg"));
/// The two boxes before a notice from the server.
pub static SERVER: SvgAsset = SvgAsset::new(include_str!("../assets/svg/server.svg"));
/// "Share your screen".
pub static SHARE_SCREEN: SvgAsset = SvgAsset::new(include_str!("../assets/svg/share-screen.svg"));
/// The loudspeaker.
pub static SPEAKER: SvgAsset = SvgAsset::new(include_str!("../assets/svg/speaker.svg"));
/// The camera of a video call.
pub static VIDEO: SvgAsset = SvgAsset::new(include_str!("../assets/svg/video.svg"));
/// The camera beside "Video call" (`stroke-width="2"`, drawn at 14 dp).
pub static VIDEO_SMALL: SvgAsset = SvgAsset::new(include_str!("../assets/svg/video-small.svg"));

/// Every icon, with its file's name: for the test that each loads.
pub static ALL: [(&str, &SvgAsset); 30] = [
    ("add-person", &ADD_PERSON),
    ("attach", &ATTACH),
    ("back", &BACK),
    ("backspace", &BACKSPACE),
    ("bell", &BELL),
    ("camera-off", &CAMERA_OFF),
    ("check", &CHECK),
    ("close", &CLOSE),
    ("contacts", &CONTACTS),
    ("dial", &DIAL),
    ("hang-up", &HANG_UP),
    ("home", &HOME),
    ("keypad", &KEYPAD),
    ("message", &MESSAGE),
    ("mic", &MIC),
    ("mic-off", &MIC_OFF),
    ("phone", &PHONE),
    ("phone-small", &PHONE_SMALL),
    ("play", &PLAY),
    ("plus", &PLUS),
    ("plus-bold", &PLUS_BOLD),
    ("record", &RECORD),
    ("search", &SEARCH),
    ("seen", &SEEN),
    ("send", &SEND),
    ("server", &SERVER),
    ("share-screen", &SHARE_SCREEN),
    ("speaker", &SPEAKER),
    ("video", &VIDEO),
    ("video-small", &VIDEO_SMALL),
];

/// An icon `size` dp square in `color`.
pub fn icon(asset: &'static SvgAsset, size: f32, color: Color) -> Icon {
    Icon::svg(asset.get()).size(size).color(color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_loads() {
        for (name, asset) in ALL {
            assert!(asset.check().is_ok(), "{name}: {:?}", asset.check());
        }
    }
}

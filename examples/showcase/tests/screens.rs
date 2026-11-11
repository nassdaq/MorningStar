//! The showcase, driven and rendered the way a device would.

use std::time::Duration;

use morningstar::ui::Ui;
use morningstar::{
    App, DrawOp, Frame, FrameInfo, Insets, Point, PointerEvent, PointerPhase, Rect, SurfaceSize,
};
use morningstar_devtools::{Image, Tolerance, assert_matches_reference, screenshot_frame};
use showcase::{Language, Message, Screen, Showcase};

/// 360 x 800 dp with a 32 dp status bar and a 20 dp gesture bar.
fn phone(scale: f32) -> FrameInfo {
    let size = SurfaceSize::new((360.0 * scale) as u32, (800.0 * scale) as u32);
    FrameInfo::new(size)
        .with_scale(scale)
        .with_insets(Insets::new(0.0, 32.0 * scale, 0.0, 20.0 * scale))
}

struct Device {
    ui: Ui<Showcase>,
    frame: Frame,
    info: FrameInfo,
    time: f32,
}

impl Device {
    fn new(screen: Screen) -> Self {
        let mut ui = Ui::new(Showcase::on(screen));
        let frame = Frame::for_app(&mut ui);
        let mut device = Self {
            ui,
            frame,
            info: phone(3.0),
            time: 0.0,
        };
        device.draw();
        device
    }

    fn draw(&mut self) -> bool {
        self.frame.begin();
        let info = self.info.with_elapsed(Duration::from_secs_f32(self.time));
        self.ui.frame(&info, &mut self.frame);
        self.frame.redraw_requested()
    }

    fn touch(&mut self, phase: PointerPhase, position: Point) {
        self.ui.pointer(&PointerEvent {
            id: 1,
            phase,
            position,
        });
    }

    fn tap(&mut self, rect: Rect) {
        self.touch(PointerPhase::Down, rect.center());
        self.draw();
        self.touch(PointerPhase::Up, rect.center());
        self.draw();
    }

    fn rect(&self, id: impl std::hash::Hash) -> Rect {
        self.ui.rect_of(id).expect("widget is on screen")
    }

    fn app(&self) -> &Showcase {
        self.ui.app()
    }
}

#[test]
fn tabs_and_buttons_navigate_between_the_three_screens() {
    let mut device = Device::new(Screen::Home);
    device.tap(device.rect(("tab", "Mipangilio")));
    assert_eq!(device.app().screen, Screen::Settings);

    // "Sign out" is below the fold: scroll to the end of the settings.
    let x = 540.0;
    for _ in 0..3 {
        device.touch(PointerPhase::Down, Point::new(x, 1900.0));
        device.touch(PointerPhase::Moved, Point::new(x, 1850.0));
        device.touch(PointerPhase::Moved, Point::new(x, 600.0));
        device.touch(PointerPhase::Cancelled, Point::new(x, 600.0));
        device.draw();
    }
    assert_eq!(
        device.ui.scroll_offset("settings-scroll"),
        device.ui.max_scroll_offset("settings-scroll")
    );
    device.tap(device.rect("sign-out"));
    assert_eq!(device.app().screen, Screen::Login);

    device.tap(device.rect("sign-in"));
    assert_eq!(device.app().screen, Screen::Home);
    // The home screen starts at the top again.
    assert_eq!(device.ui.scroll_offset("home-scroll"), Some(0.0));
}

#[test]
fn widgets_change_the_state_they_say_they_change() {
    let mut device = Device::new(Screen::Home);
    assert!(!device.app().balance_hidden);
    device.tap(device.rect("toggle-balance"));
    assert!(device.app().balance_hidden);

    device.tap(device.rect("language"));
    assert_eq!(device.app().language, Language::English);
    // The tab labels are in English now, and still work.
    device.tap(device.rect(("tab", "Settings")));
    assert_eq!(device.app().screen, Screen::Settings);

    assert!(device.app().notifications);
    device.tap(device.rect("notifications-row"));
    assert!(!device.app().notifications);
    device.tap(device.rect("biometrics-row"));
    assert!(device.app().biometrics);
    device.tap(device.rect("language-row"));
    assert_eq!(device.app().language, Language::Swahili);
}

#[test]
fn the_transaction_list_scrolls_and_rows_stay_tappable() {
    let mut device = Device::new(Screen::Home);
    assert!(device.ui.rect_of(("transaction", 9_usize)).is_none());
    let travel = device.ui.max_scroll_offset("home-scroll").unwrap();
    assert!(travel > 300.0, "ten rows do not fit: {travel}");

    // A drag that starts on a row scrolls and does not select it.
    let start = device.rect(("transaction", 1_usize)).center();
    device.touch(PointerPhase::Down, start);
    device.draw();
    device.touch(PointerPhase::Moved, Point::new(start.x, start.y - 60.0));
    device.touch(PointerPhase::Moved, Point::new(start.x, start.y - 960.0));
    device.draw();
    device.time += 1.0;
    device.touch(PointerPhase::Up, Point::new(start.x, start.y - 960.0));
    assert!(!device.draw(), "released at rest: no fling, no more frames");
    assert_eq!(device.ui.scroll_offset("home-scroll"), Some(300.0));
    assert_eq!(device.app().selected_transaction, None);

    device.tap(device.rect(("transaction", 5_usize)));
    assert_eq!(device.app().selected_transaction, Some(5));
}

#[test]
fn every_screen_fits_and_stays_inside_the_safe_area() {
    for screen in [Screen::Home, Screen::Settings, Screen::Login] {
        for language in [Language::Swahili, Language::English] {
            for scale in [1.0, 1.5, 2.625, 3.0] {
                let mut device = Device::new(screen);
                device.info = phone(scale);
                if language == Language::English {
                    device.ui.send(Message::ToggleLanguage);
                }
                assert!(!device.draw(), "{screen:?} is idle");
                assert_eq!(
                    device.ui.overflows(),
                    [],
                    "{screen:?} in {language:?} at scale {scale}"
                );
                // Nothing is drawn as text above the status bar's lower
                // edge or below the gesture bar's upper edge.
                let safe = device.info.safe_area();
                for op in device.frame.list.ops() {
                    if let DrawOp::Glyph { rect, .. } = op {
                        assert!(
                            rect.y >= safe.y && rect.y + rect.height <= safe.y + safe.height,
                            "{screen:?} at {scale}: glyph at {rect:?} outside {safe:?}"
                        );
                    }
                }
            }
        }
    }
}

fn close(pixel: [u8; 4], expected: [u8; 3], tolerance: i16) -> bool {
    (0..3).all(|channel| {
        (i16::from(pixel[channel]) - i16::from(expected[channel])).abs() <= tolerance
    })
}

#[test]
fn the_screens_render_on_the_gpu() {
    // Half-size phone: 540 x 1200 at 1.5 px per dp.
    let info = phone(1.5);

    let mut ui = Ui::new(Showcase::on(Screen::Home));
    let png = screenshot_frame(&mut ui, &info).expect("offscreen GPU");
    let image = Image::from_png(&png).unwrap();
    assert_eq!((image.width(), image.height()), (540, 1200));
    // Background at a corner (under the status bar: edge to edge).
    assert!(close(image.pixel(3, 3), [0xF4, 0xF6, 0xFA], 2));
    // The balance card is a blue-to-violet gradient.
    let card = ui.rect_of("balance-card").unwrap();
    let left = image.pixel(card.x as u32 + 12, (card.y + card.height * 0.6) as u32);
    let right = image.pixel(
        (card.x + card.width) as u32 - 12,
        (card.y + card.height * 0.6) as u32,
    );
    for pixel in [left, right] {
        assert!(
            pixel[2] > 200 && pixel[2] > pixel[0] + 60 && pixel[1] < 120,
            "{pixel:?}"
        );
    }
    assert!(
        right[0] > left[0] + 30,
        "more violet on the right: {left:?} {right:?}"
    );
    // The balance is white text on the card.
    let balance = ui.rect_of("balance").unwrap();
    let white = (balance.x as u32..(balance.x + balance.width) as u32)
        .filter(|x| {
            close(
                image.pixel(*x, balance.center().y as u32),
                [255, 255, 255],
                6,
            )
        })
        .count();
    assert!(white > 20, "{white} white pixels across the balance");
    // The profile picture is an image: warm at the top, a dark figure in
    // the middle.
    let avatar = ui.rect_of("avatar").unwrap();
    let top = image.pixel(avatar.center().x as u32, avatar.y as u32 + 4);
    let middle = image.pixel(
        avatar.center().x as u32,
        (avatar.y + avatar.height * 0.4) as u32,
    );
    assert!(top[0] > 230 && top[2] < 190, "{top:?}");
    assert!(middle[0] < 110 && middle[1] < 80, "{middle:?}");
    // The tab bar is a white surface down to the bottom edge.
    assert!(close(image.pixel(270, 1197), [255, 255, 255], 1));

    let mut ui = Ui::new(Showcase::on(Screen::Login));
    let image = Image::from_png(&screenshot_frame(&mut ui, &info).unwrap()).unwrap();
    assert!(close(image.pixel(3, 3), [255, 255, 255], 1));
    let button = ui.rect_of("sign-in").unwrap();
    let pixel = image.pixel(button.x as u32 + 20, button.center().y as u32);
    assert!(close(pixel, [0x2B, 0x3F, 0xE0], 3), "{pixel:?}");
    // The Arabic line draws glyphs (it has a font and is shaped).
    let arabic = ui.rect_of("arabic").unwrap();
    let inked = (arabic.x as u32..(arabic.x + arabic.width) as u32)
        .filter(|x| {
            (arabic.y as u32..(arabic.y + arabic.height) as u32)
                .any(|y| image.pixel(*x, y)[0] < 160)
        })
        .count();
    assert!(inked > 60, "{inked} inked columns in the Arabic line");
}

/// Visual regression against small stored references (360 x 800, one pixel
/// per dp). Regenerate with `MORNINGSTAR_UPDATE_REFERENCES=1 cargo test -p
/// showcase` and look at the PNGs before committing them.
#[test]
fn the_screens_match_their_references() {
    for (screen, name) in [
        (Screen::Home, "home"),
        (Screen::Settings, "settings"),
        (Screen::Login, "login"),
    ] {
        let mut ui = Ui::new(Showcase::on(screen));
        let png = screenshot_frame(&mut ui, &phone(1.0)).expect("offscreen GPU");
        assert_matches_reference(
            &png,
            format!("tests/references/{name}.png"),
            Tolerance::DEFAULT,
        );
    }
}

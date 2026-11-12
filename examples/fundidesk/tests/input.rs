//! Typing into FundiDesk's forms, driven the way a device drives them:
//! taps in pixels on the fields as laid out, text events from the
//! keyboard, a frame after each.

use std::hash::Hash;

use fundidesk::screens::new_repair::TextField;
use fundidesk::{DESIGN_HEIGHT, DESIGN_WIDTH, FundiDesk, Screen};
use morningstar::ui::Ui;
use morningstar::{
    App, Frame, FrameInfo, ImeAction, KeyboardType, Point, PointerEvent, PointerPhase, Rect,
    SurfaceSize, TextEditState, TextInputEvent,
};

const SCALE: f32 = 2.0;

struct Device {
    ui: Ui<FundiDesk>,
    frame: Frame,
    info: FrameInfo,
}

impl Device {
    fn on(screen: Screen) -> Self {
        let mut ui = Ui::new(FundiDesk::on(screen));
        let frame = Frame::for_app(&mut ui);
        let size = SurfaceSize::new(
            (DESIGN_WIDTH * SCALE) as u32,
            (DESIGN_HEIGHT * SCALE) as u32,
        );
        let mut device = Self {
            ui,
            frame,
            info: FrameInfo::new(size).with_scale(SCALE),
        };
        device.draw();
        device
    }

    fn draw(&mut self) {
        self.frame.begin();
        self.ui.frame(&self.info, &mut self.frame);
    }

    fn rect(&self, id: impl Hash) -> Rect {
        self.ui.rect_of(id).expect("the widget is on screen")
    }

    fn tap(&mut self, id: impl Hash) {
        let rect = self.rect(id);
        let position = Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            self.ui.pointer(&PointerEvent {
                id: 1,
                phase,
                position,
            });
            self.draw();
        }
    }

    fn key(&mut self, event: TextInputEvent) {
        self.ui.text_input(&event);
        self.draw();
    }

    /// One key event per character, as `adb shell input text` and a
    /// hardware keyboard deliver them.
    fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            self.key(TextInputEvent::Commit(character.to_string()));
        }
    }

    fn app(&self) -> &FundiDesk {
        self.ui.app()
    }
}

#[test]
fn log_in_by_typing_a_phone_number_and_a_password() {
    let mut device = Device::on(Screen::Login);
    assert!(device.frame.text_input().is_none(), "no keyboard at rest");

    device.tap("login-phone");
    assert!(device.ui.is_focused("login-phone"));
    let request = device.frame.text_input().unwrap();
    assert_eq!(request.traits.keyboard, KeyboardType::Phone);
    assert_eq!(request.traits.action, ImeAction::Next);
    device.type_text("0712345678");
    assert_eq!(device.app().login().phone, "0712345678");

    // The keyboard's Next key goes to the password.
    device.key(TextInputEvent::Action(ImeAction::Next));
    assert!(device.ui.is_focused("login-password"));
    assert!(device.frame.text_input().unwrap().traits.secure);
    device.type_text("fundi2026");
    assert_eq!(device.app().login().password, "fundi2026");

    // Its action key is the "Log in" button.
    device.key(TextInputEvent::Action(ImeAction::Done));
    assert!(!device.ui.has_focus());
    assert_eq!(device.app().screen(), Screen::Dashboard);
}

#[test]
fn the_eye_shows_and_masks_the_password_without_closing_the_keyboard() {
    let mut device = Device::on(Screen::Login);
    device.tap("login-password");
    device.type_text("siri");
    assert!(!device.app().login().password_visible);
    assert!(device.frame.text_input().unwrap().traits.secure);
    let masked = device.frame.list.ops().to_vec();

    device.tap("login-eye");
    assert!(device.app().login().password_visible);
    assert!(device.ui.is_focused("login-password"), "still editing");
    let request = device.frame.text_input().unwrap();
    assert!(!request.traits.secure);
    assert_eq!(request.text, "siri");
    assert_ne!(
        device.frame.list.ops(),
        masked,
        "letters instead of bullets"
    );

    device.tap("login-eye");
    assert!(device.frame.text_input().unwrap().traits.secure);
    assert_eq!(device.frame.list.ops(), masked);
}

#[test]
fn the_six_code_boxes_fill_one_after_another() {
    let mut device = Device::on(Screen::Verification);
    assert_eq!(device.app().verification().code(), "47");

    // The third box is the one waiting, as the design shows.
    device.tap(("code", 2usize));
    assert!(device.ui.is_focused(("code", 2usize)));
    assert_eq!(
        device.frame.text_input().unwrap().traits.keyboard,
        KeyboardType::Number
    );
    // Letters are refused; each digit moves on to the next box.
    device.type_text("1a2b34");
    assert_eq!(device.app().verification().code(), "471234");
    assert!(device.ui.is_focused(("code", 5usize)), "the last box");

    // Backspace clears the last box, then walks back through the code.
    device.key(TextInputEvent::DeleteBackward);
    assert_eq!(device.app().verification().code(), "47123");
    device.key(TextInputEvent::DeleteBackward);
    assert_eq!(device.app().verification().code(), "4712");
    assert!(device.ui.is_focused(("code", 4usize)));
    device.key(TextInputEvent::DeleteBackward);
    assert_eq!(device.app().verification().code(), "471");
    assert!(device.ui.is_focused(("code", 3usize)));

    // A soft keyboard edits its own copy and reports the result.
    device.key(TextInputEvent::Edited(TextEditState {
        text: "9".to_owned(),
        caret: 1,
        anchor: None,
        composing: None,
    }));
    assert_eq!(device.app().verification().code(), "4719");
    assert!(device.ui.is_focused(("code", 4usize)));
}

#[test]
fn a_new_repair_is_filled_in_and_kept_clear_of_the_keyboard() {
    let mut device = Device::on(Screen::NewRepair);
    assert_eq!(device.app().new_repair().customer, "Amina Juma");

    // The customer's name: delete two letters, type others.
    device.tap(("repair", TextField::Customer));
    device.key(TextInputEvent::Edited(TextEditState {
        text: "Amina Juma".to_owned(),
        caret: 10,
        anchor: None,
        composing: None,
    }));
    device.key(TextInputEvent::DeleteBackward);
    device.key(TextInputEvent::DeleteBackward);
    device.type_text("ma");
    assert_eq!(device.app().new_repair().customer, "Amina Juma");
    device.type_text("nne");
    assert_eq!(device.app().new_repair().customer, "Amina Jumanne");

    // Next, Next, Next: brand, model, then the problem, which is further
    // down the form than the phone is tall.
    for _ in 0..3 {
        device.key(TextInputEvent::Action(ImeAction::Next));
    }
    assert!(device.ui.is_focused(("repair", TextField::Problem)));
    // The keyboard comes up over the bottom 300 dp.
    device.info = device.info.with_keyboard_inset(300.0 * SCALE);
    device.draw();
    let keyboard_top = (DESIGN_HEIGHT - 300.0) * SCALE;
    let problem = device.rect(("repair", TextField::Problem));
    assert!(
        problem.y > 0.0 && problem.y + problem.height < keyboard_top,
        "{problem:?} above the keyboard at {keyboard_top}"
    );

    device.type_text("Kioo kimevunjika");
    device.key(TextInputEvent::Enter);
    device.type_text("Haiwaki");
    assert_eq!(
        device.app().new_repair().problem,
        "Kioo kimevunjika\nHaiwaki"
    );
    let grown = device.rect(("repair", TextField::Problem));
    assert!(grown.height > problem.height, "a second line");
    assert!(grown.y + grown.height < keyboard_top, "still in view");
}

#[test]
fn a_payment_amount_can_be_typed_or_chosen() {
    let mut device = Device::on(Screen::RecordPayment);
    assert_eq!(device.app().record_payment().amount_text, "40,000");

    device.tap("payment-amount");
    assert_eq!(
        device.frame.text_input().unwrap().traits.keyboard,
        KeyboardType::Number
    );
    for _ in 0..6 {
        device.key(TextInputEvent::DeleteBackward);
    }
    assert_eq!(device.app().record_payment().amount_text, "");
    // The application keeps digits and commas; the field shows what it
    // kept, and tells the keyboard so.
    device.type_text("15x,000");
    assert_eq!(device.app().record_payment().amount_text, "15,000");
    assert_eq!(device.frame.text_input().unwrap().text, "15,000");

    // A quick amount replaces what was typed (and ends the editing).
    device.tap(("amount", 1usize));
    assert_eq!(device.app().record_payment().amount_text, "20,000");
    assert!(!device.ui.has_focus());
}

#[test]
fn the_other_forms_keep_what_is_typed() {
    let mut device = Device::on(Screen::CreateAccount);
    device.tap("account-name");
    device.type_text("Asha");
    device.key(TextInputEvent::Action(ImeAction::Next));
    device.type_text("712345678");
    device.key(TextInputEvent::Action(ImeAction::Next));
    assert!(device.frame.text_input().unwrap().traits.secure);
    device.type_text("nenosiri");
    let account = device.app().create_account();
    assert_eq!(
        (
            account.name.as_str(),
            account.whatsapp.as_str(),
            account.password.as_str()
        ),
        ("Asha", "712345678", "nenosiri")
    );

    let mut device = Device::on(Screen::ShopSetup);
    device.tap("shop-name");
    device.type_text("Fundi Simu");
    device.key(TextInputEvent::Action(ImeAction::Next));
    device.type_text("Kariakoo");
    assert_eq!(device.app().shop_setup().name, "Fundi Simu");
    assert_eq!(device.app().shop_setup().location, "Kariakoo");

    let mut device = Device::on(Screen::RepairsList);
    device.tap("repairs-search");
    assert_eq!(
        device.frame.text_input().unwrap().traits.action,
        ImeAction::Search
    );
    device.type_text("Amina");
    assert_eq!(device.app().repairs_list().query, "Amina");
}

#[test]
fn a_field_under_the_keyboard_on_a_fixed_form_is_shifted_into_view() {
    let mut device = Device::on(Screen::CreateAccount);
    device.tap("account-confirmation");
    let before = device.rect("account-confirmation");
    device.info = device.info.with_keyboard_inset(340.0 * SCALE);
    device.draw();
    let keyboard_top = (DESIGN_HEIGHT - 340.0) * SCALE;
    assert!(before.y + before.height > keyboard_top, "it was covered");
    let after = device.rect("account-confirmation");
    assert!(after.y + after.height < keyboard_top, "{after:?}");
    assert!(after.y > 0.0);
}

/// Prints where to tap on a real device, from the same layout code the
/// device runs: `tools/adb-guarded.sh tap X Y` wants pixels, and guessing
/// them from a screenshot is how taps land in the wrong place.
///
/// ```sh
/// MORNINGSTAR_DEVICE="1080 2340 2.8125 96 48" \
///     cargo test -p fundidesk --test input device_tap_targets -- --ignored --nocapture
/// ```
///
/// The five numbers are the surface's width and height in pixels, the
/// scale, and the top and bottom insets in pixels, all as logged by the
/// app (`adb logcat -s Morningstar`: `surface`, `scale`, `insets`); a
/// sixth, optional, is the keyboard inset.
#[test]
#[ignore = "a tool: needs MORNINGSTAR_DEVICE"]
fn device_tap_targets() {
    use morningstar::Insets;
    let spec = std::env::var("MORNINGSTAR_DEVICE").expect("MORNINGSTAR_DEVICE");
    let numbers: Vec<f32> = spec
        .split_whitespace()
        .map(|number| number.parse().expect("a number"))
        .collect();
    let [width, height, scale, top, bottom] = numbers[..5] else {
        panic!("five numbers: width height scale top bottom");
    };
    let keyboard = numbers.get(5).copied().unwrap_or(0.0);
    let info = FrameInfo::new(SurfaceSize::new(width as u32, height as u32))
        .with_scale(scale)
        .with_insets(Insets::new(0.0, top, 0.0, bottom))
        .with_keyboard_inset(keyboard);
    let targets: [(Screen, &[&str]); 3] = [
        (Screen::Splash, &["splash"]),
        (Screen::Onboarding, &["onboarding-continue"]),
        (
            Screen::Login,
            &["login-phone", "login-password", "login-eye"],
        ),
    ];
    for (screen, ids) in targets {
        let mut ui = Ui::new(FundiDesk::on(screen));
        let mut frame = Frame::for_app(&mut ui);
        frame.begin();
        ui.frame(&info, &mut frame);
        for id in ids {
            let rect = ui.rect_of(id).expect("the widget is on screen");
            println!(
                "{}: {id}: tap {:.0} {:.0}   (rect {:.0},{:.0} {:.0}x{:.0})",
                screen.name(),
                rect.x + rect.width / 2.0,
                rect.y + rect.height / 2.0,
                rect.x,
                rect.y,
                rect.width,
                rect.height
            );
        }
    }
}

//! The colour of the system's status and navigation icons over Atlas
//! (ADR 0026): dark, on every one of its light screens, and still dark
//! while the model picker dims the screen (the dimmed page is a mid grey
//! on which dark icons read better than light ones).

use atlas::{Atlas, Message, Screen};
use morningstar::ui::{SystemBars, Ui};
use morningstar::{App, Frame, FrameInfo, SurfaceSize};

fn bars(ui: &mut Ui<Atlas>) -> Option<SystemBars> {
    let mut frame = Frame::for_app(ui);
    frame.begin();
    ui.frame(
        &FrameInfo::new(SurfaceSize::new(1080, 2340)).with_scale(2.8125),
        &mut frame,
    );
    frame.system_bars()
}

#[test]
fn every_light_screen_gets_dark_icons() {
    for screen in Screen::ALL {
        let mut ui = Ui::new(Atlas::on(screen));
        assert_eq!(bars(&mut ui), Some(SystemBars::DARK_CONTENT), "{screen:?}");
    }
}

#[test]
fn the_dimmed_screen_under_the_picker_keeps_dark_icons() {
    let mut ui = Ui::new(Atlas::designed("03-home").unwrap());
    ui.send(Message::OpenPicker);
    assert!(ui.app().picker_open());
    assert_eq!(bars(&mut ui), Some(SystemBars::DARK_CONTENT));
    let mut designed = Ui::new(Atlas::designed("05-model-picker").unwrap());
    assert_eq!(bars(&mut designed), Some(SystemBars::DARK_CONTENT));
}

//! The colour of the system's status and navigation icons over Barua Talk
//! (ADR 0026): light, on every one of its dark screens, with a sheet open
//! and over the photographs of the two call screens.

use barua_talk::{BaruaTalk, Message, Screen};
use morningstar::ui::{SystemBars, Ui};
use morningstar::{App, Frame, FrameInfo, SurfaceSize};

fn bars(ui: &mut Ui<BaruaTalk>) -> Option<SystemBars> {
    let mut frame = Frame::for_app(ui);
    frame.begin();
    ui.frame(
        &FrameInfo::new(SurfaceSize::new(1080, 2340)).with_scale(2.8125),
        &mut frame,
    );
    frame.system_bars()
}

#[test]
fn every_dark_screen_gets_light_icons() {
    for screen in Screen::ALL {
        let mut ui = Ui::new(BaruaTalk::on(screen));
        assert_eq!(bars(&mut ui), Some(SystemBars::LIGHT_CONTENT), "{screen:?}");
    }
}

#[test]
fn a_sheet_and_the_design_pages_keep_light_icons() {
    let mut ui = Ui::new(BaruaTalk::on(Screen::Home));
    ui.send(Message::OpenStart);
    assert!(ui.app().start_open());
    assert_eq!(bars(&mut ui), Some(SystemBars::LIGHT_CONTENT));
    for name in barua_talk::DESIGNED {
        let mut ui = Ui::new(BaruaTalk::designed(name).unwrap());
        assert_eq!(bars(&mut ui), Some(SystemBars::LIGHT_CONTENT), "{name}");
    }
}

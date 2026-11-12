//! The colour of the system's status and navigation icons over FundiDesk
//! (ADR 0026): dark on its light screens with nothing said, light on the
//! green splash, which says so.

use fundidesk::{DESIGN_HEIGHT, DESIGN_WIDTH, FundiDesk, Screen};
use morningstar::ui::{SystemBars, Ui};
use morningstar::{App, Frame, FrameInfo, SurfaceSize};

fn bars(screen: Screen) -> Option<SystemBars> {
    let mut ui = Ui::new(FundiDesk::on(screen));
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    ui.frame(
        &FrameInfo::new(SurfaceSize::new(DESIGN_WIDTH as u32, DESIGN_HEIGHT as u32)),
        &mut frame,
    );
    frame.system_bars()
}

#[test]
fn a_light_design_gets_dark_icons_and_its_green_splash_light_ones() {
    assert_eq!(bars(Screen::Splash), Some(SystemBars::LIGHT_CONTENT));
    for screen in [Screen::Onboarding, Screen::Login, Screen::Dashboard] {
        assert_eq!(bars(screen), Some(SystemBars::DARK_CONTENT), "{screen:?}");
    }
}

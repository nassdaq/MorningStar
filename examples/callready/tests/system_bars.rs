//! The colour of the system's status and navigation icons over CallReady
//! (ADR 0026): light, on its dark theme, with nothing said by the app.

use callready::CallReady;
use morningstar::ui::{SystemBars, Ui};
use morningstar::{App, Frame, FrameInfo, SurfaceSize};

#[test]
fn a_dark_design_gets_light_icons_by_default() {
    let mut ui = Ui::new(CallReady::default());
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    ui.frame(
        &FrameInfo::new(SurfaceSize::new(1080, 2340)).with_scale(2.8125),
        &mut frame,
    );
    assert_eq!(frame.system_bars(), Some(SystemBars::LIGHT_CONTENT));
}

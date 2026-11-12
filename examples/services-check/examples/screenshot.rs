//! Renders the smoke test's screen to a PNG on this machine's GPU.
//!
//!   cargo run -p services-check --example screenshot [output-dir]

use std::path::PathBuf;

use morningstar::ui::Ui;
use morningstar::{FrameInfo, Insets, SurfaceSize};
use morningstar_devtools::screenshot_frame;
use services_check::{Message, ServicesCheck};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".to_owned()));
    std::fs::create_dir_all(&directory)?;
    let scale = 3.0;
    let info = FrameInfo::new(SurfaceSize::new(1080, 2400))
        .with_scale(scale)
        .with_insets(Insets::new(0.0, 32.0 * scale, 0.0, 20.0 * scale));

    // With no host, each button's request fails as unsupported: the
    // screenshot shows the result lines filled in.
    let mut ui = Ui::new(ServicesCheck::new());
    ui.send(Message::Get);
    ui.send(Message::Record);
    ui.send(Message::StoreLoad);
    let path = directory.join("services-check.png");
    std::fs::write(&path, screenshot_frame(&mut ui, &info)?)?;
    println!("{}", path.display());
    Ok(())
}

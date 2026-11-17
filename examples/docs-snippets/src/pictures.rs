//! Samples for images and SVG files.

use morningstar::ui::prelude::*;

// docs:begin svg
/// An SVG file of the app, parsed once, the first time it is drawn.
static LOGO: SvgAsset = SvgAsset::new(
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <circle cx="12" cy="12" r="10" fill="#E3AE3A"/>
        <path d="M7 12l3.5 3.5L17 9" fill="none" stroke="#141005" stroke-width="2"/>
    </svg>"##,
);

/// The file as a picture, in its own colours.
pub fn logo<M: 'static>() -> Element<M> {
    Picture::svg(LOGO.get()).width(96.0).height(96.0).into()
}
// docs:end svg

// docs:begin image
/// An app with one photograph, decoded once at setup.
#[derive(Default)]
pub struct Gallery {
    photo: Option<Image>,
}

impl Application for Gallery {
    type Message = ();

    fn setup(&mut self, setup: &mut Setup<'_>) {
        // Four pixels stand in for `include_bytes!("../assets/photo.png")`
        // with `setup.add_image_png(..)`.
        let pixels = [255, 255, 255, 255].repeat(4);
        self.photo = setup.add_image_rgba8(2, 2, &pixels).ok();
    }

    fn view(&self) -> Element<()> {
        match self.photo {
            // Cropped to fill a rounded 16:9 box.
            Some(photo) => Picture::new(photo)
                .fit(ImageFit::Cover)
                .radius(16.0)
                .fill_width()
                .aspect_ratio(16.0 / 9.0)
                .into(),
            // An avatar falls back to initials when there is no image.
            None => Avatar::new("Amina Juma").image_maybe(self.photo).into(),
        }
    }

    fn update(&mut self, _message: ()) {}
}
// docs:end image

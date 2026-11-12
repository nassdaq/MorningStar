//! Launcher icons: from one square picture to everything Android and iOS
//! ask for (ADR 0020).
//!
//! An [`IconSet`] is an app's icon as declared: the full-bleed square
//! picture, optionally an Android adaptive foreground and a background
//! colour. From it come
//!
//! * for Android ([`IconSet::android_files`]) a resource directory: the
//!   square and round icons of API 24 and 25 at the five densities, and the
//!   adaptive icon of API 26 and later (a foreground at the five densities
//!   over a colour);
//! * for iOS ([`IconSet::ios_files`]) an asset catalog with the single
//!   1024 x 1024 opaque icon Xcode derives every size from, and the build
//!   settings that carry the app's name and bundle id.
//!
//! **The single-icon rule.** An app that gives only `icon` gets an adaptive
//! foreground made of that icon scaled to the 72 dp the launcher shows,
//! centred on the transparent 108 dp canvas, over the colour of the icon's
//! top-left pixel. So the launcher shows the icon exactly as drawn, cut by
//! its mask: a circular mask removes the corners, as the round icon of
//! API 25 does. Artwork that must survive every mask belongs in the middle
//! 66 dp circle, which is 92% of the icon's width; or give the foreground
//! itself as `icon_foreground`, which is used as it is.
//!
//! Everything here is plain pixel arithmetic on [`Image`]: no GPU, the
//! same bytes for the same input on every machine.

use std::path::{Path, PathBuf};

use crate::identity::{IconError, Identity};
use crate::image::Image;

/// Android's density buckets with the side of a 48 dp launcher icon in
/// each, in pixels.
pub(crate) const DENSITIES: [(&str, u32); 5] = [
    ("mdpi", 48),
    ("hdpi", 72),
    ("xhdpi", 96),
    ("xxhdpi", 144),
    ("xxxhdpi", 192),
];

/// The smallest source picture accepted, in pixels a side.
const MINIMUM: u32 = 512;
/// The side of the iOS icon, and of the built-in pictures.
const FULL: u32 = 1024;

/// The built-in icon's field: the dark blue of a sky before sunrise.
const FIELD: [u8; 3] = [0x10, 0x14, 0x26];
/// The built-in icon's star.
const STAR: [u8; 3] = [0xFF, 0xD9, 0x8C];
/// How far the built-in star reaches from the centre, as a fraction of the
/// icon's width.
const STAR_REACH: f64 = 0.31;
/// The star is the set `|x|^p + |y|^p <= 1`: four points with sides that
/// curve inwards. 1 would be a diamond, 0.5 a thin astroid.
const STAR_EXPONENT: f64 = 0.6;

/// The shape a launcher cuts an adaptive icon to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mask {
    /// A circle touching the four sides.
    Circle,
    /// A square with heavily rounded corners (the superellipse
    /// `x^4 + y^4 <= 1`), close to what Pixel and Samsung launchers use.
    Squircle,
}

impl Mask {
    /// Whether the point is inside, with the icon spanning -1 to 1.
    fn contains(self, x: f64, y: f64) -> bool {
        match self {
            Self::Circle => x * x + y * y <= 1.0,
            Self::Squircle => x.powi(4) + y.powi(4) <= 1.0,
        }
    }
}

/// One generated file: where it goes under the output directory (with `/`
/// between parts) and its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// What [`write_generated`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Written {
    /// Files created or replaced.
    pub written: Vec<PathBuf>,
    /// Files that already had the right bytes and were left alone.
    pub unchanged: usize,
}

/// Writes `files` under `directory`, creating directories as needed. A
/// file that already has exactly these bytes is not touched, so its
/// modification time stays and a build system sees nothing to redo.
pub fn write_generated(directory: &Path, files: &[Generated]) -> Result<Written, IconError> {
    let mut done = Written::default();
    for file in files {
        let path = directory.join(&file.path);
        if std::fs::read(&path).is_ok_and(|existing| existing == file.bytes) {
            done.unchanged += 1;
            continue;
        }
        let failed = |source| IconError::Write {
            path: path.clone(),
            source,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(failed)?;
        }
        std::fs::write(&path, &file.bytes).map_err(failed)?;
        done.written.push(path);
    }
    Ok(done)
}

/// An app's icon as declared, checked and ready to be made into files.
#[derive(Clone, Debug)]
pub struct IconSet {
    icon: Image,
    /// What to call the icon in an error message.
    icon_path: PathBuf,
    foreground: Option<Image>,
    background: Option<[u8; 3]>,
    /// Whether the foreground is a plain silhouette, usable as Android's
    /// themed (monochrome) icon. True only of the built-in icon.
    silhouette: bool,
}

impl IconSet {
    /// Loads and checks the icon `identity` declares, or draws the
    /// built-in one if it declares none.
    pub fn for_identity(identity: &Identity) -> Result<Self, IconError> {
        let Some(icon_path) = &identity.icon else {
            return Ok(Self::builtin(identity.icon_background));
        };
        let load = |path: &PathBuf| {
            let image = Image::load(path).map_err(|source| IconError::Image {
                path: path.clone(),
                source,
            })?;
            checked(image, path)
        };
        Ok(Self {
            icon: load(icon_path)?,
            icon_path: icon_path.clone(),
            foreground: identity.icon_foreground.as_ref().map(load).transpose()?,
            background: identity.icon_background,
            silhouette: false,
        })
    }

    /// An icon from pictures already in memory. Both must be square and at
    /// least 512 pixels a side.
    pub fn new(
        icon: Image,
        foreground: Option<Image>,
        background: Option<[u8; 3]>,
    ) -> Result<Self, IconError> {
        let icon_path = PathBuf::from("the icon");
        Ok(Self {
            icon: checked(icon, &icon_path)?,
            icon_path,
            foreground: foreground
                .map(|image| checked(image, Path::new("the icon foreground")))
                .transpose()?,
            background,
            silhouette: false,
        })
    }

    /// The icon of an app that has none of its own: a four-pointed star on
    /// a dark field, or on `background` if the app chose a colour.
    pub fn builtin(background: Option<[u8; 3]>) -> Self {
        let field = background.unwrap_or(FIELD);
        // The same star in both: the foreground's is 72/108 the size, so
        // the launcher's 72 dp window shows exactly the full icon.
        let mut icon = star(STAR_REACH);
        for pixel in icon.rgba_mut().chunks_exact_mut(4) {
            let [r, g, b] = over(field, [pixel[0], pixel[1], pixel[2], pixel[3]]);
            pixel.copy_from_slice(&[r, g, b, 255]);
        }
        Self {
            icon,
            icon_path: PathBuf::from("the built-in icon"),
            foreground: Some(star(STAR_REACH * 72.0 / 108.0)),
            background: Some(field),
            silhouette: true,
        }
    }

    /// The colour behind the adaptive foreground, and behind a transparent
    /// icon on iOS: the one given, or else the icon's top-left pixel.
    pub fn background(&self) -> Result<[u8; 3], IconError> {
        if let Some(colour) = self.background {
            return Ok(colour);
        }
        match self.icon.pixel(0, 0) {
            [r, g, b, 255] => Ok([r, g, b]),
            _ => Err(IconError::NoBackground {
                path: self.icon_path.clone(),
            }),
        }
    }

    /// The square icon of API 24 and 25, `side` pixels a side: the icon,
    /// full-bleed.
    pub fn legacy(&self, side: u32) -> Image {
        self.icon.resized(side, side)
    }

    /// The round icon of API 25: the square one cut to a circle.
    pub fn round(&self, side: u32) -> Image {
        masked(&self.legacy(side), Mask::Circle)
    }

    /// The adaptive foreground on its whole 108 dp canvas, `side` pixels a
    /// side: the app's own, or the icon in the middle two thirds (the
    /// single-icon rule; see the module's documentation).
    ///
    /// # Panics
    /// Panics if `side` is not a multiple of 6 while the app has no
    /// foreground of its own (the 18 dp margin must be whole pixels). Every
    /// density's is a multiple of 54.
    pub fn foreground(&self, side: u32) -> Image {
        if let Some(foreground) = &self.foreground {
            return foreground.resized(side, side);
        }
        assert!(
            side > 0 && side.is_multiple_of(6),
            "{side} is not a multiple of 6"
        );
        let window = side / 3 * 2;
        let margin = side / 6;
        let icon = self.icon.resized(window, window);
        let mut canvas = Image::filled(side, side, [0, 0, 0, 0]);
        for y in 0..window {
            for x in 0..window {
                canvas.set_pixel(margin + x, margin + y, icon.pixel(x, y));
            }
        }
        canvas
    }

    /// The adaptive icon as a launcher with this `mask` shows it, `side`
    /// pixels a side: the middle 72 of the foreground's 108 dp over the
    /// background colour, cut to the mask. For looking at, not for a build.
    ///
    /// # Panics
    /// Panics if `side` is not a multiple of 4.
    pub fn adaptive(&self, side: u32, mask: Mask) -> Result<Image, IconError> {
        assert!(
            side > 0 && side.is_multiple_of(4),
            "{side} is not a multiple of 4"
        );
        let background = self.background()?;
        let canvas = self.foreground(side / 2 * 3);
        let margin = side / 4;
        let mut window = Image::filled(side, side, [0, 0, 0, 255]);
        for y in 0..side {
            for x in 0..side {
                let [r, g, b] = over(background, canvas.pixel(margin + x, margin + y));
                window.set_pixel(x, y, [r, g, b, 255]);
            }
        }
        Ok(masked(&window, mask))
    }

    /// The iOS icon: 1024 x 1024 and opaque, anything transparent filled
    /// with the background colour.
    pub fn ios_icon(&self) -> Result<Image, IconError> {
        let mut icon = self.icon.resized(FULL, FULL);
        if !icon.is_opaque() {
            let background = self.background()?;
            for pixel in icon.rgba_mut().chunks_exact_mut(4) {
                let [r, g, b] = over(background, [pixel[0], pixel[1], pixel[2], pixel[3]]);
                pixel.copy_from_slice(&[r, g, b, 255]);
            }
        }
        Ok(icon)
    }

    /// Whether the adaptive icon includes a themed (monochrome) layer. Only
    /// the built-in icon does: its foreground is a silhouette. One cannot
    /// be derived from an app's own full-colour picture.
    pub fn has_monochrome(&self) -> bool {
        self.silhouette
    }

    /// The contents of an Android resource directory:
    ///
    /// * `mipmap-<density>/ic_launcher.png` and `ic_launcher_round.png` at
    ///   48, 72, 96, 144 and 192 pixels;
    /// * `mipmap-<density>/ic_launcher_foreground.png` at 108, 162, 216,
    ///   324 and 432 pixels;
    /// * `mipmap-anydpi-v26/ic_launcher.xml` and `ic_launcher_round.xml`,
    ///   the adaptive icon;
    /// * `values/ic_launcher_background.xml`, its background colour.
    ///
    /// The manifest refers to `@mipmap/ic_launcher` and
    /// `@mipmap/ic_launcher_round`.
    pub fn android_files(&self) -> Result<Vec<Generated>, IconError> {
        let [r, g, b] = self.background()?;
        let mut files = Vec::new();
        let mut png = |path: String, image: &Image| {
            files.push(Generated {
                path,
                bytes: image.to_png(),
            });
        };
        for (density, side) in DENSITIES {
            let legacy = self.legacy(side);
            png(format!("mipmap-{density}/ic_launcher.png"), &legacy);
            png(
                format!("mipmap-{density}/ic_launcher_round.png"),
                &masked(&legacy, Mask::Circle),
            );
            // 108 dp to the icon's 48.
            png(
                format!("mipmap-{density}/ic_launcher_foreground.png"),
                &self.foreground(side / 4 * 9),
            );
        }

        let monochrome = if self.silhouette {
            "    <monochrome android:drawable=\"@mipmap/ic_launcher_foreground\" />\n"
        } else {
            ""
        };
        let adaptive = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <!-- Generated by morningstar-icon (ADR 0020). Do not edit. -->\n\
             <adaptive-icon xmlns:android=\"http://schemas.android.com/apk/res/android\">\n\
             \x20   <background android:drawable=\"@color/ic_launcher_background\" />\n\
             \x20   <foreground android:drawable=\"@mipmap/ic_launcher_foreground\" />\n\
             {monochrome}\
             </adaptive-icon>\n"
        );
        let colour = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <!-- Generated by morningstar-icon (ADR 0020). Do not edit. -->\n\
             <resources>\n\
             \x20   <color name=\"ic_launcher_background\">#{r:02X}{g:02X}{b:02X}</color>\n\
             </resources>\n"
        );
        for (path, text) in [
            ("mipmap-anydpi-v26/ic_launcher.xml", &adaptive),
            ("mipmap-anydpi-v26/ic_launcher_round.xml", &adaptive),
            ("values/ic_launcher_background.xml", &colour),
        ] {
            files.push(Generated {
                path: path.to_owned(),
                bytes: text.clone().into_bytes(),
            });
        }
        Ok(files)
    }

    /// The contents of the generated folder of the iOS host:
    ///
    /// * `Assets.xcassets/AppIcon.appiconset/AppIcon.png`, 1024 x 1024 with
    ///   no alpha channel, and the `Contents.json` of a single-size
    ///   universal icon (Xcode 14 and later make every size from it);
    /// * `Assets.xcassets/Contents.json`;
    /// * `Morningstar.xcconfig`: `MORNINGSTAR_BUNDLE_ID`,
    ///   `MORNINGSTAR_PRODUCT_NAME` (the package's name, which is safe in a
    ///   path) and `MORNINGSTAR_DISPLAY_NAME` (the name under the icon),
    ///   which `templates/ios/project.yml` refers to from the build
    ///   settings of those meanings.
    ///
    /// NOT VERIFIED: nothing here has been through Xcode (ADR 0020).
    pub fn ios_files(&self, identity: &Identity) -> Result<Vec<Generated>, IconError> {
        let icon = self.ios_icon()?;
        const INFO: &str =
            "  \"info\" : {\n    \"author\" : \"xcode\",\n    \"version\" : 1\n  }\n";
        let catalog = format!("{{\n{INFO}}}\n");
        let set = format!(
            "{{\n  \"images\" : [\n    {{\n      \"filename\" : \"AppIcon.png\",\n      \
             \"idiom\" : \"universal\",\n      \"platform\" : \"ios\",\n      \
             \"size\" : \"1024x1024\"\n    }}\n  ],\n{INFO}}}\n"
        );
        let settings = format!(
            "// Generated by morningstar-icon from [package.metadata.morningstar] of\n\
             // {package} (ADR 0020). Do not edit: change Cargo.toml and run\n\
             // tools/build-ios.sh again.\n\
             MORNINGSTAR_BUNDLE_ID = {id}\n\
             MORNINGSTAR_PRODUCT_NAME = {package}\n\
             MORNINGSTAR_DISPLAY_NAME = {name}\n",
            package = identity.package,
            id = identity.bundle_id(),
            name = identity.name,
        );
        Ok(vec![
            Generated {
                path: "Assets.xcassets/Contents.json".to_owned(),
                bytes: catalog.into_bytes(),
            },
            Generated {
                path: "Assets.xcassets/AppIcon.appiconset/Contents.json".to_owned(),
                bytes: set.into_bytes(),
            },
            Generated {
                path: "Assets.xcassets/AppIcon.appiconset/AppIcon.png".to_owned(),
                bytes: crate::png::encode_opaque(FULL, FULL, icon.rgba()),
            },
            Generated {
                path: "Morningstar.xcconfig".to_owned(),
                bytes: settings.into_bytes(),
            },
        ])
    }

    /// One picture of the icon in every form, for looking at before it is
    /// on a phone. Four rows on grey, each: the square icon, the round
    /// icon, the adaptive icon under a circle, the adaptive icon under a
    /// squircle.
    ///
    /// 1. at xxxhdpi (192 px; adaptive 288), followed by the iOS icon;
    /// 2. at xhdpi, "2x" (96 px; adaptive 144);
    /// 3. at mdpi, one pixel per dp (48 px; adaptive scaled to 48, the size
    ///    a launcher draws it);
    /// 4. row 3 again with every pixel four wide, to see what survives.
    pub fn preview(&self) -> Result<Image, IconError> {
        const GAP: u32 = 24;
        let row = |legacy: u32, adaptive: u32| -> Result<Vec<Image>, IconError> {
            Ok(vec![
                self.legacy(legacy),
                self.round(legacy),
                self.adaptive(adaptive, Mask::Circle)?,
                self.adaptive(adaptive, Mask::Squircle)?,
            ])
        };
        let mut large = row(192, 288)?;
        large.push(self.ios_icon()?.resized(288, 288));
        // A launcher draws the 72 dp window at about the legacy icon's
        // size; take it from xxxhdpi so the reduction is one clean step.
        let small: Vec<Image> = row(48, 288)?
            .into_iter()
            .map(|image| image.resized(48, 48))
            .collect();
        let magnified = small.iter().map(|image| enlarged(image, 4)).collect();
        let rows = [large, row(96, 144)?, small, magnified];

        let width = |row: &Vec<Image>| row.iter().map(|image| image.width() + GAP).sum::<u32>();
        let height = |row: &Vec<Image>| row.iter().map(Image::height).max().unwrap_or(0) + GAP;
        let mut sheet = Image::filled(
            GAP + rows.iter().map(width).max().unwrap_or(0),
            GAP + rows.iter().map(height).sum::<u32>(),
            [0x8A, 0x8A, 0x8E, 255],
        );
        let mut top = GAP;
        for row in &rows {
            let mut left = GAP;
            for image in row {
                for y in 0..image.height() {
                    for x in 0..image.width() {
                        let [r, g, b, _] = sheet.pixel(left + x, top + y);
                        let [r, g, b] = over([r, g, b], image.pixel(x, y));
                        sheet.set_pixel(left + x, top + y, [r, g, b, 255]);
                    }
                }
                left += image.width() + GAP;
            }
            top += height(row);
        }
        Ok(sheet)
    }
}

/// A source picture that can be scaled to every size, or why not.
fn checked(image: Image, path: &Path) -> Result<Image, IconError> {
    let (width, height) = (image.width(), image.height());
    if width != height {
        return Err(IconError::NotSquare {
            path: path.to_owned(),
            width,
            height,
        });
    }
    if width < MINIMUM {
        return Err(IconError::TooSmall {
            path: path.to_owned(),
            width,
            minimum: MINIMUM,
        });
    }
    Ok(image)
}

/// `pixel` drawn over an opaque colour, blended as the renderer, browsers
/// and both platforms' launchers composite: on the sRGB-encoded values
/// (ADR 0023). Half-transparent white over black is byte 128.
fn over(under: [u8; 3], pixel: [u8; 4]) -> [u8; 3] {
    match pixel[3] {
        255 => [pixel[0], pixel[1], pixel[2]],
        0 => under,
        alpha => {
            let alpha = u32::from(alpha);
            std::array::from_fn(|channel| {
                let blended =
                    u32::from(pixel[channel]) * alpha + u32::from(under[channel]) * (255 - alpha);
                ((blended + 127) / 255) as u8
            })
        }
    }
}

/// How much of the pixel at (`x`, `y`) of a `side`-pixel square satisfies
/// `inside`, which takes a point with the square spanning -1 to 1: the
/// share of an 8 x 8 grid of samples, so edges are anti-aliased.
fn coverage(side: u32, x: u32, y: u32, inside: impl Fn(f64, f64) -> bool) -> f64 {
    const SAMPLES: u32 = 8;
    let unit = 2.0 / (f64::from(side) * f64::from(SAMPLES));
    let mut hits = 0u32;
    for sub_y in 0..SAMPLES {
        for sub_x in 0..SAMPLES {
            let px = (f64::from(x * SAMPLES + sub_x) + 0.5) * unit - 1.0;
            let py = (f64::from(y * SAMPLES + sub_y) + 0.5) * unit - 1.0;
            hits += u32::from(inside(px, py));
        }
    }
    f64::from(hits) / f64::from(SAMPLES * SAMPLES)
}

/// A square image cut to `mask`, with an anti-aliased edge.
fn masked(image: &Image, mask: Mask) -> Image {
    let side = image.width();
    let mut out = image.clone();
    for y in 0..side {
        for x in 0..side {
            let [r, g, b, alpha] = image.pixel(x, y);
            let covered = coverage(side, x, y, |px, py| mask.contains(px, py));
            let alpha = (f64::from(alpha) * covered).round() as u8;
            out.set_pixel(x, y, [r, g, b, alpha]);
        }
    }
    out
}

/// The built-in star on a transparent 1024 x 1024 canvas, reaching `reach`
/// of the canvas's width from the centre.
fn star(reach: f64) -> Image {
    let inside = |x: f64, y: f64| {
        let (x, y) = (x.abs() / (2.0 * reach), y.abs() / (2.0 * reach));
        x.powf(STAR_EXPONENT) + y.powf(STAR_EXPONENT) <= 1.0
    };
    let mut image = Image::filled(FULL, FULL, [STAR[0], STAR[1], STAR[2], 0]);
    // Only the star's bounding box can be covered at all.
    let margin = ((0.5 - reach) * f64::from(FULL)).floor().max(0.0) as u32;
    for y in margin..FULL - margin {
        for x in margin..FULL - margin {
            // Far from the outline one sample decides; a pixel is 1/1024 of
            // the canvas and the star's defining sum changes by less than
            // 0.1 across one anywhere it is near 1.
            let unit = 2.0 / f64::from(FULL);
            let (cx, cy) = (
                (f64::from(x) + 0.5) * unit - 1.0,
                (f64::from(y) + 0.5) * unit - 1.0,
            );
            let sum = (cx.abs() / (2.0 * reach)).powf(STAR_EXPONENT)
                + (cy.abs() / (2.0 * reach)).powf(STAR_EXPONENT);
            let covered = if sum < 0.85 {
                1.0
            } else if sum > 1.15 {
                0.0
            } else {
                coverage(FULL, x, y, inside)
            };
            let alpha = (covered * 255.0).round() as u8;
            image.set_pixel(x, y, [STAR[0], STAR[1], STAR[2], alpha]);
        }
    }
    image
}

/// `image` with every pixel `factor` wide and high, unsmoothed.
fn enlarged(image: &Image, factor: u32) -> Image {
    let mut out = Image::filled(
        image.width() * factor,
        image.height() * factor,
        [0, 0, 0, 0],
    );
    for y in 0..out.height() {
        for x in 0..out.width() {
            out.set_pixel(x, y, image.pixel(x / factor, y / factor));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::tests::metadata_json;

    const RED: [u8; 4] = [200, 30, 40, 255];
    const BLUE: [u8; 4] = [20, 40, 220, 255];

    /// A red square with a blue square in its middle half.
    fn picture(side: u32) -> Image {
        let mut image = Image::filled(side, side, RED);
        image.fill_rect(
            crate::Rect::new(side / 4, side / 4, side / 2, side / 2),
            BLUE,
        );
        image
    }

    /// A transparent square with an opaque blue square in its middle half.
    fn cutout(side: u32) -> Image {
        let mut image = Image::filled(side, side, [0, 0, 0, 0]);
        image.fill_rect(
            crate::Rect::new(side / 4, side / 4, side / 2, side / 2),
            BLUE,
        );
        image
    }

    fn file<'a>(files: &'a [Generated], path: &str) -> &'a [u8] {
        &files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("no {path}"))
            .bytes
    }

    fn decoded(files: &[Generated], path: &str) -> Image {
        Image::from_png(file(files, path)).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("morningstar-icon-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn android_gets_every_file_at_its_size() {
        let files = IconSet::new(picture(512), None, None)
            .unwrap()
            .android_files()
            .unwrap();
        // Three pictures at five densities, two XML icons, one colour.
        assert_eq!(files.len(), 18);
        for (density, legacy, foreground) in [
            ("mdpi", 48, 108),
            ("hdpi", 72, 162),
            ("xhdpi", 96, 216),
            ("xxhdpi", 144, 324),
            ("xxxhdpi", 192, 432),
        ] {
            for (name, side) in [
                ("ic_launcher", legacy),
                ("ic_launcher_round", legacy),
                ("ic_launcher_foreground", foreground),
            ] {
                let image = decoded(&files, &format!("mipmap-{density}/{name}.png"));
                assert_eq!(
                    (image.width(), image.height()),
                    (side, side),
                    "{density}/{name}"
                );
            }
        }
        let xml = std::str::from_utf8(file(&files, "mipmap-anydpi-v26/ic_launcher.xml")).unwrap();
        assert!(xml.contains("<background android:drawable=\"@color/ic_launcher_background\" />"));
        assert!(xml.contains("<foreground android:drawable=\"@mipmap/ic_launcher_foreground\" />"));
        assert!(
            !xml.contains("monochrome"),
            "none can be derived from a picture"
        );
        assert_eq!(
            file(&files, "mipmap-anydpi-v26/ic_launcher_round.xml"),
            xml.as_bytes()
        );
        // No background given: the icon's corner pixel.
        let colour =
            std::str::from_utf8(file(&files, "values/ic_launcher_background.xml")).unwrap();
        assert!(
            colour.contains("<color name=\"ic_launcher_background\">#C81E28</color>"),
            "{colour}"
        );
    }

    #[test]
    fn the_square_icon_is_the_picture_and_the_round_one_is_cut_to_a_circle() {
        let set = IconSet::new(picture(512), None, None).unwrap();
        let square = set.legacy(48);
        assert!(square.is_opaque());
        assert_eq!(square.pixel(0, 0), RED);
        assert_eq!(square.pixel(24, 24), BLUE);

        let round = set.round(48);
        for (x, y) in [(0, 0), (47, 0), (0, 47), (47, 47), (6, 6)] {
            assert_eq!(round.pixel(x, y)[3], 0, "corner ({x}, {y})");
        }
        // Inside, the picture is untouched.
        for (x, y) in [(24, 24), (24, 1), (1, 24), (46, 24), (8, 8)] {
            assert_eq!(round.pixel(x, y), square.pixel(x, y), "({x}, {y})");
        }
        // The edge is anti-aliased, and the whole is a disc: pi/4 of the square.
        let alphas: Vec<u8> = round.rgba().chunks_exact(4).map(|pixel| pixel[3]).collect();
        assert!(alphas.iter().any(|&alpha| alpha > 0 && alpha < 255));
        let covered = alphas.iter().map(|&alpha| f64::from(alpha)).sum::<f64>() / 255.0;
        let disc = std::f64::consts::FRAC_PI_4 * 48.0 * 48.0;
        assert!((covered - disc).abs() < 2.0, "{covered} vs {disc}");
        // Symmetric in both axes.
        for y in 0..48 {
            for x in 0..48 {
                assert_eq!(round.pixel(x, y)[3], round.pixel(47 - x, y)[3]);
                assert_eq!(round.pixel(x, y)[3], round.pixel(x, 47 - y)[3]);
            }
        }
    }

    #[test]
    fn with_only_an_icon_the_foreground_is_the_icon_in_the_72_dp_window() {
        let set = IconSet::new(picture(1024), None, None).unwrap();
        for side in [108, 162, 216, 324, 432] {
            let foreground = set.foreground(side);
            let (margin, window) = (side / 6, side / 3 * 2);
            let icon = set.legacy(window);
            for y in 0..side {
                for x in 0..side {
                    let inside = (margin..margin + window).contains(&x)
                        && (margin..margin + window).contains(&y);
                    let expected = if inside {
                        icon.pixel(x - margin, y - margin)
                    } else {
                        [0, 0, 0, 0]
                    };
                    assert_eq!(foreground.pixel(x, y), expected, "{side}: ({x}, {y})");
                }
            }
        }
        // So a launcher shows the icon itself, cut by its mask.
        let shown = set.adaptive(288, Mask::Circle).unwrap();
        assert_eq!(shown, set.round(288));
    }

    #[test]
    fn a_foreground_of_the_apps_own_is_used_as_it_is() {
        let set = IconSet::new(picture(512), Some(cutout(864)), Some([1, 2, 3])).unwrap();
        assert_eq!(set.foreground(432), cutout(432));
        assert_eq!(set.foreground(108), cutout(108));
        let files = set.android_files().unwrap();
        assert_eq!(
            decoded(&files, "mipmap-xxxhdpi/ic_launcher_foreground.png"),
            cutout(432)
        );
        // The colour given wins over the corner pixel.
        let colour =
            std::str::from_utf8(file(&files, "values/ic_launcher_background.xml")).unwrap();
        assert!(colour.contains("#010203"), "{colour}");
        // Under a mask: the background where the foreground is clear.
        let shown = set.adaptive(144, Mask::Squircle).unwrap();
        assert_eq!(shown.pixel(72, 72), BLUE);
        assert_eq!(shown.pixel(72, 4), [1, 2, 3, 255]);
        assert_eq!(shown.pixel(0, 0)[3], 0);
        // A squircle keeps more of the corner than a circle does.
        let circle = set.adaptive(144, Mask::Circle).unwrap();
        assert_eq!(circle.pixel(24, 24)[3], 255);
        assert_eq!(circle.pixel(16, 16)[3], 0);
        assert_eq!(shown.pixel(16, 16)[3], 255);
    }

    #[test]
    fn a_transparent_corner_needs_a_background_colour() {
        let set = IconSet::new(cutout(512), None, None).unwrap();
        let error = set.android_files().unwrap_err().to_string();
        assert!(error.contains("icon_background"), "{error}");
        assert!(set.ios_icon().is_err());
        assert!(
            IconSet::new(cutout(512), None, Some([0, 0, 0]))
                .unwrap()
                .android_files()
                .is_ok()
        );
    }

    #[test]
    fn the_ios_icon_is_1024_opaque_and_flattened_onto_the_background() {
        // Half-transparent white over black is half the encoded value, as
        // the renderer and the platforms composite (ADR 0023).
        let mut icon = cutout(512);
        icon.fill_rect(crate::Rect::new(0, 0, 512, 64), [255, 255, 255, 128]);
        let set = IconSet::new(icon, None, Some([0, 0, 0])).unwrap();
        let flat = set.ios_icon().unwrap();
        assert_eq!((flat.width(), flat.height()), (1024, 1024));
        assert!(flat.is_opaque());
        assert_eq!(flat.pixel(10, 10), [128, 128, 128, 255]);
        assert_eq!(flat.pixel(10, 400), [0, 0, 0, 255]);
        assert_eq!(flat.pixel(512, 512), BLUE);

        let identity = Identity::from_cargo_metadata(
            &metadata_json(
                "my-app",
                Path::new("/nowhere/my-app"),
                r#"{"morningstar":{"name":"My App","id":"dev.example.my_app"}}"#,
            ),
            "my-app",
        )
        .unwrap();
        let files = set.ios_files(&identity).unwrap();
        assert_eq!(files.len(), 4);
        let png = file(&files, "Assets.xcassets/AppIcon.appiconset/AppIcon.png");
        // IHDR: bit depth 8, colour type 2 (RGB): no alpha channel at all,
        // which is what the App Store asks of an icon.
        assert_eq!(&png[24..26], &[8, 2]);
        assert_eq!(Image::from_png(png).unwrap(), flat);

        let contents = std::str::from_utf8(file(
            &files,
            "Assets.xcassets/AppIcon.appiconset/Contents.json",
        ))
        .unwrap();
        let contents = crate::json::parse(contents).unwrap();
        let images = contents.get("images").unwrap().as_array().unwrap();
        assert_eq!(images.len(), 1);
        for (key, value) in [
            ("filename", "AppIcon.png"),
            ("idiom", "universal"),
            ("platform", "ios"),
            ("size", "1024x1024"),
        ] {
            assert_eq!(images[0].get(key).unwrap().as_str(), Some(value));
        }
        assert!(
            crate::json::parse(
                std::str::from_utf8(file(&files, "Assets.xcassets/Contents.json")).unwrap()
            )
            .is_ok()
        );

        let settings = std::str::from_utf8(file(&files, "Morningstar.xcconfig")).unwrap();
        assert!(settings.contains("\nMORNINGSTAR_BUNDLE_ID = dev.example.my-app\n"));
        assert!(settings.contains("\nMORNINGSTAR_PRODUCT_NAME = my-app\n"));
        assert!(settings.contains("\nMORNINGSTAR_DISPLAY_NAME = My App\n"));
    }

    #[test]
    fn an_opaque_icon_needs_no_background_on_ios() {
        let set = IconSet::new(picture(1024), None, None).unwrap();
        assert_eq!(set.ios_icon().unwrap(), picture(1024));
    }

    #[test]
    fn pictures_that_cannot_be_icons_are_refused() {
        let error = IconSet::new(Image::filled(1024, 512, RED), None, None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("1024x512") && error.contains("square"),
            "{error}"
        );
        let error = IconSet::new(picture(256), None, None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("256x256") && error.contains("512x512"),
            "{error}"
        );
        let error = IconSet::new(picture(512), Some(picture(128)), None)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("foreground") && error.contains("128x128"),
            "{error}"
        );
    }

    #[test]
    fn icon_files_are_loaded_and_checked_through_the_identity() {
        let dir = scratch("load");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        let identity = |metadata: &str| {
            Identity::from_cargo_metadata(&metadata_json("my-app", &dir, metadata), "my-app")
                .unwrap()
        };

        // No icon anywhere: the built-in one.
        let set = IconSet::for_identity(&identity("null")).unwrap();
        assert!(set.has_monochrome());

        // assets/icon.png is found without being named.
        picture(512).save(dir.join("assets/icon.png")).unwrap();
        let set = IconSet::for_identity(&identity("null")).unwrap();
        assert!(!set.has_monochrome());
        assert_eq!(set.background().unwrap(), [200, 30, 40]);

        // A file that is not a PNG, a rectangle, and one too small.
        std::fs::write(dir.join("assets/icon.jpg"), b"\xFF\xD8\xFF\xE0 not a png").unwrap();
        Image::filled(600, 512, RED)
            .save(dir.join("assets/wide.png"))
            .unwrap();
        picture(128).save(dir.join("assets/small.png")).unwrap();
        for (file, expected) in [
            ("assets/icon.jpg", "must be a PNG"),
            ("assets/wide.png", "must be square"),
            ("assets/small.png", "at least 512x512"),
        ] {
            let error = IconSet::for_identity(&identity(&format!(
                r#"{{"morningstar":{{"icon":"{file}"}}}}"#
            )))
            .unwrap_err()
            .to_string();
            assert!(error.contains(expected), "{file}: {error}");
            assert!(error.contains(file), "names the file: {error}");
        }
        let error = IconSet::for_identity(&identity(
            r#"{"morningstar":{"icon_foreground":"assets/small.png"}}"#,
        ))
        .unwrap_err()
        .to_string();
        assert!(error.contains("assets/small.png"), "{error}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_built_in_icon_is_a_light_star_on_a_dark_field() {
        let set = IconSet::builtin(None);
        let icon = set.legacy(1024);
        assert!(icon.is_opaque());
        assert_eq!(icon.pixel(0, 0), [FIELD[0], FIELD[1], FIELD[2], 255]);
        assert_eq!(icon.pixel(512, 512), [STAR[0], STAR[1], STAR[2], 255]);
        // Four points: on the axes it reaches far, on the diagonals it does not.
        let reach = (STAR_REACH * 1024.0) as u32;
        assert_eq!(icon.pixel(512, 512 - reach + 40)[..3], STAR);
        assert_eq!(icon.pixel(512 + reach - 40, 512)[..3], STAR);
        assert_eq!(icon.pixel(512 + reach / 2, 512 + reach / 2)[..3], FIELD);
        // Symmetric, so it has no wrong way up.
        for (x, y) in [(300, 512), (512, 260), (450, 470), (505, 250)] {
            assert_eq!(icon.pixel(x, y), icon.pixel(1023 - x, y));
            assert_eq!(icon.pixel(x, y), icon.pixel(x, 1023 - y));
            assert_eq!(icon.pixel(x, y), icon.pixel(y, x));
        }
        // The foreground is the same star, clear around it and inside the
        // 66 dp circle every mask keeps.
        let foreground = set.foreground(432);
        assert_eq!(foreground.pixel(0, 0)[3], 0);
        assert_eq!(foreground.pixel(216, 216), [STAR[0], STAR[1], STAR[2], 255]);
        for y in 0..432u32 {
            for x in 0..432u32 {
                let (dx, dy) = (f64::from(x) - 215.5, f64::from(y) - 215.5);
                if dx.hypot(dy) > 132.0 {
                    assert_eq!(foreground.pixel(x, y)[3], 0, "({x}, {y})");
                }
            }
        }
        // What a launcher shows is the full icon under its mask.
        let shown = set.adaptive(192, Mask::Circle).unwrap();
        let round = set.round(192);
        // (Resampled by different factors, so only nearly the same bytes.)
        let total: u64 = shown
            .rgba()
            .iter()
            .zip(round.rgba())
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        let mean = total as f64 / shown.rgba().len() as f64;
        assert!(mean < 0.5, "mean difference {mean}");

        // Its silhouette is the themed icon.
        let files = set.android_files().unwrap();
        let xml = std::str::from_utf8(file(&files, "mipmap-anydpi-v26/ic_launcher.xml")).unwrap();
        assert!(xml.contains("<monochrome android:drawable=\"@mipmap/ic_launcher_foreground\" />"));
        // A colour of the app's own replaces the field.
        let tinted = IconSet::builtin(Some([30, 132, 22]));
        assert_eq!(tinted.legacy(48).pixel(0, 0), [30, 132, 22, 255]);
        assert_eq!(tinted.background().unwrap(), [30, 132, 22]);
    }

    #[test]
    fn the_same_input_gives_the_same_bytes() {
        let set = || IconSet::new(picture(512), Some(cutout(512)), None).unwrap();
        assert_eq!(
            set().android_files().unwrap(),
            set().android_files().unwrap()
        );
        assert_eq!(
            IconSet::builtin(None).android_files().unwrap(),
            IconSet::builtin(None).android_files().unwrap()
        );
        assert_eq!(set().preview().unwrap(), set().preview().unwrap());
    }

    #[test]
    fn files_that_have_not_changed_are_not_written_again() {
        let dir = scratch("write");
        let files = IconSet::new(picture(512), None, None)
            .unwrap()
            .android_files()
            .unwrap();
        let first = write_generated(&dir, &files).unwrap();
        assert_eq!((first.written.len(), first.unchanged), (18, 0));
        for file in &files {
            assert_eq!(std::fs::read(dir.join(&file.path)).unwrap(), file.bytes);
        }

        // Make "untouched" observable whatever the clock's resolution:
        // writing to a read-only file fails, leaving it alone does not.
        let watched = dir.join("mipmap-mdpi/ic_launcher.png");
        let mut permissions = std::fs::metadata(&watched).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&watched, permissions.clone()).unwrap();
        let modified = std::fs::metadata(&watched).unwrap().modified().unwrap();

        let second = write_generated(&dir, &files).unwrap();
        assert_eq!((second.written.len(), second.unchanged), (0, 18));
        assert_eq!(
            std::fs::metadata(&watched).unwrap().modified().unwrap(),
            modified
        );

        // A different icon rewrites what differs and only that.
        let other = IconSet::new(picture(512), None, Some([9, 9, 9]))
            .unwrap()
            .android_files()
            .unwrap();
        let third = write_generated(&dir, &other).unwrap();
        assert_eq!(
            third.written,
            [dir.join("values/ic_launcher_background.xml")]
        );
        assert_eq!(third.unchanged, 17);

        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&watched, permissions).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_preview_shows_every_form() {
        let sheet = IconSet::new(picture(512), None, None)
            .unwrap()
            .preview()
            .unwrap();
        assert!(sheet.is_opaque());
        // Widest row: 192 + 192 + 288 + 288 + 288 and six gaps of 24.
        assert_eq!(sheet.width(), 1392);
        // Top-left tile: the square icon at 192.
        assert_eq!(sheet.pixel(24, 24), RED);
        assert_eq!(sheet.pixel(24 + 96, 24 + 96), BLUE);
    }
}

//! The SVG loader: what the subset draws, what it refuses and where, and
//! that hostile input is bounded and never panics. See ADR 0027.

use morningstar_core::Color;
use morningstar_text::{
    Layer, LineCap, SvgAsset, SvgError, SvgErrorKind, SvgPicture, VectorPicture,
};

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const NONE: [u8; 4] = [0; 4];

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let start = ((y * width + x) * 4) as usize;
    pixels[start..start + 4].try_into().unwrap()
}

/// A 16 x 16 picture of `body`, rasterised one pixel per unit.
fn draw(body: &str) -> Vec<u8> {
    draw_with("", body)
}

fn draw_with(root: &str, body: &str) -> Vec<u8> {
    let source = format!(r##"<svg viewBox="0 0 16 16" {root}>{body}</svg>"##);
    match SvgPicture::parse(&source) {
        Ok(picture) => picture.rasterise(16, 16),
        Err(error) => panic!("{error}\n{source}"),
    }
}

fn refuse(source: &str) -> SvgError {
    match SvgPicture::parse(source) {
        Ok(_) => panic!("accepted: {source}"),
        Err(error) => error,
    }
}

fn refuse_body(body: &str) -> SvgErrorKind {
    refuse(&format!(r##"<svg viewBox="0 0 16 16">{body}</svg>"##)).kind
}

fn close(actual: [u8; 4], expected: [u8; 4], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance)
}

// ---------------------------------------------------------------- shapes

#[test]
fn every_basic_shape_is_drawn() {
    let pixels = draw(r##"<rect x="2" y="1" width="4" height="3" fill="red"/>"##);
    for y in 0..16 {
        for x in 0..16 {
            let inside = (2..6).contains(&x) && (1..4).contains(&y);
            assert_eq!(at(&pixels, 16, x, y), if inside { RED } else { NONE });
        }
    }

    let pixels = draw(r##"<circle cx="8" cy="8" r="6" fill="red"/>"##);
    assert_eq!(at(&pixels, 16, 8, 8), RED);
    assert_eq!(at(&pixels, 16, 8, 3), RED);
    assert_eq!(at(&pixels, 16, 1, 1), NONE);
    let edge = at(&pixels, 16, 3, 4)[3];
    assert!((1..255).contains(&edge), "an anti-aliased edge, was {edge}");

    let pixels = draw(r##"<ellipse cx="8" cy="8" rx="7" ry="3" fill="red"/>"##);
    assert_eq!(at(&pixels, 16, 2, 8), RED);
    assert_eq!(at(&pixels, 16, 8, 3), NONE);

    let pixels = draw(r##"<line x1="2" y1="8" x2="14" y2="8" stroke="red" stroke-width="4"/>"##);
    assert_eq!(at(&pixels, 16, 8, 6), RED);
    assert_eq!(at(&pixels, 16, 8, 9), RED);
    assert_eq!(at(&pixels, 16, 8, 5), NONE);
    assert_eq!(at(&pixels, 16, 1, 8), NONE, "butt caps");

    // A polygon closes; a polyline is filled as if it did and stroked open.
    let pixels = draw(r##"<polygon points="2,2 14,2 14,14 2,14" fill="red"/>"##);
    assert_eq!(at(&pixels, 16, 8, 8), RED);
    assert_eq!(at(&pixels, 16, 1, 8), NONE);
    let pixels =
        draw(r##"<polyline points="2 2 14 2 14 14" fill="none" stroke="red" stroke-width="2"/>"##);
    assert_eq!(at(&pixels, 16, 8, 1), RED);
    assert_eq!(at(&pixels, 16, 14, 8), RED);
    assert_eq!(at(&pixels, 16, 8, 8), NONE, "not closed back to the start");

    let pixels = draw(r##"<path d="M2 2h12v12h-12z" fill="red"/>"##);
    assert_eq!(at(&pixels, 16, 8, 8), RED);
}

#[test]
fn a_rounded_rect_follows_the_svg_rules_for_its_radii() {
    // One radius stands for both; each is at most half its side.
    let one = draw(r##"<rect x="0" y="0" width="16" height="16" rx="8" fill="red"/>"##);
    let both = draw(r##"<rect x="0" y="0" width="16" height="16" rx="8" ry="8" fill="red"/>"##);
    let over = draw(r##"<rect x="0" y="0" width="16" height="16" ry="99" fill="red"/>"##);
    let circle = draw(r##"<circle cx="8" cy="8" r="8" fill="red"/>"##);
    assert_eq!(one, both);
    assert_eq!(one, over);
    assert_eq!(at(&one, 16, 0, 0), NONE);
    assert_eq!(at(&one, 16, 8, 8), RED);
    let differing = one
        .iter()
        .zip(&circle)
        .filter(|(a, b)| a.abs_diff(**b) > 2)
        .count();
    assert_eq!(differing, 0, "a square with half-side radii is a circle");
}

#[test]
fn shapes_without_size_draw_nothing_and_are_not_errors() {
    for body in [
        r##"<rect width="0" height="4" fill="red"/>"##,
        r##"<rect fill="red"/>"##,
        r##"<circle cx="8" cy="8" fill="red"/>"##,
        r##"<ellipse rx="4" fill="red"/>"##,
        r##"<path fill="red"/>"##,
        r##"<path d="" fill="red"/>"##,
        r##"<polygon points="" fill="red"/>"##,
        r##"<polyline points="4 4" stroke="red"/>"##,
    ] {
        assert!(draw(body).iter().all(|byte| *byte == 0), "{body}");
    }
}

// ----------------------------------------------------------- presentation

#[test]
fn svg_defaults_are_a_black_fill_and_no_stroke() {
    let pixels = draw(r##"<rect x="4" y="4" width="8" height="8"/>"##);
    assert_eq!(at(&pixels, 16, 8, 8), [0, 0, 0, 255]);
    assert_eq!(at(&pixels, 16, 3, 8), NONE);
}

#[test]
fn presentation_is_inherited_through_groups_and_overridden() {
    let pixels = draw(
        r##"<g fill="red" stroke="blue" stroke-width="2">
             <rect x="2" y="2" width="4" height="4"/>
             <g fill="none"><rect x="9" y="2" width="4" height="4"/></g>
             <rect x="2" y="9" width="4" height="4" stroke="none" fill="blue"/>
           </g>"##,
    );
    assert_eq!(at(&pixels, 16, 4, 4), RED);
    assert_eq!(at(&pixels, 16, 1, 4), BLUE, "the inherited stroke");
    assert_eq!(at(&pixels, 16, 11, 4), NONE, "fill turned off by the group");
    assert_eq!(at(&pixels, 16, 8, 4), BLUE);
    assert_eq!(at(&pixels, 16, 4, 11), BLUE);
    assert_eq!(at(&pixels, 16, 1, 11), NONE, "stroke turned off");

    // On the root element too.
    let pixels = draw_with(
        r##"fill="none" stroke="red" stroke-width="2""##,
        r##"<rect x="4" y="4" width="8" height="8"/>"##,
    );
    assert_eq!(at(&pixels, 16, 4, 8), RED);
    assert_eq!(at(&pixels, 16, 8, 8), NONE);
}

#[test]
fn a_style_attribute_wins_over_presentation_attributes() {
    let pixels = draw(
        r##"<rect x="4" y="4" width="8" height="8" style="fill: blue; stroke:red;stroke-width:2 !important;" fill="red"/>"##,
    );
    assert_eq!(at(&pixels, 16, 8, 8), BLUE);
    assert_eq!(at(&pixels, 16, 3, 8), RED);
    // Wherever it is written.
    let pixels = draw(r##"<rect style="fill:blue" fill="red" width="16" height="16"/>"##);
    assert_eq!(at(&pixels, 16, 8, 8), BLUE);
}

#[test]
fn colours_are_read_in_every_form() {
    let fill = |color: &str| {
        let body = format!(r##"<rect width="16" height="16" fill="{color}"/>"##);
        at(&draw(&body), 16, 8, 8)
    };
    assert_eq!(fill("#f00"), RED);
    assert_eq!(fill("#F00F"), RED);
    assert_eq!(fill("#ff0000"), RED);
    assert_eq!(fill("#FF0000ff"), RED);
    assert_eq!(fill("rgb(255, 0, 0)"), RED);
    assert_eq!(fill("rgb(255 0 0)"), RED);
    assert_eq!(fill("rgb(100%, 0%, 0%)"), RED);
    assert_eq!(fill("rgba(255,0,0,1)"), RED);
    assert_eq!(fill("RED"), RED);
    assert_eq!(fill("rebeccapurple"), [0x66, 0x33, 0x99, 255]);
    assert_eq!(fill("#F6C445"), [0xF6, 0xC4, 0x45, 255]);
    assert_eq!(fill("transparent"), NONE);
    assert_eq!(fill("none"), NONE);
    // Alpha: premultiplied in the result.
    assert_eq!(fill("#ff000080"), [128, 0, 0, 128]);
    assert_eq!(fill("rgba(255, 0, 0, 0.5)"), [128, 0, 0, 128]);
    assert_eq!(fill("rgb(255 0 0 / 50%)"), [128, 0, 0, 128]);
    assert_eq!(fill("#f008"), [136, 0, 0, 136]);
}

#[test]
fn opacities_multiply_and_blend_the_encoded_values() {
    let pixels = draw(
        r##"<rect width="16" height="8" fill="red"/>
           <rect width="8" height="16" fill="blue" fill-opacity="0.5"/>"##,
    );
    // Half blue over red, as a browser shows it.
    assert_eq!(at(&pixels, 16, 2, 2), [128, 0, 128, 255]);
    assert_eq!(at(&pixels, 16, 2, 12), [0, 0, 128, 128]);

    let pixels = draw(
        r##"<rect x="4" y="4" width="8" height="8" fill="red" stroke="blue" stroke-width="4"
                 stroke-opacity="50%"/>"##,
    );
    assert_eq!(at(&pixels, 16, 8, 8), RED);
    assert_eq!(
        at(&pixels, 16, 5, 8),
        [128, 0, 128, 255],
        "stroke over fill"
    );
    assert_eq!(at(&pixels, 16, 3, 8), [0, 0, 128, 128]);

    // `opacity` fades fill and stroke as one.
    let pixels = draw(
        r##"<rect x="4" y="4" width="8" height="8" fill="red" stroke="blue" stroke-width="4"
                 opacity="0.5"/>"##,
    );
    assert_eq!(at(&pixels, 16, 5, 8), [0, 0, 128, 128]);
    assert_eq!(at(&pixels, 16, 8, 8), [128, 0, 0, 128]);
}

#[test]
fn a_faded_group_does_not_show_its_children_through_each_other() {
    let pixels = draw(
        r##"<g opacity="0.5">
             <rect x="0" y="0" width="10" height="16" fill="red"/>
             <rect x="6" y="0" width="10" height="16" fill="blue"/>
           </g>"##,
    );
    assert_eq!(at(&pixels, 16, 2, 8), [128, 0, 0, 128]);
    assert_eq!(
        at(&pixels, 16, 8, 8),
        [0, 0, 128, 128],
        "the overlap: blue alone"
    );
    assert_eq!(at(&pixels, 16, 13, 8), [0, 0, 128, 128]);

    // Nested: a quarter.
    let pixels = draw(
        r##"<g opacity="0.5"><g opacity="0.5">
             <rect width="10" height="16" fill="red"/>
             <rect x="6" width="10" height="16" fill="blue"/>
           </g><rect y="12" width="16" height="4" fill="red"/></g>"##,
    );
    assert_eq!(at(&pixels, 16, 8, 4), [0, 0, 64, 64]);
    assert_eq!(at(&pixels, 16, 8, 14), [128, 0, 0, 128]);
}

#[test]
fn fill_rule_evenodd_cuts_a_hole() {
    let path = "M2 2h12v12h-12z M5 5h6v6h-6z";
    let nonzero = draw(&format!(r##"<path d="{path}" fill="red"/>"##));
    let evenodd = draw(&format!(
        r##"<path d="{path}" fill="red" fill-rule="evenodd"/>"##
    ));
    assert_eq!(at(&nonzero, 16, 8, 8), RED);
    assert_eq!(at(&evenodd, 16, 8, 8), NONE);
    assert_eq!(at(&evenodd, 16, 3, 8), RED);
}

#[test]
fn strokes_take_caps_joins_miter_limits_and_dashes() {
    // Caps.
    let line = |cap: &str| {
        draw(&format!(
            r##"<line x1="4" y1="8" x2="12" y2="8" stroke="red" stroke-width="4" stroke-linecap="{cap}"/>"##
        ))
    };
    assert_eq!(at(&line("butt"), 16, 3, 8), NONE);
    assert_eq!(at(&line("square"), 16, 2, 6), RED);
    assert_eq!(at(&line("round"), 16, 3, 8), RED);
    let corner = at(&line("round"), 16, 2, 6)[3];
    assert!(
        (1..255).contains(&corner),
        "a round cap's corner, was {corner}"
    );

    // A right-angle corner: a mitre fills the outer corner pixel, a bevel
    // cuts it, and a mitre limit of 1 turns the mitre into a bevel.
    let corner = |extra: &str| {
        draw(&format!(
            r##"<path d="M4 14V4h10" fill="none" stroke="red" stroke-width="4" {extra}/>"##
        ))
    };
    assert_eq!(at(&corner(""), 16, 2, 2), RED);
    assert_ne!(at(&corner(r##"stroke-linejoin="bevel""##), 16, 2, 2), RED);
    assert_ne!(at(&corner(r##"stroke-miterlimit="1""##), 16, 2, 2), RED);
    let round = at(&corner(r##"stroke-linejoin="round""##), 16, 2, 2)[3];
    assert!((1..255).contains(&round));

    // Dashes: 4 on, 4 off, from the start of the line.
    let dashed = |extra: &str| {
        draw(&format!(
            r##"<line x1="0" y1="8" x2="16" y2="8" stroke="red" stroke-width="2" {extra}/>"##
        ))
    };
    let row =
        |pixels: &[u8]| -> Vec<bool> { (0..16).map(|x| at(pixels, 16, x, 8) == RED).collect() };
    let pattern = |text: &str| -> Vec<bool> { text.bytes().map(|byte| byte == b'#').collect() };
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="4""##)),
        pattern("####....####....")
    );
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="4 4""##)),
        pattern("####....####....")
    );
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="4,2""##)),
        pattern("####..####..####")
    );
    // An offset starts that far into the pattern; a negative one wraps.
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="4" stroke-dashoffset="2""##)),
        pattern("##....####....##")
    );
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="4" stroke-dashoffset="-2""##)),
        pattern("..####....####..")
    );
    assert_eq!(
        row(&dashed(r##"style="stroke-dasharray: 2 6""##)),
        pattern("##......##......")
    );
    // An odd list repeats; `none` and all zeros are solid.
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="2 1 1""##)),
        pattern("##.#..#.##.#..#.")
    );
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="none""##)),
        pattern("################")
    );
    assert_eq!(
        row(&dashed(r##"stroke-dasharray="0 0""##)),
        pattern("################")
    );
}

#[test]
fn display_none_and_hidden_things_are_not_drawn() {
    let pixels = draw(
        r##"<rect width="8" height="8" fill="red" display="none"/>
           <g display="none"><rect x="8" width="8" height="8" fill="red"/></g>
           <g visibility="hidden">
             <rect y="8" width="8" height="8" fill="red"/>
             <rect x="8" y="8" width="8" height="8" fill="blue" visibility="visible"/>
           </g>
           <rect width="4" height="4" style="display:none" fill="red"/>"##,
    );
    assert_eq!(at(&pixels, 16, 4, 4), NONE);
    assert_eq!(at(&pixels, 16, 12, 4), NONE);
    assert_eq!(at(&pixels, 16, 4, 12), NONE);
    assert_eq!(at(&pixels, 16, 12, 12), BLUE, "made visible again");
}

// ---------------------------------------------------------- currentColor

#[test]
fn current_color_is_given_when_drawn_unless_the_file_sets_color() {
    let picture = SvgPicture::parse(
        r##"<svg viewBox="0 0 16 16">
             <rect width="8" height="16" fill="currentColor"/>
             <g color="blue"><rect x="8" width="8" height="16" fill="currentColor"/></g>
           </svg>"##,
    )
    .unwrap();
    assert!(picture.uses_current_color());
    let black = picture.rasterise(16, 16);
    assert_eq!(at(&black, 16, 4, 8), [0, 0, 0, 255], "black unless set");
    assert_eq!(at(&black, 16, 12, 8), BLUE);

    let red = picture.with_current_color(Color::rgb(1.0, 0.0, 0.0));
    let pixels = red.rasterise(16, 16);
    assert_eq!(at(&pixels, 16, 4, 8), RED);
    assert_eq!(at(&pixels, 16, 12, 8), BLUE, "the file's own colour stays");
    assert_ne!(picture.content_hash(), red.content_hash());
    assert_ne!(picture, red);
    assert_eq!(red, picture.with_current_color(Color::rgb(1.0, 0.0, 0.0)));

    // A file that never says currentColor is the same picture in any.
    let fixed = SvgPicture::parse(r##"<svg viewBox="0 0 4 4"><rect width="4" height="4"/></svg>"##)
        .unwrap();
    assert!(!fixed.uses_current_color());
    assert_eq!(
        fixed.content_hash(),
        fixed.with_current_color(Color::WHITE).content_hash()
    );
}

#[test]
fn single_ink_pictures_are_recognised_and_give_a_mask() {
    let single = |source: &str| SvgPicture::parse(source).unwrap().is_single_ink();
    // The usual outline icon.
    assert!(single(
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12l5 5 9-10"/></svg>"##
    ));
    // The usual filled icon: no paint at all, or one literal colour.
    assert!(single(
        r##"<svg viewBox="0 0 24 24"><path d="M3 3h18v18H3z"/></svg>"##
    ));
    assert!(single(
        r##"<svg viewBox="0 0 24 24"><path d="M3 3h18v18H3z" fill="#FFF" opacity=".5"/><circle r="3" fill="#ffffff"/></svg>"##
    ));
    // Two colours, a colour beside currentColor, a gradient: not one ink.
    assert!(!single(
        r##"<svg viewBox="0 0 24 24"><path d="M3 3h18v18H3z" fill="red"/><circle r="3" fill="blue"/></svg>"##
    ));
    assert!(!single(
        r##"<svg viewBox="0 0 24 24"><path d="M3 3h18v18H3z" fill="red"/><circle r="3" fill="currentColor"/></svg>"##
    ));
    assert!(!single(
        r##"<svg viewBox="0 0 24 24"><linearGradient id="g"><stop stop-color="red"/></linearGradient><circle r="3" fill="url(#g)"/></svg>"##
    ));

    let picture = SvgPicture::parse(
        r##"<svg viewBox="0 0 16 16"><rect x="4" y="4" width="8" height="8" fill="currentColor" opacity="0.5"/></svg>"##,
    )
    .unwrap();
    let mask = picture.rasterise_mask(16, 16);
    assert_eq!(mask.len(), 256);
    assert_eq!(mask[8 * 16 + 8], 128);
    assert_eq!(mask[0], 0);
}

// ------------------------------------------------------------- transforms

#[test]
fn transform_lists_apply_the_last_function_first() {
    // 4 x 2 at the origin, turned a quarter clockwise (now x -2..0,
    // y 0..4), then moved right by 8.
    let pixels =
        draw(r##"<rect width="4" height="2" fill="red" transform="translate(8 0) rotate(90)"/>"##);
    for y in 0..16 {
        for x in 0..16 {
            let inside = (6..8).contains(&x) && (0..4).contains(&y);
            assert_eq!(
                at(&pixels, 16, x, y),
                if inside { RED } else { NONE },
                "{x},{y}"
            );
        }
    }
    // The other order turns the move as well: down by 8.
    let pixels =
        draw(r##"<rect width="4" height="2" fill="red" transform="rotate(90),translate(8,0)"/>"##);
    assert_eq!(at(&pixels, 16, 0, 9), NONE);
    assert!(pixels.iter().all(|byte| *byte == 0), "x is -2..0: outside");

    let moved = |transform: &str| {
        draw(&format!(
            r##"<rect width="4" height="4" fill="red" transform="{transform}"/>"##
        ))
    };
    let plain = draw(r##"<rect x="8" y="4" width="4" height="4" fill="red"/>"##);
    assert_eq!(moved("translate(8 4)"), plain);
    assert_eq!(moved("translate(8,4)"), plain);
    assert_eq!(moved("matrix(1 0 0 1 8 4)"), plain);
    assert_eq!(moved("translate(4) translate(4 4)"), plain);
    assert_eq!(moved("translate(12 4) scale(-1 1)"), plain);
    assert_eq!(moved("scale(2) translate(4 2) scale(.5)"), plain);
    // About a point: a half turn about (8, 4) of the square at the origin.
    assert_eq!(
        moved("rotate(180 8 4)"),
        draw(r##"<rect x="12" y="4" width="4" height="4" fill="red"/>"##)
    );
    // A skew: each row moves right by its height.
    let skewed = moved("skewX(45)");
    assert_eq!(at(&skewed, 16, 4, 3), RED);
    assert_eq!(at(&skewed, 16, 1, 3), NONE);
    let skewed = moved("skewY(45)");
    assert_eq!(at(&skewed, 16, 3, 4), RED);
    assert_eq!(at(&skewed, 16, 3, 1), NONE);
}

#[test]
fn group_transforms_nest_and_scale_strokes() {
    let pixels = draw(
        r##"<g transform="translate(8 8)"><g transform="scale(2)">
             <line x1="-3" y1="0" x2="3" y2="0" stroke="red" stroke-width="1"/>
           </g></g>"##,
    );
    // A stroke 1 wide under a scale of 2: rows 7 and 8, columns 2 to 13.
    assert_eq!(at(&pixels, 16, 8, 7), RED);
    assert_eq!(at(&pixels, 16, 8, 8), RED);
    assert_eq!(at(&pixels, 16, 8, 6), NONE);
    assert_eq!(at(&pixels, 16, 2, 8), RED);
    assert_eq!(at(&pixels, 16, 1, 8), NONE);
}

// ------------------------------------------------------------------ clips

#[test]
fn a_clip_path_cuts_what_refers_to_it() {
    let pixels = draw(
        r##"<defs><clipPath id="right"><rect x="8" width="8" height="16"/></clipPath></defs>
           <rect y="6" width="16" height="4" fill="red" clip-path="url(#right)"/>"##,
    );
    assert_eq!(at(&pixels, 16, 4, 8), NONE);
    assert_eq!(at(&pixels, 16, 12, 8), RED);

    // Defined after it is used, and quoted.
    let pixels = draw(
        r##"<rect y="6" width="16" height="4" fill="red" clip-path="url('#right')"/>
           <clipPath id="right"><rect x="8" width="8" height="16"/></clipPath>"##,
    );
    assert_eq!(at(&pixels, 16, 4, 8), NONE);
    assert_eq!(at(&pixels, 16, 12, 8), RED);
}

#[test]
fn a_clip_is_the_union_of_its_shapes_each_with_its_transform_and_rule() {
    let pixels = draw(
        r##"<clipPath id="two">
             <rect width="4" height="16"/>
             <rect width="4" height="16" transform="translate(12 0)"/>
             <path d="M6 6h4v4h-4z M7 7h2v2h-2z" clip-rule="evenodd"/>
           </clipPath>
           <rect width="16" height="16" fill="red" clip-path="url(#two)"/>"##,
    );
    assert_eq!(at(&pixels, 16, 2, 8), RED);
    assert_eq!(at(&pixels, 16, 13, 8), RED);
    assert_eq!(at(&pixels, 16, 5, 8), NONE);
    assert_eq!(at(&pixels, 16, 6, 6), RED);
    assert_eq!(at(&pixels, 16, 7, 7), NONE, "the hole of the even-odd part");
    // An empty clip cuts everything away.
    let pixels = draw(
        r##"<clipPath id="none"/><rect width="16" height="16" fill="red" clip-path="url(#none)"/>"##,
    );
    assert!(pixels.iter().all(|byte| *byte == 0));
}

#[test]
fn a_clip_lives_in_the_space_of_the_element_that_uses_it() {
    let clip = r##"<clipPath id="left"><rect width="8" height="16"/></clipPath>"##;
    // On a group: the child's own transform does not move the clip.
    let pixels = draw(&format!(
        r##"{clip}<g clip-path="url(#left)">
             <rect y="6" width="16" height="4" fill="red" transform="translate(4 0)"/>
           </g>"##
    ));
    assert_eq!(at(&pixels, 16, 3, 8), NONE, "the rect moved right");
    assert_eq!(at(&pixels, 16, 6, 8), RED);
    assert_eq!(at(&pixels, 16, 10, 8), NONE, "the clip stayed");
    // On the element with the transform: the clip moves with it.
    let pixels = draw(&format!(
        r##"{clip}<rect y="6" width="16" height="4" fill="red" transform="translate(4 0)"
                  clip-path="url(#left)"/>"##
    ));
    assert_eq!(at(&pixels, 16, 3, 8), NONE);
    assert_eq!(at(&pixels, 16, 10, 8), RED);
    assert_eq!(at(&pixels, 16, 13, 8), NONE);
}

#[test]
fn clips_on_a_group_and_on_its_child_both_apply() {
    let pixels = draw(
        r##"<clipPath id="left"><rect width="10" height="16"/></clipPath>
           <clipPath id="top"><rect width="16" height="10"/></clipPath>
           <g clip-path="url(#left)">
             <rect width="16" height="16" fill="red" clip-path="url(#top)"/>
           </g>
           <g clip-path="url(#top)" opacity="0.5">
             <rect x="12" width="2" height="16" fill="blue"/>
             <rect x="13" width="2" height="16" fill="blue"/>
           </g>"##,
    );
    assert_eq!(at(&pixels, 16, 4, 4), RED);
    assert_eq!(at(&pixels, 16, 4, 12), NONE);
    assert_eq!(at(&pixels, 16, 11, 4), NONE);
    assert_eq!(
        at(&pixels, 16, 13, 4),
        [0, 0, 128, 128],
        "a clipped, faded group"
    );
    assert_eq!(at(&pixels, 16, 13, 12), NONE);
}

// -------------------------------------------------------------- gradients

#[test]
fn a_linear_gradient_runs_across_the_bounding_box_by_default() {
    let pixels = draw(
        r##"<linearGradient id="g"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient>
           <rect x="0" y="4" width="16" height="8" fill="url(#g)"/>"##,
    );
    // Pixel centres: (x + 0.5) / 16 of the way.
    for x in 0..16 {
        let expected = ((x as f32 + 0.5) / 16.0 * 255.0).round() as u8;
        let pixel = at(&pixels, 16, x, 8);
        assert!(
            close(pixel, [expected, expected, expected, 255], 1),
            "{x}: {pixel:?}"
        );
    }
    assert_eq!(at(&pixels, 16, 8, 2), NONE);

    // Top to bottom, in percentages, over a box that is not at the origin.
    let pixels = draw(
        r##"<linearGradient id="g" x1="0%" y1="0%" x2="0%" y2="100%">
             <stop offset="0%" stop-color="red"/><stop offset="100%" stop-color="blue"/>
           </linearGradient>
           <rect x="4" y="8" width="8" height="8" fill="url(#g)"/>"##,
    );
    assert!(close(at(&pixels, 16, 8, 8), [239, 0, 16, 255], 1));
    assert!(close(at(&pixels, 16, 8, 15), [16, 0, 239, 255], 1));
}

#[test]
fn gradients_in_user_space_take_a_transform_and_spread() {
    let gradient = |extra: &str| {
        draw(&format!(
            r##"<linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="8" y2="0" {extra}>
                 <stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/>
               </linearGradient>
               <rect width="16" height="16" fill="url(#g)"/>"##
        ))
    };
    let level = |pixels: &[u8], x: u32| at(pixels, 16, x, 8)[0];
    let pad = gradient("");
    assert!(level(&pad, 3).abs_diff(112) <= 1);
    assert_eq!(level(&pad, 12), 255, "padded with the last stop");
    let repeat = gradient(r##"spreadMethod="repeat""##);
    assert!(level(&repeat, 11).abs_diff(112) <= 1);
    let reflect = gradient(r##"spreadMethod="reflect""##);
    assert!(level(&reflect, 12).abs_diff(112) <= 1, "mirrored about 8");
    // Moved right by 8: black until there.
    let moved = gradient(r##"gradientTransform="translate(8 0)""##);
    assert_eq!(level(&moved, 4), 0);
    assert!(level(&moved, 11).abs_diff(112) <= 1);
    // Turned a quarter: it now runs down.
    let turned = gradient(r##"gradientTransform="rotate(90)""##);
    assert!(at(&turned, 16, 12, 3)[0].abs_diff(112) <= 1);
    assert_eq!(at(&turned, 16, 3, 12)[0], 255);
}

#[test]
fn a_radial_gradient_runs_from_its_focus_to_its_circle() {
    let pixels = draw(
        r##"<radialGradient id="g"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/></radialGradient>
           <rect width="16" height="16" fill="url(#g)"/>"##,
    );
    // Centre (8, 8), radius 8: the pixel beside the centre is nearly
    // white, one 4 away is half grey, the corner is black.
    assert!(at(&pixels, 16, 8, 8)[0] > 230);
    assert!(at(&pixels, 16, 12, 8)[0].abs_diff(111) <= 3);
    assert_eq!(at(&pixels, 16, 0, 0), [0, 0, 0, 255]);
    assert_eq!(
        at(&pixels, 16, 15, 8)[0],
        at(&pixels, 16, 8, 15)[0],
        "round"
    );

    // In user space, with a focus to the left: white is there, not at
    // the centre.
    let pixels = draw(
        r##"<radialGradient id="g" gradientUnits="userSpaceOnUse" cx="8" cy="8" r="8" fx="3" fy="8">
             <stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#000"/>
           </radialGradient>
           <rect width="16" height="16" fill="url(#g)"/>"##,
    );
    assert!(at(&pixels, 16, 3, 8)[0] > 215);
    assert!(at(&pixels, 16, 8, 8)[0] < at(&pixels, 16, 3, 8)[0]);
    assert!(
        at(&pixels, 16, 1, 8)[0] < 140,
        "close to the circle on the near side"
    );
}

#[test]
fn gradient_stops_take_opacity_styles_and_current_color() {
    // SVG interpolates colour and alpha separately: halfway between
    // opaque red and transparent blue is half-transparent purple.
    let pixels = draw(
        r##"<linearGradient id="g" gradientUnits="userSpaceOnUse" x1="0" x2="16">
             <stop offset="0" stop-color="red"/>
             <stop offset="1" style="stop-color: blue; stop-opacity: 0"/>
           </linearGradient>
           <rect width="16" height="16" fill="url(#g)"/>"##,
    );
    // At x = 8.5 of 16: alpha 0.469, red 0.469, blue 0.531, premultiplied.
    assert!(
        close(at(&pixels, 16, 8, 8), [56, 0, 64, 120], 2),
        "{:?}",
        at(&pixels, 16, 8, 8)
    );

    // `fill-opacity` fades a gradient; a single stop is a flat colour;
    // offsets that go backwards are raised to the one before.
    let pixels = draw(
        r##"<linearGradient id="one"><stop offset="0.5" stop-color="red"/></linearGradient>
           <rect width="16" height="8" fill="url(#one)" fill-opacity="0.5"/>
           <linearGradient id="hard" gradientUnits="userSpaceOnUse" x2="16">
             <stop offset="0.5" stop-color="red"/><stop offset="0.25" stop-color="blue"/>
           </linearGradient>
           <rect y="8" width="16" height="8" fill="url(#hard)"/>"##,
    );
    assert_eq!(at(&pixels, 16, 2, 4), [128, 0, 0, 128]);
    assert_eq!(at(&pixels, 16, 14, 4), [128, 0, 0, 128]);
    assert_eq!(at(&pixels, 16, 6, 12), RED);
    assert_eq!(at(&pixels, 16, 10, 12), BLUE, "a hard edge at one half");

    let picture = SvgPicture::parse(
        r##"<svg viewBox="0 0 16 16"><linearGradient id="g"><stop stop-color="currentColor"/></linearGradient>
           <rect width="16" height="16" fill="url(#g)"/></svg>"##,
    )
    .unwrap();
    assert!(picture.uses_current_color());
    let pixels = picture
        .with_current_color(Color::rgb(1.0, 0.0, 0.0))
        .rasterise(16, 16);
    assert_eq!(at(&pixels, 16, 8, 8), RED);
}

#[test]
fn gradients_inherit_from_the_gradient_they_refer_to() {
    let pixels = draw(
        r##"<linearGradient id="stops"><stop offset="0" stop-color="red"/><stop offset="1" stop-color="blue"/></linearGradient>
           <linearGradient id="down" xlink:href="#stops" x1="0" y1="0" x2="0" y2="1"/>
           <linearGradient id="again" href="#down"/>
           <radialGradient id="round" href="#down" r="0.5"/>
           <rect width="8" height="16" fill="url(#again)"/>
           <rect x="8" width="8" height="16" fill="url(#round)"/>"##,
    );
    // Stops through two links, the vector through one.
    assert!(at(&pixels, 16, 4, 0)[0] > 230);
    assert!(at(&pixels, 16, 4, 15)[2] > 230);
    assert_eq!(at(&pixels, 16, 1, 8), at(&pixels, 16, 6, 8), "it runs down");
    // A radial gradient takes the stops but not a linear one's vector.
    assert!(at(&pixels, 16, 12, 8)[0] > 200);
    assert!(at(&pixels, 16, 12, 0)[2] > 230);

    // A gradient without stops paints nothing; a stroke can be a gradient.
    let pixels = draw(
        r##"<linearGradient id="empty"/><rect width="16" height="16" fill="url(#empty)"/>
           <linearGradient id="g"><stop stop-color="red"/></linearGradient>
           <line x1="0" y1="8" x2="16" y2="8" stroke="url(#g)" stroke-width="4"/>"##,
    );
    assert_eq!(at(&pixels, 16, 8, 2), NONE);
    // A horizontal line has no bounding-box height: in bounding-box
    // units it is not painted, as SVG says.
    assert_eq!(at(&pixels, 16, 8, 8), NONE);
    let pixels = draw(
        r##"<linearGradient id="g" gradientUnits="userSpaceOnUse" x2="16"><stop stop-color="red"/></linearGradient>
           <line x1="0" y1="8" x2="16" y2="8" stroke="url(#g)" stroke-width="4"/>"##,
    );
    assert_eq!(at(&pixels, 16, 8, 8), RED);
}

// ------------------------------------------------------------ the document

#[test]
fn the_size_comes_from_width_and_height_or_the_view_box() {
    let size = |root: &str| {
        let picture = SvgPicture::parse(&format!("<svg {root}/>")).unwrap();
        (picture.width(), picture.height(), picture.view_box())
    };
    assert_eq!(
        size(r##"viewBox="0 0 24 12""##),
        (24.0, 12.0, [0.0, 0.0, 24.0, 12.0])
    );
    assert_eq!(
        size(r##"width="250" height="250px" viewBox="0 0 280 280""##),
        (250.0, 250.0, [0.0, 0.0, 280.0, 280.0])
    );
    assert_eq!(
        size(r##"width="64" height="32""##),
        (64.0, 32.0, [0.0, 0.0, 64.0, 32.0])
    );
    assert_eq!(
        size(r##"viewBox="-8,-8,16 16""##).2,
        [-8.0, -8.0, 16.0, 16.0]
    );
    // Sizes that are not pixels say nothing: the view box stands in.
    assert_eq!(
        size(r##"width="100%" height="1em" viewBox="0 0 24 24""##).0,
        24.0
    );

    assert_eq!(refuse("<svg/>").kind, SvgErrorKind::NoSize);
    assert_eq!(
        refuse(r##"<svg width="100%" height="24"/>"##).kind,
        SvgErrorKind::NoSize
    );
    assert_eq!(
        refuse(r##"<g viewBox="0 0 1 1"/>"##).kind,
        SvgErrorKind::NoSize
    );
    assert_eq!(refuse("").kind, SvgErrorKind::NoSize);
    assert!(matches!(
        refuse(r##"<svg viewBox="0 0 0 16"/>"##).kind,
        SvgErrorKind::UnsupportedValue { .. }
    ));
    assert!(matches!(
        refuse(r##"<svg viewBox="0 0 16"/>"##).kind,
        SvgErrorKind::UnsupportedValue { .. }
    ));
}

#[test]
fn the_view_box_is_fitted_and_centred_and_its_origin_respected() {
    let picture = SvgPicture::parse(
        r##"<svg viewBox="10 10 8 8"><rect x="10" y="10" width="8" height="8" fill="red"/>
             <rect x="2" y="12" width="4" height="4" fill="blue"/></svg>"##,
    )
    .unwrap();
    let pixels = picture.rasterise(16, 8);
    assert_eq!(at(&pixels, 16, 3, 4), NONE);
    assert_eq!(at(&pixels, 16, 4, 4), RED);
    assert_eq!(at(&pixels, 16, 11, 4), RED);
    assert_eq!(at(&pixels, 16, 12, 4), NONE);
    // What lies outside the view box but inside the image shows, as in a
    // browser: the blue square is 8 to 4 units left of the box.
    let pixels = picture.rasterise(32, 8);
    assert_eq!(at(&pixels, 32, 5, 4), BLUE);
    assert!(picture.rasterise(0, 8).is_empty());
}

#[test]
fn xml_around_the_picture_is_read() {
    let picture = SvgPicture::parse(concat!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n",
        "<!-- Created with a drawing program -->\n",
        "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n",
        "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink=\"http://www.w3.org/1999/xlink\"\n",
        "     xmlns:inkscape=\"http://www.inkscape.org/namespaces/inkscape\"\n",
        "     xmlns:sodipodi=\"http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd\"\n",
        "     version=\"1.1\" id=\"svg8\" viewBox = '0 0 16 16' inkscape:version=\"1.2\"\n",
        "     sodipodi:docname=\"a &amp; b.svg\" xml:space=\"preserve\" class=\"icon\" data-name=\"x\" aria-hidden=\"true\">\n",
        "  <title>A &lt;picture&gt;</title><desc>Text & more is not checked here</desc>\n",
        "  <metadata><rdf:RDF><cc:Work rdf:about=\"\"><dc:format>image/svg+xml</dc:format></cc:Work></rdf:RDF></metadata>\n",
        "  <sodipodi:namedview pagecolor=\"#ffffff\"><inkscape:grid type=\"xygrid\"/></sodipodi:namedview>\n",
        "  <!-- a comment with <tags> and -- dashes -->\n",
        "  <g inkscape:label=\"Layer 1\" inkscape:groupmode=\"layer\" id=\"layer1\">\n",
        "    <rect width=\"16\" height=\"16\" fill=\"&#x72;ed\" sodipodi:nodetypes=\"ccccc\"><title>square</title></rect>\n",
        "    stray text\n",
        "  </g>\n",
        "</svg>\n<!-- trailing -->\n",
    ))
    .unwrap();
    assert_eq!(at(&picture.rasterise(16, 16), 16, 8, 8), RED);
}

#[test]
fn malformed_xml_is_refused_with_its_position() {
    let syntax = |source: &str| match refuse(source).kind {
        SvgErrorKind::Syntax(message) => message,
        other => panic!("{other:?} for {source}"),
    };
    let head = r##"<svg viewBox="0 0 1 1">"##;
    assert!(syntax(&format!("{head}<g>")).contains("ends inside"));
    assert!(syntax(&format!("{head}<g></svg>")).contains("does not match"));
    assert!(syntax(&format!("{head}</g>")).contains("does not match"));
    assert!(syntax(&format!("{head}</svg><svg/>")).contains("second root"));
    assert!(syntax(&format!("{head}</svg>text")).contains("outside the root"));
    assert!(syntax(&format!("text{head}</svg>")).contains("outside the root"));
    assert!(syntax(&format!("{head}<rect width=4/></svg>")).contains("quoted"));
    assert!(syntax(&format!("{head}<rect width/></svg>")).contains("`=`"));
    assert!(syntax(&format!(r##"{head}<rect width="4/></svg>"##)).contains("unclosed attribute"));
    assert!(syntax(&format!(r##"{head}<rect x="1" x="2"/></svg>"##)).contains("twice"));
    assert!(syntax(&format!(r##"{head}<rect x="1"y="2"/></svg>"##)).contains("whitespace"));
    assert!(syntax(&format!(r##"{head}<rect x="a<b"/></svg>"##)).contains("`<`"));
    assert!(syntax(&format!("{head}< rect/></svg>")).contains("element name"));
    assert!(syntax(&format!("{head}<!-- never closed</svg>")).contains("comment"));
    assert!(syntax(&format!("{head}<![CDATA[x]]></svg>")).contains("CDATA"));
    assert!(syntax(&format!(r##"{head}<rect x="&nbsp;"/></svg>"##)).contains("unknown entity"));
    assert!(
        syntax(&format!(r##"{head}<rect x="&#xD800;"/></svg>"##)).contains("character reference")
    );
    assert!(syntax(&format!(r##"{head}<rect x="a & b"/></svg>"##)).contains("entity"));
    assert!(
        syntax("<!DOCTYPE svg [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;\">]><svg viewBox='0 0 1 1'/>")
            .contains("DOCTYPE")
    );

    let error = refuse("<svg viewBox=\"0 0 1 1\">\n  <g>\n    <rect width=4/>\n  </g>\n</svg>");
    assert_eq!((error.line, error.column), (3, 17));
    assert_eq!(error.offset, 46);
    assert!(
        error.to_string().starts_with("SVG line 3, column 17: "),
        "{error}"
    );
}

// ---------------------------------------------------------------- refusals

#[test]
fn elements_outside_the_subset_are_refused_by_name_and_position() {
    for element in [
        "text",
        "use",
        "image",
        "mask",
        "filter",
        "pattern",
        "style",
        "script",
        "symbol",
        "marker",
        "a",
        "switch",
        "foreignObject",
        "svg",
        "animate",
        "animateTransform",
        "set",
        "tspan",
        "view",
        "svg:rect",
        "blink",
    ] {
        let source =
            format!("<svg viewBox=\"0 0 16 16\">\n <g>\n  <{element}></{element}>\n </g>\n</svg>");
        let error = refuse(&source);
        assert_eq!(
            error.kind,
            SvgErrorKind::UnsupportedElement(element.to_owned()),
            "{element}"
        );
        assert_eq!((error.line, error.column), (3, 3), "{element}");
        assert!(
            error
                .to_string()
                .contains(&format!("<{element}> is not supported"))
        );
    }
    // Inside a shape, a clip or a gradient too; and unused in `defs`.
    assert_eq!(
        refuse_body(r##"<rect width="4" height="4"><animate attributeName="x"/></rect>"##),
        SvgErrorKind::UnsupportedElement("animate".into())
    );
    assert_eq!(
        refuse_body(r##"<clipPath id="c"><g><rect width="4" height="4"/></g></clipPath>"##),
        SvgErrorKind::UnsupportedElement("g".into())
    );
    assert_eq!(
        refuse_body(r##"<clipPath id="c"><text>x</text></clipPath>"##),
        SvgErrorKind::UnsupportedElement("text".into())
    );
    assert_eq!(
        refuse_body(r##"<linearGradient id="g"><animate/></linearGradient>"##),
        SvgErrorKind::UnsupportedElement("animate".into())
    );
    assert_eq!(
        refuse_body(r##"<defs><mask id="m"/></defs>"##),
        SvgErrorKind::UnsupportedElement("mask".into())
    );
}

#[test]
fn attributes_outside_the_subset_are_refused_by_name() {
    let attribute = |body: &str| match refuse_body(body) {
        SvgErrorKind::UnsupportedAttribute { element, attribute } => (element, attribute),
        other => panic!("{other:?} for {body}"),
    };
    let named = |element: &str, name: &str| (element.to_owned(), name.to_owned());
    assert_eq!(
        attribute(r##"<path d="M0 0h1" pathLength="10"/>"##),
        named("path", "pathLength")
    );
    assert_eq!(
        attribute(r##"<rect width="1" height="1" onclick="x()"/>"##),
        named("rect", "onclick")
    );
    assert_eq!(
        attribute(r##"<g systemLanguage="en"/>"##),
        named("g", "systemLanguage")
    );
    assert_eq!(
        attribute(r##"<circle r="2" cz="1"/>"##),
        named("circle", "cz")
    );
    assert_eq!(
        attribute(r##"<circle r="2" width="1"/>"##),
        named("circle", "width")
    );
    assert_eq!(
        attribute(r##"<g transform-origin="center"/>"##),
        named("g", "transform-origin")
    );
    assert_eq!(
        attribute(r##"<g style="transform: rotate(45deg)"/>"##),
        named("g", "style: transform")
    );
    assert_eq!(
        attribute(r##"<g style="fill:red; bogus"/>"##),
        named("g", "style: bogus")
    );
    assert_eq!(
        attribute(r##"<g style="--tw-ring: 0"/>"##),
        named("g", "style: --tw-ring")
    );
    assert_eq!(
        attribute(r##"<linearGradient id="g" gradientUnit="x"/>"##),
        named("linearGradient", "gradientUnit")
    );
    assert_eq!(
        attribute(r##"<radialGradient id="g" fr="0.2"/>"##),
        named("radialGradient", "fr")
    );
    assert_eq!(
        attribute(r##"<linearGradient id="g"><stop color="red"/></linearGradient>"##),
        named("stop", "color")
    );
    assert_eq!(
        attribute(r##"<clipPath id="c" clip-path="url(#c)"/>"##),
        named("clipPath", "clip-path")
    );
    assert_eq!(
        attribute(
            r##"<clipPath id="c"/><clipPath id="d"><rect width="1" height="1" clip-path="url(#c)"/></clipPath>"##
        ),
        named("rect", "clip-path")
    );
    let error =
        refuse("<svg viewBox=\"0 0 1 1\">\n<path d=\"M0 0h1\"\n   pathLength=\"10\"/></svg>");
    assert_eq!(
        (error.line, error.column),
        (3, 4),
        "the attribute's position"
    );
}

#[test]
fn values_outside_the_subset_are_refused() {
    let value = |body: &str| match refuse_body(body) {
        SvgErrorKind::UnsupportedValue {
            attribute, value, ..
        } => (attribute, value),
        other => panic!("{other:?} for {body}"),
    };
    let named = |name: &str, value: &str| (name.to_owned(), value.to_owned());
    assert_eq!(
        value(r##"<rect width="1em" height="1"/>"##),
        named("width", "1em")
    );
    assert_eq!(
        value(r##"<rect width="50%" height="1"/>"##),
        named("width", "50%")
    );
    assert_eq!(
        value(r##"<rect width="-4" height="1"/>"##),
        named("width", "-4")
    );
    assert_eq!(value(r##"<circle r="NaN"/>"##), named("r", "NaN"));
    assert_eq!(value(r##"<circle r="inf"/>"##), named("r", "inf"));
    assert_eq!(value(r##"<circle r="1 2"/>"##), named("r", "1 2"));
    assert_eq!(
        value(r##"<circle r="4" fill="salmonish"/>"##),
        named("fill", "salmonish")
    );
    assert_eq!(
        value(r##"<circle r="4" fill="#12345"/>"##),
        named("fill", "#12345")
    );
    assert_eq!(
        value(r##"<circle r="4" fill="hsl(0 100% 50%)"/>"##),
        named("fill", "hsl(0 100% 50%)")
    );
    assert_eq!(
        value(r##"<circle r="4" fill="rgb(1,2)"/>"##),
        named("fill", "rgb(1,2)")
    );
    assert_eq!(
        value(r##"<circle r="4" fill="context-fill"/>"##),
        named("fill", "context-fill")
    );
    assert_eq!(
        value(r##"<circle r="4" stroke-linejoin="arcs"/>"##),
        named("stroke-linejoin", "arcs")
    );
    assert_eq!(
        value(r##"<circle r="4" stroke-linecap="flat"/>"##),
        named("stroke-linecap", "flat")
    );
    assert_eq!(
        value(r##"<circle r="4" stroke-width="-1"/>"##),
        named("stroke-width", "-1")
    );
    assert_eq!(
        value(r##"<circle r="4" stroke-miterlimit="0.5"/>"##),
        named("stroke-miterlimit", "0.5")
    );
    assert_eq!(
        value(r##"<circle r="4" stroke-dasharray="4 -1"/>"##),
        named("stroke-dasharray", "4 -1")
    );
    assert_eq!(
        value(r##"<circle r="4" stroke-dasharray="10%"/>"##),
        named("stroke-dasharray", "10%")
    );
    assert_eq!(
        value(r##"<circle r="4" fill-rule="winding"/>"##),
        named("fill-rule", "winding")
    );
    assert_eq!(
        value(r##"<circle r="4" opacity="half"/>"##),
        named("opacity", "half")
    );
    assert_eq!(
        value(r##"<circle r="4" mask="url(#m)"/>"##),
        named("mask", "url(#m)")
    );
    assert_eq!(
        value(r##"<circle r="4" filter="url(#f)"/>"##),
        named("filter", "url(#f)")
    );
    assert_eq!(
        value(r##"<circle r="4" style="filter: blur(2px)"/>"##),
        named("filter", "blur(2px)")
    );
    assert_eq!(
        value(r##"<path d="M0 0h4" marker-end="url(#a)"/>"##),
        named("marker-end", "url(#a)")
    );
    assert_eq!(
        value(r##"<circle r="4" paint-order="stroke"/>"##),
        named("paint-order", "stroke")
    );
    assert_eq!(
        value(r##"<circle r="4" vector-effect="non-scaling-stroke"/>"##),
        named("vector-effect", "non-scaling-stroke")
    );
    assert_eq!(
        value(r##"<circle r="4" mix-blend-mode="multiply"/>"##),
        named("mix-blend-mode", "multiply")
    );
    assert_eq!(
        value(r##"<g transform="rotate(45deg)"/>"##),
        named("transform", "rotate(45deg)")
    );
    assert_eq!(
        value(r##"<g transform="spin(4)"/>"##),
        named("transform", "spin(4)")
    );
    assert_eq!(
        value(r##"<g transform="translate(1 2 3)"/>"##),
        named("transform", "translate(1 2 3)")
    );
    assert_eq!(
        value(r##"<g transform="scale(2"/>"##),
        named("transform", "scale(2")
    );
    assert_eq!(value(r##"<polygon points="1 2 3"/>"##).0, "points");
    assert_eq!(
        value(r##"<polygon points="1 2 x 4"/>"##),
        named("points", "x 4")
    );
    assert_eq!(
        value(r##"<clipPath id="c" clipPathUnits="objectBoundingBox"/>"##),
        named("clipPathUnits", "objectBoundingBox")
    );
    assert_eq!(
        value(r##"<g id="p"/><circle r="4" fill="url(#p)"/>"##).0,
        "fill",
        "a paint server that is not a gradient"
    );
    assert_eq!(
        value(r##"<rect id="r" width="1" height="1"/><circle r="4" clip-path="url(#r)"/>"##).0,
        "clip-path"
    );
    assert_eq!(
        value(r##"<linearGradient id="g" spreadMethod="mirror"/>"##),
        named("spreadMethod", "mirror")
    );
    assert_eq!(
        value(r##"<linearGradient id="g" x1="left"/>"##),
        named("x1", "left")
    );
    assert_eq!(
        value(r##"<linearGradient id="g"><stop offset="half"/></linearGradient>"##),
        named("offset", "half")
    );
    assert_eq!(
        value(r##"<linearGradient id="g"><stop stop-color="nope"/></linearGradient>"##),
        named("stop-color", "nope")
    );
    assert!(matches!(
        refuse(r##"<svg viewBox="0 0 1 1" preserveAspectRatio="none"/>"##).kind,
        SvgErrorKind::UnsupportedValue { .. }
    ));
    // A long value is shortened in the message.
    let long = "x".repeat(500);
    let (_, shown) = value(&format!(r##"<circle r="4" fill="{long}"/>"##));
    assert!(shown.chars().count() <= 61);
}

#[test]
fn missing_references_and_bad_paths_are_refused() {
    assert_eq!(
        refuse_body(r##"<circle r="4" fill="url(#nowhere)"/>"##),
        SvgErrorKind::UnknownReference {
            element: "circle".into(),
            id: "nowhere".into()
        }
    );
    assert_eq!(
        refuse_body(r##"<g clip-path="url(#nowhere)"/>"##),
        SvgErrorKind::UnknownReference {
            element: "g".into(),
            id: "nowhere".into()
        }
    );
    assert_eq!(
        refuse_body(r##"<linearGradient id="g" href="#nowhere"/>"##),
        SvgErrorKind::UnknownReference {
            element: "linearGradient".into(),
            id: "nowhere".into()
        }
    );
    // As SVG says, a fallback after a missing reference is used.
    let pixels = draw(r##"<rect width="16" height="16" fill="url(#nowhere) red"/>"##);
    assert_eq!(at(&pixels, 16, 8, 8), RED);

    assert_eq!(
        refuse_body(r##"<path d="M0 0 L 4"/>"##),
        SvgErrorKind::InvalidPath { offset: 8 }
    );
    assert_eq!(
        refuse_body(r##"<clipPath id="c"><path d="L 4 4"/></clipPath>"##),
        SvgErrorKind::InvalidPath { offset: 1 }
    );
}

#[test]
fn harmless_properties_are_passed_over() {
    let pixels = draw(
        r##"<g style="font-family: 'Inter', sans-serif; font-size: 12px; -inkscape-font-specification:Inter;
                     shape-rendering: geometricPrecision; isolation: isolate; paint-order: normal;
                     mix-blend-mode: normal; mask: none; filter: none; marker: none; overflow: visible;
                     stroke-dashoffset: 0; stop-color: #000; enable-background: accumulate"
              pointer-events="none" text-anchor="middle" vector-effect="none" clip-path="none">
             <rect width="16" height="16" fill="red" paint-order="fill stroke markers"/>
           </g>"##,
    );
    assert_eq!(at(&pixels, 16, 8, 8), RED);
}

// ----------------------------------------------------------------- lenient

#[test]
fn the_lenient_mode_leaves_out_what_it_cannot_draw_and_says_so() {
    let source = r##"<svg viewBox="0 0 16 16">
        <style>.a { fill: blue }</style>
        <rect class="a" width="8" height="16" fill="red" mask="url(#m)" onclick="x()"/>
        <text x="2" y="8">Hi</text>
        <use href="#a"/>
        <rect x="8" width="1em" height="16" fill="blue"/>
        <rect x="8" width="8" height="16" fill="url(#gone)" stroke="red" stroke-width="2"/>
        <path d="M0 0 L" fill="red"/>
    </svg>"##;
    assert!(SvgPicture::parse(source).is_err());
    let (picture, notes) = SvgPicture::parse_lenient(source).unwrap();
    let pixels = picture.rasterise(16, 16);
    assert_eq!(
        at(&pixels, 16, 4, 8),
        RED,
        "the first rect, without its mask"
    );
    assert_eq!(at(&pixels, 16, 8, 8), RED, "the third rect's stroke");
    assert_eq!(
        at(&pixels, 16, 12, 8),
        NONE,
        "a fill that refers to nothing is none"
    );
    let kinds: Vec<String> = notes.iter().map(|note| note.to_string()).collect();
    assert_eq!(notes.len(), 8, "{kinds:#?}");
    assert!(kinds[0].contains("<style> is not supported"));
    assert!(kinds[1].contains("mask"));
    assert!(kinds[2].contains("onclick"));
    assert!(kinds[3].contains("<text>"));
    assert!(kinds[4].contains("<use>"));
    assert!(kinds[5].contains("1em"));
    assert!(kinds[6].contains("#gone"));
    assert!(kinds[7].contains("path data"));
    assert_eq!(notes[3].line, 4);
    // Not the same cache entry as the strict reading of a file.
    let clean = r##"<svg viewBox="0 0 1 1"/>"##;
    assert_ne!(
        SvgPicture::parse(clean).unwrap().content_hash(),
        SvgPicture::parse_lenient(clean).unwrap().0.content_hash()
    );
    // Structure and limits stay fatal.
    assert!(SvgPicture::parse_lenient("<svg viewBox='0 0 1 1'><g></svg>").is_err());
    assert!(SvgPicture::parse_lenient("<svg/>").is_err());
}

// ------------------------------------------------------------ hostile input

fn limit(source: &str) -> &'static str {
    match refuse(source).kind {
        SvgErrorKind::Limit(what) => what,
        other => panic!("{other:?}"),
    }
}

#[test]
fn nesting_counts_and_sizes_are_bounded() {
    // Depth: 64 is read, 65 is refused, and a hundred thousand does not
    // overflow the stack.
    let nested = |depth: usize| {
        format!(
            "<svg viewBox='0 0 1 1'>{}<rect width='1' height='1'/>{}</svg>",
            "<g>".repeat(depth),
            "</g>".repeat(depth)
        )
    };
    assert!(SvgPicture::parse(&nested(62)).is_ok());
    assert!(limit(&nested(64)).contains("nested"));
    assert!(limit(&nested(100_000)).contains("nested"));
    assert!(limit(&format!("<svg viewBox='0 0 1 1'>{}", "<g>".repeat(100_000))).contains("nested"));

    let many = format!("<svg viewBox='0 0 1 1'>{}</svg>", "<g/>".repeat(20_001));
    assert!(limit(&many).contains("elements"));
    let attributes: String = (0..200)
        .map(|index| format!(" data-a{index}='1'"))
        .collect();
    assert!(limit(&format!("<svg viewBox='0 0 1 1'{attributes}/>")).contains("attributes"));

    let long_path = format!(
        "<svg viewBox='0 0 1 1'><path d='M0 0{}'/></svg>",
        "l1 1".repeat(200_001)
    );
    assert!(limit(&long_path).contains("path commands"));
    // Spread over many paths too.
    let segment = format!("<path d='M0 0{}'/>", "h1".repeat(1000));
    let paths = format!("<svg viewBox='0 0 1 1'>{}</svg>", segment.repeat(201));
    assert!(limit(&paths).contains("path commands"));
    let points = format!(
        "<svg viewBox='0 0 1 1'><polygon points='{}'/></svg>",
        "1 1 ".repeat(200_002)
    );
    assert!(limit(&points).contains("path commands"));

    let huge = format!(
        "<svg viewBox='0 0 1 1'><!--{}--></svg>",
        " ".repeat(morningstar_text::MAX_SVG_BYTES)
    );
    assert!(limit(&huge).contains("4 MiB"));

    let stops = format!(
        "<svg viewBox='0 0 1 1'><linearGradient id='g'>{}</linearGradient></svg>",
        "<stop/>".repeat(257)
    );
    assert!(limit(&stops).contains("stops"));
}

#[test]
fn numbers_are_bounded() {
    assert!(limit("<svg viewBox='0 0 1 1'><path d='M0 0L1e30 0'/></svg>").contains("coordinate"));
    assert!(limit("<svg viewBox='0 0 1 1'><path d='M0 0h2e6'/></svg>").contains("coordinate"));
    // A number too large for the reader is not a number.
    for body in [
        "<rect width='1e7' height='1'/>",
        "<rect width='1e999' height='1'/>",
        "<circle r='99999999999999999999999999999999999999999999'/>",
        "<g transform='scale(1e30)'/>",
        "<g transform='matrix(NaN 0 0 1 0 0)'/>",
        "<polygon points='0 0 1e9 1 2 2'/>",
    ] {
        assert!(
            matches!(refuse_body(body), SvgErrorKind::UnsupportedValue { .. }),
            "{body}"
        );
    }
    assert!(matches!(
        refuse("<svg viewBox='0 0 1e30 1'/>").kind,
        SvgErrorKind::UnsupportedValue { .. }
    ));
    // Transforms that multiply up through the tree.
    let grown = format!(
        "<svg viewBox='0 0 1 1'>{}<rect width='1' height='1'/>{}</svg>",
        "<g transform='scale(1000000)'>".repeat(4),
        "</g>".repeat(4)
    );
    assert!(limit(&grown).contains("transform"));

    // Dashes: no more than a hundred thousand along a path, and an
    // enormous offset costs nothing.
    assert!(limit(
        "<svg viewBox='0 0 1 1'><path d='M0 0h1e6' stroke='red' stroke-dasharray='0.001'/></svg>"
    )
    .contains("dashes"));
    let picture = SvgPicture::parse(
        "<svg viewBox='0 0 16 16'><path d='M0 8h16' stroke='red' stroke-width='2' stroke-dasharray='4' stroke-dashoffset='1000000'/></svg>",
    )
    .unwrap();
    assert_eq!(at(&picture.rasterise(16, 16), 16, 0, 8), RED);

    // Gradients that refer to each other.
    assert!(limit(
        "<svg viewBox='0 0 1 1'><linearGradient id='a' href='#b'/><linearGradient id='b' href='#a'/></svg>"
    )
    .contains("loop"));
    assert!(
        limit("<svg viewBox='0 0 1 1'><linearGradient id='a' href='#a'/></svg>").contains("loop")
    );

    // Faded groups each need a canvas: four deep at most.
    let faded = |depth: usize| {
        format!(
            "<svg viewBox='0 0 4 4'>{}{}</svg>",
            "<g opacity='0.9'><rect width='1' height='1'/>".repeat(depth),
            "<rect width='2' height='2'/></g>".repeat(depth)
        )
    };
    assert!(SvgPicture::parse(&faded(4)).is_ok());
    assert!(limit(&faded(5)).contains("groups"));
}

#[test]
fn extreme_geometry_rasterises_without_panicking() {
    for body in [
        // As large as the reader lets through, and far outside the image.
        "<rect x='-1000000' y='-1000000' width='1000000' height='1000000' fill='red' stroke='blue' stroke-width='1000000'/>",
        "<circle cx='500000' cy='-500000' r='500000' fill='red'/>",
        "<path d='M-1e6 -1e6 L1e6 1e6 L-1e6 1e6z' fill='red' stroke='blue' stroke-width='0.0001'/>",
        "<path d='M0 0 A 1e6 1e6 0 1 1 1e-3 0' stroke='red' fill='none'/>",
        "<path d='M0 0 C 1e6 -1e6 -9e5 9e5 1 1 S 5 5 1e6 0 Q -5e5 3 2 2 T 3 3' stroke='red'/>",
        "<g transform='matrix(1e6 0 0 1e6 -1e6 1e6)'><rect width='1' height='1' fill='red'/></g>",
        "<g transform='scale(0)'><rect width='1' height='1' fill='red' stroke='blue'/></g>",
        "<g transform='matrix(1 1 1 1 0 0)'><circle r='5' fill='red' stroke='blue'/></g>",
        "<g transform='scale(1e-30)'><circle r='5' fill='red' stroke='blue'/></g>",
        "<linearGradient id='g' gradientTransform='scale(0)'><stop stop-color='red'/><stop offset='1'/></linearGradient><rect width='8' height='8' fill='url(#g)'/>",
        "<radialGradient id='g' r='0'><stop stop-color='red'/><stop offset='1'/></radialGradient><rect width='8' height='8' fill='url(#g)'/>",
        "<radialGradient id='g' fx='900%' fy='-900%'><stop stop-color='red'/><stop offset='1'/></radialGradient><rect width='8' height='8' fill='url(#g)'/>",
        "<linearGradient id='g' x1='1e6' x2='-1e6' gradientUnits='userSpaceOnUse' spreadMethod='reflect'><stop stop-color='red'/><stop offset='1'/></linearGradient><rect width='8' height='8' fill='url(#g)'/>",
        "<path d='M0 8h16' stroke='red' stroke-dasharray='1e-6 1e6' stroke-dashoffset='-1e6'/>",
        "<path d='M0 8h16' stroke='red' stroke-width='1e6' stroke-linejoin='miter' stroke-miterlimit='1e6'/>",
        "<clipPath id='c'><circle r='1e6' transform='scale(1e3)'/></clipPath><rect width='8' height='8' clip-path='url(#c)'/>",
    ] {
        let source = format!("<svg viewBox='0 0 16 16'>{body}</svg>");
        let picture = SvgPicture::parse(&source).unwrap_or_else(|error| panic!("{error}: {body}"));
        for (width, height) in [(16, 16), (1, 1), (64, 3)] {
            assert_eq!(
                picture.rasterise(width, height).len(),
                (width * height * 4) as usize,
                "{body}"
            );
        }
    }
    // A minute or enormous view box.
    for view_box in ["0 0 0.000001 0.000001", "-1e6 -1e6 1e6 1e6", "0 0 1e6 1e-6"] {
        let source = format!(
            "<svg viewBox='{view_box}'><circle r='1e5' fill='red' stroke='blue' stroke-width='3'/></svg>"
        );
        let picture = SvgPicture::parse(&source).unwrap();
        assert_eq!(picture.rasterise(32, 32).len(), 32 * 32 * 4);
    }
}

/// A sample that touches every part of the reader.
const SAMPLE: &str = r##"<?xml version="1.0"?>
<!-- sample -->
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
  <defs>
    <linearGradient id="a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#F6C445"/><stop offset="100%" stop-color="rgb(14, 14, 14)" stop-opacity=".5"/></linearGradient>
    <radialGradient id="b" href="#a" cx="50%" cy="50%" r="50%" fx="30%" gradientTransform="rotate(30 .5 .5)"/>
    <clipPath id="c"><circle cx="12" cy="12" r="10"/><rect x="2" y="2" width="4" height="4" rx="1"/></clipPath>
  </defs>
  <g clip-path="url(#c)" opacity="0.8" transform="translate(1 1) rotate(-18 12 12) scale(.9)">
    <rect x="-4" y="8" width="32" height="8" fill="url(#a)" style="stroke: none; fill-opacity: 0.9"/>
    <ellipse cx="12" cy="12" rx="9" ry="4" fill="url(#b)" stroke-dasharray="2 1.5" stroke-dashoffset="1"/>
    <path d="M4 22v-4a14 14 0 0 1 28 0v4M5 12l5 5 9-10z" stroke-linecap="round" stroke-linejoin="round" fill-rule="evenodd" fill="&#35;4CAF50"/>
    <polyline points="1,1 5,3 9,1 13,3" stroke="red"/><polygon points="20 20 23 20 23 23" fill="navy" stroke-miterlimit="2"/>
    <line x1="0" y1="24" x2="24" y2="0" stroke="#5AA9E680"/>
  </g>
</svg>
"##;

#[test]
fn the_sample_loads() {
    let picture = SvgPicture::parse(SAMPLE).unwrap_or_else(|error| panic!("{error}"));
    assert!(!picture.is_single_ink());
    let pixels = picture.rasterise(48, 48);
    assert!(pixels.chunks_exact(4).filter(|pixel| pixel[3] > 0).count() > 400);
}

#[test]
fn truncated_and_damaged_input_never_panics() {
    // Cut at every byte.
    for end in 0..SAMPLE.len() {
        if !SAMPLE.is_char_boundary(end) {
            continue;
        }
        if let Ok(picture) = SvgPicture::parse(&SAMPLE[..end]) {
            picture.rasterise(8, 8);
        }
        if let Ok((picture, _)) = SvgPicture::parse_lenient(&SAMPLE[..end]) {
            picture.rasterise(8, 8);
        }
    }
    // Bytes replaced, removed and doubled, from a fixed sequence.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let interesting = b"<>/\"'=&;#()%.-e 0123456789,urlMmLlZzAaCcSs\n";
    let (mut loaded, mut refused) = (0, 0);
    for _ in 0..6000 {
        let mut bytes = SAMPLE.as_bytes().to_vec();
        for _ in 0..1 + next() % 4 {
            let index = (next() % bytes.len() as u64) as usize;
            match next() % 4 {
                0 => bytes[index] = interesting[(next() % interesting.len() as u64) as usize],
                1 => {
                    bytes.remove(index);
                }
                2 => {
                    let byte = bytes[index];
                    bytes.insert(index, byte);
                }
                _ => {
                    let end = (index + (next() % 24) as usize).min(bytes.len());
                    bytes.drain(index..end);
                }
            }
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        match SvgPicture::parse(&text) {
            Ok(picture) => {
                loaded += 1;
                assert_eq!(picture.rasterise(12, 12).len(), 12 * 12 * 4);
            }
            Err(error) => {
                refused += 1;
                assert!(error.offset <= text.len());
                assert!(!error.to_string().is_empty());
            }
        }
        if let Ok((picture, _)) = SvgPicture::parse_lenient(&text) {
            picture.rasterise_mask(12, 12);
        }
    }
    // Both happen: the damage is neither always fatal nor never.
    assert!(
        loaded > 200 && refused > 200,
        "{loaded} loaded, {refused} refused"
    );
}

// ------------------------------------------------------------ equivalences

#[test]
fn a_loaded_file_matches_the_same_picture_written_as_layers() {
    const GOLD: Color = Color::rgb(1.0, 1.0, 0.0);
    static LAYERS: [Layer; 4] = [
        Layer::circle(50.0, 50.0, 48.0).fill(Color::BLACK),
        Layer::rect(-20.0, 40.0, 140.0, 20.0)
            .fill(GOLD)
            .rotate(-18.0, 50.0, 50.0)
            .clip_circle(50.0, 50.0, 48.0),
        Layer::circle(50.0, 50.0, 20.0)
            .fill(Color::WHITE)
            .stroke(Color::BLACK, 3.0),
        Layer::path("M4 22v-4a14 14 0 0 1 28 0v4")
            .stroke(Color::BLACK, 3.0)
            .cap(LineCap::Round)
            .translate(32.0, 32.0),
    ];
    let layers = VectorPicture::new(100.0, 100.0, &LAYERS);
    let loaded = SvgPicture::parse(
        r##"<svg viewBox="0 0 100 100">
             <defs><clipPath id="disc"><circle cx="50" cy="50" r="48"/></clipPath></defs>
             <circle cx="50" cy="50" r="48" fill="#000"/>
             <g clip-path="url(#disc)">
               <rect x="-20" y="40" width="140" height="20" fill="#ff0" transform="rotate(-18 50 50)"/>
             </g>
             <circle cx="50" cy="50" r="20" fill="white" stroke="black" stroke-width="3"/>
             <g fill="none" stroke="#000" stroke-width="3" stroke-linecap="round" transform="translate(32 32)">
               <path d="M4 22v-4a14 14 0 0 1 28 0v4"/>
             </g>
           </svg>"##,
    )
    .unwrap();
    for (width, height) in [(100, 100), (200, 200), (281, 281), (64, 64)] {
        assert_eq!(
            loaded.rasterise(width, height),
            layers.rasterise(width, height),
            "{width} x {height}"
        );
    }
}

#[test]
fn an_asset_parses_once_and_reports_a_bad_file() {
    static GOOD: SvgAsset =
        SvgAsset::new(r##"<svg viewBox="0 0 8 8"><circle cx="4" cy="4" r="3" fill="red"/></svg>"##);
    static BAD: SvgAsset = SvgAsset::new(r##"<svg viewBox="0 0 8 8"><text>Hi</text></svg>"##);
    assert!(GOOD.check().is_ok());
    assert!(std::ptr::eq(GOOD.get(), GOOD.get()));
    assert_eq!(at(&GOOD.get().rasterise(8, 8), 8, 4, 4), RED);

    let error = BAD.check().unwrap_err();
    assert_eq!(error.kind, SvgErrorKind::UnsupportedElement("text".into()));
    // It draws nothing rather than something else.
    assert!(BAD.get().rasterise(8, 8).iter().all(|byte| *byte == 0));
    assert_eq!(BAD.get().width(), 0.0);
}

#[test]
fn geometry_far_larger_than_the_image_is_still_drawn_right() {
    // A background far bigger than the picture, as drawing programs leave.
    let pixels =
        draw(r##"<rect x="-100000" y="-100000" width="200000" height="200000" fill="red"/>"##);
    assert!(pixels.chunks_exact(4).all(|pixel| pixel == RED));

    // The edge of an enormous circle crossing the picture at x = 8.
    let pixels = draw(r##"<circle cx="-99992" cy="8" r="100000" fill="red"/>"##);
    for y in 0..16 {
        assert_eq!(at(&pixels, 16, 6, y), RED, "row {y}");
        assert_eq!(at(&pixels, 16, 9, y), NONE, "row {y}");
    }

    // A long stroke, a long dashed stroke, and a hole by the even-odd rule.
    let pixels = draw(r##"<path d="M-100000 8H100000" stroke="red" stroke-width="4"/>"##);
    for x in 0..16 {
        assert_eq!(at(&pixels, 16, x, 5), NONE);
        assert_eq!(at(&pixels, 16, x, 6), RED);
        assert_eq!(at(&pixels, 16, x, 9), RED);
        assert_eq!(at(&pixels, 16, x, 10), NONE);
    }
    let pixels =
        draw(r##"<path d="M-80 8H8000" stroke="red" stroke-width="2" stroke-dasharray="4"/>"##);
    let row: Vec<bool> = (0..16).map(|x| at(&pixels, 16, x, 8) == RED).collect();
    let expected: Vec<bool> = "####....####...."
        .bytes()
        .map(|byte| byte == b'#')
        .collect();
    assert_eq!(row, expected);
    let pixels = draw(
        r##"<path d="M-90000 -90000h180000v180000h-180000z M4 4h8v8h-8z" fill="red" fill-rule="evenodd"/>"##,
    );
    assert_eq!(at(&pixels, 16, 8, 8), NONE);
    assert_eq!(at(&pixels, 16, 2, 8), RED);
    assert_eq!(at(&pixels, 16, 14, 14), RED);

    // The same shape through the rasteriser directly, where it fits, and
    // cut to the box, where a far corner forces it: the visible part is
    // the same within a level.
    let near = draw(
        r##"<path d="M-500 -300 L 12 4 L 3 900 z" fill="red" stroke="blue" stroke-width="3"/>"##,
    );
    let far = draw(
        r##"<path d="M-500 -300 L 12 4 L 3 900 L 3 90000 z" fill="red" stroke="blue" stroke-width="3"/>"##,
    );
    let differing = near
        .chunks_exact(4)
        .zip(far.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 1))
        .count();
    assert_eq!(differing, 0);
}

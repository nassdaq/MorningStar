//! What a real design needed beyond boxes and one font: named font
//! families, letter spacing, capitals, line heights in dp, rich text,
//! vector icons and dashed borders.

use morningstar_core::{DrawOp, Rect, SurfaceSize};
use morningstar_runtime::{App, Frame, FrameInfo};
use morningstar_ui::prelude::*;

const NOTO: &[u8] = include_bytes!("../../../examples/hello-gpu/assets/fonts/NotoSans-Regular.ttf");
const INTER: &[u8] = include_bytes!("../../../examples/fundidesk/assets/fonts/Inter-Regular.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../../../examples/fundidesk/assets/fonts/Inter-Bold.ttf");
const GROTESK: &[u8] =
    include_bytes!("../../../examples/fundidesk/assets/fonts/SpaceGrotesk-Bold.ttf");

const INTER_FAMILY: FontFamily = FontFamily::named("Inter");
const GROTESK_FAMILY: FontFamily = FontFamily::named("Space Grotesk");
const CHECK: VectorIcon = VectorIcon::stroked("M5 12l5 5 9-10");
const RED: Color = hex(0xFF0000);

struct Screen {
    view: fn() -> Element<()>,
    theme: Theme,
}

impl Application for Screen {
    type Message = ();

    fn setup(&mut self, setup: &mut Setup<'_>) {
        // Noto first: it is what text with no family gets.
        for font in [NOTO, INTER, INTER_BOLD, GROTESK] {
            setup.add_font(font);
        }
        assert!(setup.has_font_family("Inter"));
        assert!(setup.has_font_family("Space Grotesk"));
        assert!(!setup.has_font_family("Comic Sans"));
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn view(&self) -> Element<()> {
        (self.view)()
    }

    fn update(&mut self, _: ()) {}
}

struct Rendered {
    ui: Ui<Screen>,
    frame: Frame,
    scale: f32,
}

impl Rendered {
    /// The rectangle of the widget with this id, in dp.
    fn rect(&self, id: &str) -> Rect {
        let rect = self.ui.rect_of(id).unwrap_or_else(|| panic!("no {id}"));
        Rect::new(
            rect.x / self.scale,
            rect.y / self.scale,
            rect.width / self.scale,
            rect.height / self.scale,
        )
    }

    fn glyphs(&self) -> Vec<(Rect, Rect, Color)> {
        self.frame
            .list
            .ops()
            .iter()
            .filter_map(|op| match *op {
                DrawOp::Glyph { rect, atlas, color } => Some((rect, atlas, color)),
                _ => None,
            })
            .collect()
    }
}

fn render_with(view: fn() -> Element<()>, theme: Theme, scale: f32) -> Rendered {
    let mut ui = Ui::new(Screen { view, theme });
    let mut frame = Frame::for_app(&mut ui);
    frame.begin();
    let size = SurfaceSize::new((360.0 * scale) as u32, (800.0 * scale) as u32);
    ui.frame(&FrameInfo::new(size).with_scale(scale), &mut frame);
    Rendered { ui, frame, scale }
}

fn render(view: fn() -> Element<()>) -> Rendered {
    render_with(view, Theme::LIGHT, 2.0)
}

// ------------------------------------------------------------------ fonts

const SAMPLE: &str = "Kariakoo Phone Clinic";

fn family_samples() -> Element<()> {
    col![
        text(SAMPLE).size(20.0).id("default"),
        text(SAMPLE).size(20.0).family("Noto Sans").id("noto"),
        text(SAMPLE).size(20.0).family(INTER_FAMILY).id("inter"),
        text(SAMPLE)
            .size(20.0)
            .bold()
            .family(GROTESK_FAMILY)
            .id("grotesk"),
        text(SAMPLE)
            .size(20.0)
            .bold()
            .family("Inter")
            .id("inter-bold"),
        text(SAMPLE).size(20.0).family("No Such Font").id("missing"),
    ]
    .into()
}

#[test]
fn a_text_can_name_its_font_family() {
    let screen = render(family_samples);
    let width = |id: &str| screen.rect(id).width;
    assert_eq!(width("default"), width("noto"));
    assert_eq!(width("missing"), width("default"), "falls back");
    assert!((width("inter") - width("noto")).abs() > 2.0);
    assert!((width("grotesk") - width("inter-bold")).abs() > 2.0);
    assert!(width("inter-bold") > width("inter"));
    assert!(screen.ui.overflows().is_empty());
}

fn roles() -> Element<()> {
    col![
        text(SAMPLE).title().id("title"),
        text(SAMPLE)
            .title()
            .family(GROTESK_FAMILY)
            .id("title-grotesk"),
        text(SAMPLE).title().family(INTER_FAMILY).id("title-inter"),
        text(SAMPLE).display().id("display"),
        text(SAMPLE)
            .display()
            .family(GROTESK_FAMILY)
            .id("display-grotesk"),
        text(SAMPLE).headline().bold().id("headline"),
        text(SAMPLE)
            .headline()
            .bold()
            .family(GROTESK_FAMILY)
            .id("headline-grotesk"),
        text(SAMPLE).id("body"),
        text(SAMPLE).family(INTER_FAMILY).id("body-inter"),
        text(SAMPLE).label().regular().id("label"),
        text(SAMPLE)
            .label()
            .regular()
            .family(INTER_FAMILY)
            .id("label-inter"),
        text(SAMPLE).caption().id("caption"),
        text(SAMPLE)
            .caption()
            .family(INTER_FAMILY)
            .id("caption-inter"),
    ]
    .into()
}

#[test]
fn the_theme_gives_headings_and_body_their_families() {
    let theme = Theme {
        fonts: Fonts::new(INTER_FAMILY, GROTESK_FAMILY),
        ..Theme::LIGHT
    };
    let screen = render_with(roles, theme, 2.0);
    let width = |id: &str| screen.rect(id).width;
    for role in ["title", "display", "headline"] {
        assert_eq!(width(role), width(&format!("{role}-grotesk")), "{role}");
    }
    for role in ["body", "label", "caption"] {
        assert_eq!(width(role), width(&format!("{role}-inter")), "{role}");
    }
    // An explicit family beats the theme's.
    assert_ne!(width("title"), width("title-inter"));

    // With the default theme nothing is named: everything is the first font.
    let plain = render(roles);
    assert_ne!(plain.rect("body").width, plain.rect("body-inter").width);
    assert_eq!(Theme::LIGHT.fonts, Fonts::DEFAULT);
    assert_eq!(Theme::DARK.font(TextRole::Title), FontFamily::Default);
}

fn spaced() -> Element<()> {
    col![
        text("CUSTOMER").size(13.0).family(INTER_FAMILY).id("plain"),
        text("CUSTOMER")
            .size(13.0)
            .family(INTER_FAMILY)
            .letter_spacing(0.6)
            .id("wide"),
        text("CUSTOMER")
            .size(13.0)
            .family(INTER_FAMILY)
            .letter_spacing(-0.5)
            .id("tight"),
        text("Customer")
            .size(13.0)
            .family(INTER_FAMILY)
            .letter_spacing(0.6)
            .uppercase()
            .id("upper"),
        text(format!("custom{}", "er"))
            .size(13.0)
            .family(INTER_FAMILY)
            .uppercase()
            .id("owned"),
    ]
    .into()
}

#[test]
fn letter_spacing_is_in_dp_and_uppercase_rewrites_the_text() {
    for scale in [1.0, 2.0, 3.0] {
        let screen = render_with(spaced, Theme::LIGHT, scale);
        let width = |id: &str| screen.rect(id).width;
        // Eight characters, each followed by the spacing. Widths are whole
        // pixels, so allow one.
        let slack = 1.0 / scale + 0.01;
        assert!(
            (width("wide") - width("plain") - 8.0 * 0.6).abs() <= slack,
            "{scale}"
        );
        assert!(
            (width("tight") - width("plain") + 8.0 * 0.5).abs() <= slack,
            "{scale}"
        );
        assert_eq!(width("upper"), width("wide"));
        assert_eq!(width("owned"), width("plain"));
        // The same line height (boxes snap to pixels, so within one).
        let heights = screen.rect("wide").height - screen.rect("plain").height;
        assert!(heights.abs() <= slack);
    }
}

fn line_heights() -> Element<()> {
    col![
        text("One line").size(15.0).line_height_dp(22.0).id("dp"),
        text("One line")
            .size(15.0)
            .line_height(22.0 / 15.0)
            .id("multiple"),
        text("Two\nlines").size(28.0).line_height_dp(34.0).id("two"),
        text("One line").size(0.0).line_height_dp(22.0).id("zero"),
    ]
    .into()
}

#[test]
fn a_line_height_can_be_given_in_dp() {
    for scale in [1.0, 2.0, 2.625] {
        let screen = render_with(line_heights, Theme::LIGHT, scale);
        let snapped = |dp: f32| (dp * scale).round() / scale;
        assert!((screen.rect("dp").height - snapped(22.0)).abs() < 0.51 / scale);
        assert_eq!(screen.rect("dp").height, screen.rect("multiple").height);
        assert!((screen.rect("two").height - 68.0).abs() <= 1.0 / scale);
    }
}

// -------------------------------------------------------------- rich text

fn rich() -> Element<()> {
    // The root always fills the screen; the 320 dp column sits inside it.
    Container::new(rich_column()).into()
}

fn rich_column() -> Container<()> {
    col![
        Text::rich([
            span("New to FundiDesk? "),
            span("Create an account")
                .semibold()
                .color(ThemeColor::Primary),
        ])
        .size(14.0)
        .muted()
        .family(INTER_FAMILY)
        .id("rich"),
        text("New to FundiDesk? Create an account")
            .size(14.0)
            .family(INTER_FAMILY)
            .id("plain"),
        Text::rich([
            span("Log each device the moment it comes in. See what is "),
            span("delayed").color(RED).bold(),
            span(", what is ready and what is still unpaid."),
        ])
        .size(15.0)
        .line_height_dp(23.0)
        .family(INTER_FAMILY)
        .fill_width()
        .id("paragraph"),
        Text::rich([
            span("total "),
            span("TZS 40,000").family(GROTESK_FAMILY).bold()
        ])
        .size(14.0)
        .family(INTER_FAMILY)
        .id("mixed-family"),
        Text::rich([span("total "), span("TZS 40,000").bold()])
            .size(14.0)
            .family(INTER_FAMILY)
            .id("one-family"),
        Text::rich(["caps ", "lock"])
            .uppercase()
            .family(INTER_FAMILY)
            .id("upper"),
        text("CAPS LOCK").family(INTER_FAMILY).id("upper-plain"),
        Text::rich(Vec::<Span>::new()).id("empty"),
    ]
    .width(320.0)
}

#[test]
fn rich_text_mixes_weight_colour_and_family_in_one_paragraph() {
    let theme = Theme::LIGHT;
    let screen = render_with(rich, theme, 2.0);
    let (rich_rect, plain) = (screen.rect("rich"), screen.rect("plain"));
    assert_eq!(rich_rect.height, plain.height, "one line");
    assert!(rich_rect.width > plain.width, "the bold phrase is wider");
    assert!(rich_rect.width < plain.width * 1.1);

    // Colours per span, with theme tokens resolved: 15 muted glyphs
    // ("NewtoFundiDesk?"), then 15 in the primary colour.
    let glyphs = screen.glyphs();
    let line: Vec<_> = glyphs.iter().take(30).collect();
    assert!(
        line[..15]
            .iter()
            .all(|glyph| glyph.2 == theme.colors.text_muted)
    );
    assert!(
        line[15..]
            .iter()
            .all(|glyph| glyph.2 == theme.colors.primary)
    );

    // A paragraph wraps as one text inside its box.
    let paragraph = screen.rect("paragraph");
    assert_eq!(paragraph.width, 320.0);
    assert_eq!(paragraph.height, 3.0 * 23.0);
    assert_eq!(glyphs.iter().filter(|glyph| glyph.2 == RED).count(), 7);

    assert_ne!(
        screen.rect("mixed-family").width,
        screen.rect("one-family").width
    );
    assert_eq!(screen.rect("upper").width, screen.rect("upper-plain").width);
    assert_eq!(screen.rect("empty").width, 0.0);
    assert!(screen.ui.overflows().is_empty());
}

// ------------------------------------------------------------------ icons

fn icons() -> Element<()> {
    col![
        Icon::vector(CHECK).id("default"),
        Icon::vector(CHECK).size(18.0).color(RED).id("small"),
        icon(CHECK)
            .size(18.0)
            .color(RED)
            .stroke_width(3.2)
            .id("heavy"),
        Icon::vector(CHECK)
            .size(40.0)
            .background(hex(0x00FF00))
            .color(RED)
            .id("tile"),
        Icon::new("+").id("glyph-tile"),
        Icon::vector(VectorIcon::svg("<svg><g/></svg>")).id("broken"),
    ]
    .gap(4.0)
    .padding(10.0)
    .into()
}

#[test]
fn a_vector_icon_is_drawn_at_its_size_in_its_colour() {
    for scale in [1.0, 2.0, 3.0] {
        let theme = Theme::LIGHT;
        let screen = render_with(icons, theme, scale);
        assert_eq!(screen.rect("default"), Rect::new(10.0, 10.0, 24.0, 24.0));
        assert_eq!(screen.rect("small"), Rect::new(10.0, 38.0, 18.0, 18.0));
        assert_eq!(screen.rect("tile").width, 40.0);
        assert_eq!(screen.rect("glyph-tile").width, 40.0);
        // A broken icon keeps its place and draws nothing.
        assert_eq!(screen.rect("broken").width, 24.0);

        // Default, small, heavy, the one on the tile, then the "+" glyph.
        let glyphs = screen.glyphs();
        assert_eq!(glyphs.len(), 5);
        let (rect, mask, color) = glyphs[0];
        assert_eq!(rect, screen.ui.rect_of("default").unwrap());
        assert_eq!(color, theme.colors.text);
        // Rasterised for exactly the pixels it covers.
        assert_eq!((mask.width, mask.height), (24.0 * scale, 24.0 * scale));

        let (rect, mask, color) = glyphs[1];
        assert_eq!(rect, screen.ui.rect_of("small").unwrap());
        assert_eq!((mask.width, color), (18.0 * scale, RED));
        // Another stroke width is another mask of the same size.
        assert_ne!((glyphs[2].1.x, glyphs[2].1.y), (mask.x, mask.y));
        assert_eq!(glyphs[2].1.width, mask.width);

        // On a tile the icon takes half the side, centred.
        let tile = screen.ui.rect_of("tile").unwrap();
        let (rect, _, color) = glyphs[3];
        assert_eq!(rect.width, 20.0 * scale);
        assert_eq!(rect.x - tile.x, 10.0 * scale);
        assert_eq!(rect.y - tile.y, 10.0 * scale);
        assert_eq!(color, RED);
        assert!(screen.frame.list.ops().iter().any(
            |op| matches!(*op, DrawOp::Rect { rect, color, .. } if rect == tile && color == hex(0x00FF00))
        ));
    }
}

fn scrolled_icons() -> Element<()> {
    ScrollView::new(
        Column::new()
            .extend((0..200).map(|index| Icon::vector(CHECK).size(40.0).id(("icon", index)))),
    )
    .into()
}

#[test]
fn icons_far_outside_a_scroll_view_are_not_drawn() {
    let screen = render(scrolled_icons);
    // 800 dp of viewport plus the cull margin, at 40 dp each.
    let drawn = screen.glyphs().len();
    assert!((20..=25).contains(&drawn), "{drawn}");
}

// ---------------------------------------------------------------- borders

fn dashed() -> Element<()> {
    col![
        Container::empty()
            .size(110.0, 64.0)
            .radius(12.0)
            .background(hex(0xFFFFFF))
            .border_dashed(1.0, RED)
            .id("css"),
        Container::empty()
            .size(110.0, 64.0)
            .border(2.0, RED)
            .border_dash(6.0, 4.0)
            .id("pattern"),
        Container::empty()
            .size(110.0, 64.0)
            .gradient(Gradient::vertical(hex(0x000000), hex(0xFFFFFF)))
            .border_dashed(1.0, RED)
            .id("gradient"),
        Container::empty()
            .size(110.0, 64.0)
            .border_dash(6.0, 4.0)
            .id("no-border"),
        Container::empty()
            .size(110.0, 64.0)
            .border(1.0, RED)
            .id("solid"),
    ]
    .into()
}

#[test]
fn a_border_can_be_dashed() {
    let screen = render(dashed);
    let dashed_ops: Vec<_> = screen
        .frame
        .list
        .ops()
        .iter()
        .filter_map(|op| match *op {
            DrawOp::DashedRect {
                rect,
                corner_radius,
                fill,
                border,
                dash,
            } => Some((rect, corner_radius, fill, border, dash)),
            _ => None,
        })
        .collect();
    assert_eq!(dashed_ops.len(), 3);

    // The CSS pattern, in pixels at scale 2: a 2 px border has 6 px dashes
    // and gaps near 4 px, fitted to the perimeter.
    let (rect, radius, fill, border, dash) = dashed_ops[0];
    assert_eq!(rect, screen.ui.rect_of("css").unwrap());
    assert_eq!((radius, border.width, fill), (24.0, 2.0, hex(0xFFFFFF)));
    assert_eq!(dash.length, 6.0);
    assert!((dash.gap - 4.0).abs() < 0.2, "{dash:?}");

    // An explicit pattern is in dp.
    let (_, _, fill, border, dash) = dashed_ops[1];
    assert_eq!((border.width, dash.length, dash.gap), (4.0, 12.0, 8.0));
    assert_eq!(fill.a, 0.0);

    // Over a gradient the dashes are their own operation with no fill.
    assert_eq!(dashed_ops[2].2.a, 0.0);

    // A dash without a border draws nothing; a plain border is unchanged.
    let solid = screen.ui.rect_of("solid").unwrap();
    assert!(
        screen
            .frame
            .list
            .ops()
            .iter()
            .any(|op| matches!(*op, DrawOp::BorderedRect { rect, .. } if rect == solid))
    );
    let none = screen.ui.rect_of("no-border").unwrap();
    assert!(!screen.frame.list.ops().iter().any(|op| match *op {
        DrawOp::DashedRect { rect, .. } | DrawOp::BorderedRect { rect, .. } => rect == none,
        _ => false,
    }));
}

// ------------------------------------------------------------- robustness

fn odd() -> Element<()> {
    let nan = f32::NAN;
    col![
        text("x").letter_spacing(nan).line_height_dp(nan).family(""),
        text("x").letter_spacing(f32::INFINITY).line_height_dp(-4.0),
        text("x")
            .letter_spacing(-1.0e9)
            .size(nan)
            .line_height_dp(0.0),
        Text::rich([span(""), span("y").family("")])
            .uppercase()
            .max_lines(0),
        Icon::vector(CHECK).size(nan).stroke_width(nan),
        Icon::vector(CHECK).size(-3.0).stroke_width(-1.0),
        Icon::vector(CHECK).size(1.0e9).stroke_width(f32::INFINITY),
        Icon::vector(CHECK).size(0.0).background(RED).round(),
        Icon::vector(VectorIcon::svg("")),
        Icon::vector(VectorIcon::stroked("not a path")),
        Container::empty().size(40.0, 40.0).border_dashed(nan, RED),
        Container::empty()
            .size(40.0, 40.0)
            .border_dashed(1.0e9, RED),
        Container::empty()
            .size(40.0, 40.0)
            .border(1.0, RED)
            .border_dash(nan, -1.0),
        Container::empty()
            .size(40.0, 40.0)
            .border(1.0, RED)
            .border_dash(0.0, 0.0),
        Container::empty()
            .size(0.0, 0.0)
            .radius(nan)
            .border_dashed(1.0, RED),
    ]
    .into()
}

#[test]
fn odd_input_to_the_new_widgets_never_panics() {
    for size in [
        SurfaceSize::new(720, 1600),
        SurfaceSize::new(1, 1),
        SurfaceSize::new(0, 0),
    ] {
        for scale in [0.01, 1.0, 2.0, 100.0] {
            let mut ui = Ui::new(Screen {
                view: odd,
                theme: Theme::LIGHT,
            });
            let mut frame = Frame::for_app(&mut ui);
            for _ in 0..2 {
                frame.begin();
                ui.frame(&FrameInfo::new(size).with_scale(scale), &mut frame);
            }
            for op in frame.list.ops() {
                if let DrawOp::Glyph { rect, .. } = *op {
                    assert!(rect.x.is_finite() && rect.width.is_finite());
                }
            }
        }
    }
}

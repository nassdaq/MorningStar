# Icon

One-colour vector artwork that takes its colour where it is drawn. `icon(..)` is short for a bare `Icon::vector(..)`.

{{snippet examples/docs-snippets/src/widgets.rs#icons}}

{{api crates/morningstar-ui/src/element.rs Icon}}

## Where an icon comes from

| Source | Write |
| --- | --- |
| Path data in the source | `VectorIcon::stroked("M5 12l5 5L20 7")` or `VectorIcon::filled(..)`, in a 24 by 24 box |
| Whole SVG markup in the source | `VectorIcon::svg("<svg ..>")` |
| An SVG file of the app | `static BACK: SvgAsset = SvgAsset::new(include_str!("../assets/svg/back.svg"));` then `Icon::svg(BACK.get())` |
| A letter or symbol as a stand-in | `Icon::new("?")` |

An icon says nothing to a screen reader. If it is a control, give the pressable around it a `.label(..)`.

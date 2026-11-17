# Theme

A theme is plain data with public fields. Start from `Theme::LIGHT` or `Theme::DARK` and change what the design changes; [Themes and motion](../guide/theming-and-motion.md) shows how.

## Theme

{{fields crates/morningstar-ui/src/theme.rs Theme}}

{{api crates/morningstar-ui/src/theme.rs Theme}}

## Colors

{{fields crates/morningstar-ui/src/theme.rs Colors}}

The default light theme has a cool grey background, white surfaces and a blue accent.

## TypeScale

{{fields crates/morningstar-ui/src/theme.rs TypeScale}}

| Role | Size | Weight | Line height |
| --- | --- | --- | --- |
| `display` | 34 dp | Bold | 1.2 |
| `title` | 24 dp | Bold | 1.25 |
| `headline` | 17 dp | Semibold | 1.3 |
| `body` | 15 dp | Regular | 1.4 |
| `label` | 14 dp | Semibold | 1.3 |
| `caption` | 12 dp | Regular | 1.35 |

A step is a `TextSpec::new(size, weight, line_height)`.

## Fonts

{{fields crates/morningstar-ui/src/theme.rs Fonts}}

{{api crates/morningstar-ui/src/theme.rs Fonts}}

## Spacing and Radii

| Spacing | dp | | Radii | dp |
| --- | --- | --- | --- | --- |
| `xs` | 4 | | `sm` | 8 |
| `sm` | 8 | | `md` | 12 |
| `md` | 12 | | `lg` | 16 |
| `lg` | 16 | | `xl` | 24 |
| `xl` | 24 | | `full` | 9999 |
| `xxl` | 32 | | | |

These are values an app may use; widgets take plain numbers, and nothing forces a design onto the scale.

## ThemeColor

The roles a widget can name instead of a value.

{{variants crates/morningstar-ui/src/color.rs ThemeColor}}

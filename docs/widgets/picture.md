# Picture and Avatar

## Picture

A picture draws an image, a multi-colour vector picture, or a loaded SVG file.

{{snippet examples/docs-snippets/src/pictures.rs#image}}

{{api crates/morningstar-ui/src/element.rs Picture}}

`ImageFit` says how the image fills a box of a different shape: `Cover` crops, `Contain` shows it whole.

Images are added once, at setup, from PNG data (`setup.add_image_png(bytes)`) or raw pixels (`setup.add_image_rgba8(width, height, pixels)`). Only PNG is decoded.

## An SVG file

{{snippet examples/docs-snippets/src/pictures.rs#svg}}

An `SvgAsset` is parsed once, the first time it is drawn. The loader accepts a static subset of SVG and refuses anything else with the line it is on; see [Text, fonts and pictures](../guide/text-and-pictures.md#svg-files).

## Avatar

A person: an image cropped to a circle, or initials when there is none.

{{api crates/morningstar-ui/src/element.rs Avatar}}

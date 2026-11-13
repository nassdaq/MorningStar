# 0034 — Blur of a box's background, evaluated per pixel: a solid box as its shadow, a radial gradient by quadrature; no blur pass

Status: Accepted (2026-11-13). Verified on the development host only (macOS, Metal, offscreen). Nothing here has run on a device and nothing here has been timed.

Extends [0011](0011-renderer-vocabulary.md) (shadows as an analytic blur) and [0033](0033-radial-gradients.md). It adds no render pass and changes no decision of theirs.

## Context

CSS `filter: blur(Npx)` blurs whatever an element paints with a Gaussian of standard deviation N. The general way to do that is a post-process: draw the element into an offscreen target, blur it in two passes, composite. The Atlas design uses the filter five times, always on the same thing: an ellipse under the orb filled with `radial-gradient(ellipse, rgba(214,92,190,0.5), rgba(214,92,190,0) 70%)`, blurred by a twentieth of the orb's size. What is blurred is one box's background, known in closed form.

## Decision

**`Container::blur(sigma)` blurs the box's background only, in the quad's own fragment shader, with no offscreen pass.** What it means depends on the background:

- **A solid colour.** A blurred rounded rectangle is exactly what a shadow is. The UI layer draws `Shadow { blur: 2 * sigma, offset 0 }` in the box's colour in place of the fill (a blur radius is two standard deviations, [0011](0011-renderer-vocabulary.md)). Exact along x, a 6-point quadrature along y, as shadows have always been.
- **A radial gradient.** `fill_radial_gradient_rect(.., blur)` passes the sigma on the instance. The fragment stage evaluates the *unblurred* picture (the segment's colour times the rounded box's coverage) at 6 x 6 points around the pixel and sums them with Gauss-Hermite weights: nodes at `sqrt(2) * sigma * x_i`, which integrate a Gaussian times a polynomial of degree 11 exactly. The quad grows by three sigma, as a shadow's does. The segments of a multi-stop gradient each integrate their own band, and the bands sum to the whole.
- **A linear gradient, a border, the children.** Not blurred. A blur of arbitrary content is not available; it would be the post-process pass, on the layer targets of [0028](0028-group-opacity-layers.md), and is left until a design needs it.

**How exact the quadrature is.** The integrand is not a polynomial: a gradient has a kink at its centre and at each stop, and a box has an edge. Measured against a brute-force convolution, for the design's glow (an ellipse 141 x 42 dp fading to nothing at its edge, sigma 9 dp, peak alpha 0.31 after blurring) the largest error is 0.011 in alpha with 6 points a side, 0.007 with 8 and 0.016 with 4: about 1% of the picture's strength, under two 8-bit levels for this colour over this background. Six was chosen: 36 evaluations of a square root and a mix per pixel of a quad that small. **The error grows where the gradient does not fade out inside its box**: a blurred gradient with a visible hard edge is a step under the quadrature and can ripple by several percent. For that case, a solid box (exact) or the post-process pass are the right tools; the doc comment on `Container::blur` says so.

**Why not the pass now.** It needs a second pipeline, intermediate targets and a kernel of some hundred taps at these sigmas on a phone, for every frame the orb moves; and the only content the design blurs has a closed form. The decision is per use, as [0011](0011-renderer-vocabulary.md) decided for shadows.

**An ellipse is drawn as a pill.** `border-radius: 50%` on a box that is not square is an ellipse, which the rounded-box distance cannot draw. The design's blurred ellipse and its highlight both carry gradients that reach zero at 99% of the way to the ellipse's edge, so the box's own shape never shows and a pill (which contains the ellipse) is the same picture. A box whose elliptical *edge* shows is still a vector picture ([0017](0017-transforms-and-vector-pictures.md)).

## Verification

`a_blurred_glow_matches_a_gaussian_blur_of_the_gradient` in `crates/morningstar-gpu/tests/radial.rs` compares the glow in miniature with a brute-force Gaussian blur of the gradient: every sampled pixel within 4 of 255. `crates/morningstar-ui/tests/decor.rs` holds what the UI layer emits for a blurred solid box and a blurred gradient. On Atlas the glow under the orb shows on five of ten screens, each within its fidelity limit.

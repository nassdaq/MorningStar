// Draws every display-list operation as an instanced quad: rounded rectangles
// (solid, bordered or gradient-filled), blurred shadows, glyphs sampled from
// the atlas, and images. One instance each, four vertices generated from the
// vertex index, so every kind keeps its paint order within a draw call.
// Clipping is per instance and applied as coverage, not as scissor state, for
// the same reason. The only thing that splits the instances into more than
// one draw call is a change of image texture (group 1); see `quads.rs`.
//
// All geometry is in physical pixels with the origin at the top-left.
// Colours are premultiplied linear RGB.
//
// Transforms (ADR 0017): each instance carries a 2x3 affine matrix. The
// vertex stage puts the quad's corners through it; everything the fragment
// stage evaluates (the rounded box, border, dashes, gradient, shadow, atlas
// and image coordinates) stays in the quad's own untransformed space, so a
// turned shape is the same shape, turned, with the same anti-aliased edge.
// With the identity matrix every expression below reduces exactly to what
// it was before transforms existed.
//
// Portability notes (Vulkan, Metal, GLES 3.0):
// - No derivatives and no implicit-level sampling, so the branches on `kind`
//   below are valid although `kind` is not uniform. An image's mip level is
//   computed per instance on the CPU for the same reason.
// - The fragment's pixel position is an interpolated varying rather than
//   `@builtin(position)`, so it does not depend on a backend's framebuffer
//   orientation.

struct Globals {
    viewport: vec2<f32>,
    atlas_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;
// The image the current draw call's image instances sample: sRGB-encoded,
// premultiplied, so a sample is premultiplied linear colour.
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

// Must match `quads.rs`.
const KIND_RECT: f32 = 0.0;
const KIND_GLYPH: f32 = 1.0;
const KIND_GRADIENT: f32 = 2.0;
const KIND_SHADOW: f32 = 3.0;
const KIND_IMAGE: f32 = 4.0;

struct Instance {
    // x, y, width, height
    @location(0) rect: vec4<f32>,
    // Fill colour; a gradient's start colour; a shadow's or glyph's colour;
    // an image's tint.
    @location(1) color: vec4<f32>,
    // Border colour; a gradient's end colour. Images: the texture
    // coordinates sampling is confined to, as min (xy) and max (zw).
    @location(2) color2: vec4<f32>,
    // x: corner radius. y: kind. z: border width, a shadow's Gaussian
    // sigma, or an image's mip level. w: corner radius of the rounded clip.
    @location(3) params: vec4<f32>,
    // Glyphs: x, y, width, height of the atlas region, in texels.
    // Rectangles: x is the dash length of a dashed border and y its period
    // (dash plus gap), in pixels; zero for a solid border.
    // Gradients: start point (xy) and end point (zw), in pixels.
    // Images: x, y, width, height of the source region, in texture
    // coordinates (0 to 1 across the image).
    @location(4) extra: vec4<f32>,
    // Rectangular clip as min (xy) and max (zw).
    @location(5) scissor: vec4<f32>,
    // Rounded clip as min (xy) and max (zw); its radius is `params.w`.
    @location(6) clip: vec4<f32>,
    // The transform's linear part: a point (x, y) of the quad's own space
    // goes to (x * a + y * c, x * b + y * d) plus the translation.
    @location(7) matrix: vec4<f32>,
    // xy: the transform's translation. z: screen pixels per local pixel
    // (1 unless the transform scales). w: flags; 1 is set when the
    // transform does more than move, 2 when `clip` is in the quad's own
    // space rather than on the screen.
    @location(8) placement: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    // Position relative to the rectangle's centre, in pixels.
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) half_size: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) color2: vec4<f32>,
    // x: corner radius, clamped. y: kind. z: border width, sigma or mip
    // level. w: clip corner radius.
    @location(4) @interpolate(flat) shape: vec4<f32>,
    // Glyphs: atlas coordinates. Gradients: x is the position along the
    // gradient line, 0 at the start point and 1 at the end point. Images:
    // texture coordinates. Rectangles: dash length and period of a dashed
    // border (the same at every vertex, so interpolation leaves them alone).
    @location(5) uv: vec2<f32>,
    // Position on the target, in pixels.
    @location(6) pixel: vec2<f32>,
    @location(7) @interpolate(flat) scissor: vec4<f32>,
    @location(8) @interpolate(flat) clip: vec4<f32>,
    // Position in the quad's own space, in pixels: `pixel` before the
    // transform.
    @location(9) own: vec2<f32>,
    // x: screen pixels per local pixel. y: flags.
    @location(10) @interpolate(flat) placement: vec2<f32>,
}

// Extra pixels drawn around a rectangle so its anti-aliased edge, which
// extends half a pixel outside it, is not clipped by the quad.
const FRINGE: f32 = 1.0;
// A shadow is drawn out to this many standard deviations from its edge.
// Beyond that it is below 0.2% of its peak.
const SHADOW_EXTENT: f32 = 3.0;
// The Gaussian tail beyond SHADOW_EXTENT deviations: 1 - Phi(3).
const SHADOW_TAIL: f32 = 0.00135;

@vertex
fn vs_main(@builtin(vertex_index) index: u32, instance: Instance) -> VertexOutput {
    // Triangle strip: (0,0) (1,0) (0,1) (1,1).
    let corner = vec2<f32>(f32(index & 1u), f32((index >> 1u) & 1u));
    let kind = instance.params.y;

    let edge_scale = instance.placement.z;
    let flags = instance.placement.w;
    let turned = flags == 1.0 || flags == 3.0;

    // The fringe is a screen pixel, whatever the transform's scale. Glyph
    // quads map texels to pixels one to one, so they get no fringe, unless
    // they are turned: then the quad's own edge needs anti-aliasing too.
    var fringe = FRINGE / edge_scale;
    if kind == KIND_GLYPH {
        if !turned {
            fringe = 0.0;
        }
    } else if kind == KIND_SHADOW {
        fringe = fringe + SHADOW_EXTENT * instance.params.z;
    }

    let half_size = instance.rect.zw * 0.5;
    let center = instance.rect.xy + half_size;
    let local = (corner * 2.0 - 1.0) * (half_size + fringe);
    // In the quad's own space; `screen` is where that lands.
    let pixel = center + local;
    let screen = vec2<f32>(
        pixel.x * instance.matrix.x + pixel.y * instance.matrix.z + instance.placement.x,
        pixel.x * instance.matrix.y + pixel.y * instance.matrix.w + instance.placement.y,
    );

    var uv = (instance.extra.xy + corner * instance.extra.zw) / globals.atlas_size;
    if kind == KIND_GLYPH && turned {
        // The quad has a fringe: the atlas region maps onto the rectangle
        // itself and the fringe extrapolates (and is clamped when sampled).
        uv = (instance.extra.xy + (pixel - instance.rect.xy) / instance.rect.zw * instance.extra.zw)
            / globals.atlas_size;
    } else if kind == KIND_RECT {
        uv = instance.extra.xy;
    } else if kind == KIND_GRADIENT {
        // Linear in position, so interpolating it across the quad is exact.
        let axis = instance.extra.zw - instance.extra.xy;
        uv = vec2<f32>(dot(pixel - instance.extra.xy, axis) / dot(axis, axis), 0.0);
    } else if kind == KIND_IMAGE {
        // The source region maps onto the rectangle itself; the fringe
        // around it extrapolates and is clamped in the fragment shader.
        uv = instance.extra.xy + (pixel - instance.rect.xy) / instance.rect.zw * instance.extra.zw;
    }

    var out: VertexOutput;
    out.position = vec4<f32>(
        screen.x / globals.viewport.x * 2.0 - 1.0,
        1.0 - screen.y / globals.viewport.y * 2.0,
        0.0,
        1.0,
    );
    out.local = local;
    out.half_size = half_size;
    out.color = instance.color;
    out.color2 = instance.color2;
    if kind == KIND_GLYPH && turned {
        // A turned glyph is resampled, and its fringe lies outside its
        // atlas region: the fragment stage keeps sampling within these
        // bounds (the centres of the region's outermost texels).
        out.color2 = vec4<f32>(
            instance.extra.xy + 0.5,
            instance.extra.xy + instance.extra.zw - 0.5,
        ) / globals.atlas_size.xyxy;
    }
    out.shape = vec4<f32>(
        min(instance.params.x, min(half_size.x, half_size.y)),
        kind,
        instance.params.z,
        instance.params.w,
    );
    out.uv = uv;
    out.pixel = screen;
    out.scissor = instance.scissor;
    out.clip = instance.clip;
    out.own = pixel;
    out.placement = vec2<f32>(edge_scale, flags);
    return out;
}

// Signed distance to a rounded box centred on the origin: negative inside,
// positive outside.
fn rounded_box_distance(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(point) - half_size + radius;
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

// Coverage of a dash pattern at `point`, relative to the centre of a
// rectangle with a border of `width` along the inside of its edge. The
// pattern is laid along the middle of the border, starting at the bottom end
// of the left edge and running clockwise, which is where a browser starts a
// rounded rectangle's outline. `dash.x` is the dash length, `dash.y` the
// period. Dash ends get one pixel of linear falloff.
fn dash_coverage(
    point: vec2<f32>,
    half_size: vec2<f32>,
    radius: f32,
    width: f32,
    dash: vec2<f32>,
) -> f32 {
    // The middle line: a rounded box inset by half the border width.
    let half = max(half_size - 0.5 * width, vec2<f32>(0.0));
    let r = clamp(radius - 0.5 * width, 0.0, min(half.x, half.y));
    // Half-lengths of the straight parts, and the length of one corner.
    let a = half.x - r;
    let b = half.y - r;
    let quarter = 1.5707963 * r;
    // Position relative to the corner centres: positive beyond them.
    let over = abs(point) - vec2<f32>(a, b);

    var along: f32;
    if over.x > 0.0 && over.y > 0.0 {
        // A corner. `turn` is the angle already turned through within it.
        if point.x < 0.0 && point.y < 0.0 {
            let turn = atan2(over.y, over.x);
            along = 2.0 * b + turn * r;
        } else if point.x >= 0.0 && point.y < 0.0 {
            let turn = atan2(over.x, over.y);
            along = 2.0 * b + quarter + 2.0 * a + turn * r;
        } else if point.x >= 0.0 {
            let turn = atan2(over.y, over.x);
            along = 4.0 * b + 2.0 * quarter + 2.0 * a + turn * r;
        } else {
            let turn = atan2(over.x, over.y);
            along = 4.0 * b + 3.0 * quarter + 4.0 * a + turn * r;
        }
    } else if over.x > over.y {
        // Nearer a vertical edge: the distances to the two middle lines are
        // `r - over.x` and `r - over.y`.
        if point.x < 0.0 {
            along = b - point.y;
        } else {
            along = 2.0 * b + 2.0 * quarter + 2.0 * a + (point.y + b);
        }
    } else if point.y < 0.0 {
        along = 2.0 * b + quarter + (point.x + a);
    } else {
        along = 4.0 * b + 3.0 * quarter + 2.0 * a + (a - point.x);
    }

    let phase = along - floor(along / dash.y) * dash.y;
    // Distance into the dash from its nearer end; negative in the gap.
    var inside = min(phase, dash.x - phase);
    if phase > dash.x {
        inside = -min(phase - dash.x, dash.y - phase);
    }
    return clamp(inside + 0.5, 0.0, 1.0);
}

// Abramowitz and Stegun 7.1.27; absolute error below 5e-4.
fn erf2(x: vec2<f32>) -> vec2<f32> {
    let s = sign(x);
    let a = abs(x);
    var r = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    r = r * r;
    return s - s / (r * r);
}

// Fraction of a Gaussian of deviation `sigma`, centred `x` from the middle
// of the row, that falls inside the rounded box on the row `y` from its
// middle. Exact along x.
fn shadow_row(x: f32, y: f32, sigma: f32, radius: f32, half_size: vec2<f32>) -> f32 {
    // Half-width of the box on this row: narrower where the corners curve.
    let delta = min(half_size.y - radius - abs(y), 0.0);
    let curved = half_size.x - radius + sqrt(max(0.0, radius * radius - delta * delta));
    let integral = 0.5 + 0.5 * erf2((x + vec2<f32>(-curved, curved)) * (sqrt(0.5) / sigma));
    return integral.y - integral.x;
}

// The contribution of the two rows `offset` either side of `middle` to the
// integral along y: Gaussian weight times the row's coverage along x.
fn shadow_pair(
    point: vec2<f32>,
    half_size: vec2<f32>,
    radius: f32,
    sigma: f32,
    middle: f32,
    offset: f32,
) -> f32 {
    let y = middle + vec2<f32>(-offset, offset);
    let weight = exp(-(y * y) / (2.0 * sigma * sigma)) / (2.5066283 * sigma);
    return shadow_row(point.x, point.y - y.x, sigma, radius, half_size) * weight.x
        + shadow_row(point.x, point.y - y.y, sigma, radius, half_size) * weight.y;
}

// Coverage of a rounded box blurred by a Gaussian of deviation `sigma`, at
// `point` relative to the box's centre. The blur is separable: closed form
// along x, and numerical along y, taken only over the rows the box occupies
// so that the integrand has no step in it. After Evan Wallace's "Fast
// Rounded Rectangle Shadows", with Gauss-Legendre quadrature (6 points) in
// place of the midpoint rule: several times more accurate per sample.
fn shadow_coverage(point: vec2<f32>, half_size: vec2<f32>, radius: f32, sigma: f32) -> f32 {
    let low = point.y - half_size.y;
    let high = point.y + half_size.y;
    let first = clamp(-SHADOW_EXTENT * sigma, low, high);
    let last = clamp(SHADOW_EXTENT * sigma, low, high);
    let middle = (first + last) * 0.5;
    let reach = (last - first) * 0.5;

    // Node positions and weights for the interval -1..1, by symmetric pair.
    var value = 0.0;
    value += 0.4679139346 * shadow_pair(point, half_size, radius, sigma, middle, 0.2386191861 * reach);
    value += 0.360761573 * shadow_pair(point, half_size, radius, sigma, middle, 0.6612093865 * reach);
    value += 0.1713244924 * shadow_pair(point, half_size, radius, sigma, middle, 0.9324695142 * reach);
    // The quad ends SHADOW_EXTENT deviations out, where a straight edge's
    // shadow is still SHADOW_TAIL of its peak. Shifting everything down by
    // that much makes the shadow reach zero there instead of stopping at a
    // faint step, which would show in a light glow on a dark background.
    return clamp((value * reach - SHADOW_TAIL) / (1.0 - SHADOW_TAIL), 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = in.shape.x;
    let kind = in.shape.y;
    // Screen pixels per pixel of the quad's own space: distances measured
    // in that space are multiplied by it before they become coverage, so an
    // edge fades over one screen pixel whatever the transform's scale.
    let edge_scale = in.placement.x;
    let turned = in.placement.y == 1.0 || in.placement.y == 3.0;
    let local_clip = in.placement.y >= 2.0;

    var atlas_uv = in.uv;
    if kind == KIND_GLYPH && turned {
        atlas_uv = clamp(in.uv, in.color2.xy, in.color2.zw);
    }
    // Sampled unconditionally, at an explicit level, so it is valid outside
    // uniform control flow on every backend.
    let glyph_coverage = textureSampleLevel(atlas, atlas_sampler, atlas_uv, 0.0).r;

    var color: vec4<f32>;
    if kind == KIND_GLYPH {
        color = in.color * glyph_coverage;
        if turned {
            // The mask may be solid right up to the edge of its quad (the
            // stem of an "l"); turned, that edge is no longer on the pixel
            // grid and is anti-aliased like a rectangle's.
            let edge = rounded_box_distance(in.local, in.half_size, 0.0);
            color = color * clamp(0.5 - edge * edge_scale, 0.0, 1.0);
        }
    } else if kind == KIND_SHADOW {
        color = in.color * shadow_coverage(in.local, in.half_size, radius, in.shape.z);
    } else {
        let distance = rounded_box_distance(in.local, in.half_size, radius);
        // One pixel of linear falloff centred on the edge.
        let outer = clamp(0.5 - distance * edge_scale, 0.0, 1.0);
        if kind == KIND_GRADIENT {
            color = mix(in.color, in.color2, clamp(in.uv.x, 0.0, 1.0)) * outer;
        } else if kind == KIND_IMAGE {
            // Explicit level, so sampling here, inside a branch, is valid
            // everywhere, and other kinds do not pay for the fetch. The clamp
            // keeps filtering from reaching past the source region into a
            // neighbouring sprite.
            let coordinates = clamp(in.uv, in.color2.xy, in.color2.zw);
            let texel = textureSampleLevel(image, image_sampler, coordinates, in.shape.z);
            // Premultiplied texel times premultiplied tint.
            color = texel * in.color * outer;
        } else {
            // The pixel splits into three parts: `inner` is fill alone,
            // `outer - inner` is border over fill, the rest is empty. Moving
            // inwards by the border width gives the inner edge, including
            // its smaller corner radius. With no border the two coincide.
            let inner = clamp(0.5 - (distance + in.shape.z) * edge_scale, 0.0, 1.0);
            var stroke = in.color2;
            if in.uv.x > 0.0 {
                stroke = stroke * dash_coverage(in.local, in.half_size, radius, in.shape.z, in.uv);
            }
            let ring = stroke + in.color * (1.0 - stroke.a);
            color = in.color * inner + ring * (outer - inner);
        }
    }

    // Rectangular clip: the exact fraction of the pixel inside it.
    let overlap = min(in.scissor.zw, in.pixel + 0.5) - max(in.scissor.xy, in.pixel - 0.5);
    let scissor_coverage = clamp(overlap.x, 0.0, 1.0) * clamp(overlap.y, 0.0, 1.0);
    // Rounded clip: the same falloff as a rounded rectangle's edge. A clip
    // pushed inside a transform is a rectangle of the quad's own space and
    // is evaluated there.
    var clip_point = in.pixel;
    var clip_scale = 1.0;
    if local_clip {
        clip_point = in.own;
        clip_scale = edge_scale;
    }
    let clip_half = (in.clip.zw - in.clip.xy) * 0.5;
    let clip_center = in.clip.xy + clip_half;
    let clip_distance = rounded_box_distance(clip_point - clip_center, clip_half, in.shape.w);
    let clip_coverage = clamp(0.5 - clip_distance * clip_scale, 0.0, 1.0);

    return color * (scissor_coverage * clip_coverage);
}

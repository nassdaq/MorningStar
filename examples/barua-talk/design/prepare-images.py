"""Writes the PNG files the app draws (assets/images/*.png) from the design's
photos (assets/images/*.jpg). The framework decodes PNG only, so the
conversion happens here, once, and the results are committed.

- The seven portraits and `shop` are converted as they are.
- `baraka-video` is cut to the part page 04 shows: `object-fit: cover;
  object-position: 30% 0` in 390 x 844 keeps a 627 x 1356 window 100 px from
  the left. The app's pictures are always centred, so the cut is made here.
- `self-camera` is cut to the part its 104 x 146 tile shows (`object-position:
  50% 20%`: a 570 x 800 window in the middle).
- `backdrop-<person>` is what page 03 puts behind an incoming call: the
  portrait 510 x 964 (`object-fit: cover`), 60 dp off the screen on every
  side, under `filter: blur(40px) brightness(0.45)`, over the page colour.
  The framework blurs a box's background and not a picture, so the blurred
  picture is made here, as CSS defines it (a Gaussian of 40 px on the
  premultiplied sRGB values, then the brightness), at one pixel per 2 dp:
  nothing finer than that survives the blur.

Needs Pillow and numpy. Run from examples/barua-talk:
    python3 design/prepare-images.py"""
import numpy as np
from PIL import Image

DIR = 'assets/images/'
PEOPLE = ['neema', 'support', 'asha', 'zawadi', 'juma', 'baraka', 'hamisi']
# Who can ring: the people, not the user or a room's picture.
CALLERS = ['asha', 'zawadi', 'juma', 'baraka', 'hamisi']
PAGE = (0x0D, 0x0F, 0x15)


def load(name):
    return Image.open(DIR + name + '.jpg').convert('RGB')


def save(image, name):
    image.save(DIR + name + '.png', optimize=True)
    print(name + '.png', image.size)


def gaussian(pixels, sigma):
    """A Gaussian blur of a float image, rows then columns."""
    reach = int(sigma * 4)
    kernel = np.exp(-0.5 * (np.arange(-reach, reach + 1) / sigma) ** 2)
    kernel /= kernel.sum()
    for axis in (0, 1):
        pixels = np.apply_along_axis(lambda line: np.convolve(line, kernel, mode='same'), axis, pixels)
    return pixels


def backdrop(portrait):
    # In half-dp pixels: the screen is 195 x 422, the picture 255 x 482 at
    # (-30, -30), the blur 20.
    scale = 482 / portrait.height
    cover = portrait.resize((round(portrait.width * scale), 482), Image.LANCZOS)
    left = (cover.width - 255) // 2
    cover = cover.crop((left, 0, left + 255, 482))
    pad = 90  # room for the blur to fade out in
    canvas = np.zeros((482 + 2 * pad, 255 + 2 * pad, 4), dtype=np.float32)
    canvas[pad:pad + 482, pad:pad + 255, :3] = np.asarray(cover, dtype=np.float32)
    canvas[pad:pad + 482, pad:pad + 255, 3] = 255.0
    blurred = gaussian(canvas, 20.0)
    window = blurred[pad + 30:pad + 30 + 422, pad + 30:pad + 30 + 195]
    alpha = window[..., 3:4] / 255.0
    colour = window[..., :3] * 0.45  # premultiplied: brightness scales it as it is
    out = colour + np.asarray(PAGE, dtype=np.float32) * (1.0 - alpha)
    return Image.fromarray(np.clip(out + 0.5, 0, 255).astype(np.uint8), 'RGB')


for person in PEOPLE:
    portrait = load(person)
    save(portrait, person)
    if person in CALLERS:
        save(backdrop(portrait), 'backdrop-' + person)
save(load('shop'), 'shop')
save(load('baraka-video').crop((100, 0, 727, 1356)), 'baraka-video')
save(load('self-camera').crop((110, 0, 680, 800)), 'self-camera')

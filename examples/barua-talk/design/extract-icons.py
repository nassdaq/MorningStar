"""Writes assets/svg/<name>.svg for every distinct inline <svg> of the design's
HTML: the design's own markup, with `sc-camel-view-box` spelt `viewBox`, the
namespace added, the size set to the view box and the one colour of the drawing
made `currentColor` (the design writes `currentColor`, `#FFFFFF` or `#E3E6EE`),
so an icon takes its colour where it is drawn. The same outline at two stroke
widths is two files.

Two drawings are left out and built in the app instead: the waveform of the
voice message (sixteen bars in two colours, which the app colours bar by bar as
the message plays) and the dot of the record button, whose colour is a
variable of the page (`{{recDot}}`); the ring around it is `record.svg`.

Run from examples/barua-talk:  python3 design/extract-icons.py"""
import re, glob

# (the drawing, its stroke width or "fill") -> the file's name
NAMES = {
 ('<circle cx="11" cy="11" r="7"></circle><path d="M20 20l-3.6-3.6"></path>', '1.8'): 'search',
 ('<rect x="4" y="4" width="6" height="6" rx="1.5"></rect><rect x="14" y="4" width="6" height="6" rx="1.5"></rect><rect x="4" y="14" width="6" height="6" rx="1.5"></rect><rect x="14" y="14" width="6" height="6" rx="1.5"></rect>', '1.8'): 'dial',
 ('<path d="M12 5v14M5 12h14"></path>', '2'): 'plus',
 ('<path d="M12 5v14M5 12h14"></path>', '2.4'): 'plus-bold',
 ('<rect x="3" y="6" width="13" height="12" rx="3"></rect><path d="M16 10l5-3v10l-5-3"></path>', '2'): 'video-small',
 ('<rect x="3" y="6" width="13" height="12" rx="3"></rect><path d="M16 10l5-3v10l-5-3"></path>', '1.8'): 'video',
 ('<path d="M5 4h4l2 5-2.5 1.5a11 11 0 005 5L15 13l5 2v4a2 2 0 01-2 2A16 16 0 013 6a2 2 0 012-2z"></path>', '2'): 'phone-small',
 ('<path d="M5 4h4l2 5-2.5 1.5a11 11 0 005 5L15 13l5 2v4a2 2 0 01-2 2A16 16 0 013 6a2 2 0 012-2z"></path>', '1.8'): 'phone',
 ('<path d="M12 3l9 8v9a1 1 0 01-1 1h-5v-6H9v6H4a1 1 0 01-1-1v-9z"></path>', 'fill'): 'home',
 ('<path d="M6 16V11a6 6 0 0112 0v5l2 2H4z"></path><path d="M10 21h4"></path>', '1.8'): 'bell',
 ('<circle cx="9" cy="8" r="3.5"></circle><path d="M2.5 20a6.5 6.5 0 0113 0"></path><circle cx="17.5" cy="9" r="2.5"></circle><path d="M17.5 14.5A4.5 4.5 0 0122 19"></path>', '1.8'): 'contacts',
 ('<path d="M15 5l-7 7 7 7"></path>', '2'): 'back',
 ('<path d="M8 5l12 7-12 7z"></path>', 'fill'): 'play',
 ('<rect x="4" y="4" width="16" height="6" rx="2"></rect><rect x="4" y="14" width="16" height="6" rx="2"></rect><path d="M8 7h.01M8 17h.01"></path>', '2'): 'server',
 ('<path d="M2 12s3.5-6.5 10-6.5S22 12 22 12s-3.5 6.5-10 6.5S2 12 2 12z"></path><circle cx="12" cy="12" r="2.5"></circle>', '2'): 'seen',
 ('<path d="M20 11.5l-8 8a5 5 0 01-7-7l8.5-8.5a3.3 3.3 0 014.7 4.7L9.7 17.2a1.7 1.7 0 01-2.4-2.4L15 7"></path>', '1.8'): 'attach',
 ('<path d="M3 11.5L21 3l-7.5 18-2.6-7.4z"></path>', 'fill'): 'send',
 ('<path d="M4 5h16v11H9l-5 4z"></path>', '1.8'): 'message',
 ('<path d="M3 14.5c5-5 13-5 18 0l-2.5 2.5-3.5-1.5v-2.6a9 9 0 00-6 0v2.6L5.5 17z"></path>', '2'): 'hang-up',
 ('<circle cx="10" cy="8" r="3.5"></circle><path d="M3.5 20a6.5 6.5 0 0113 0M19 8v6M16 11h6"></path>', '1.8'): 'add-person',
 ('<path d="M4 9h3l5-4v14l-5-4H4zM16 9a4 4 0 010 6M18.5 6.5a7.5 7.5 0 010 11"></path>', '1.8'): 'speaker',
 ('<rect x="3" y="4" width="18" height="13" rx="2"></rect><path d="M12 14V8M9 10.5L12 8l3 2.5M8 21h8"></path>', '1.8'): 'share-screen',
 ('<circle cx="12" cy="12" r="8.5"></circle>', '1.8'): 'record',
 ('<rect x="9" y="3" width="6" height="11" rx="3"></rect><path d="M5.5 11a6.5 6.5 0 0013 0M12 17.5V21"></path>', '1.8'): 'mic',
 ('<circle cx="6" cy="6" r="1.7"></circle><circle cx="12" cy="6" r="1.7"></circle><circle cx="18" cy="6" r="1.7"></circle><circle cx="6" cy="12" r="1.7"></circle><circle cx="12" cy="12" r="1.7"></circle><circle cx="18" cy="12" r="1.7"></circle><circle cx="6" cy="18" r="1.7"></circle><circle cx="12" cy="18" r="1.7"></circle><circle cx="18" cy="18" r="1.7"></circle>', 'fill'): 'keypad',
}

done = {}
for f in sorted(glob.glob('design/html/*.html')):
    for m in re.finditer(r'<svg ([^>]*)>(.*?)</svg>', open(f).read(), re.S):
        attrs, inner = m.group(1), m.group(2).strip()
        if 'sc-camel-view-box="0 0 130 32"' in attrs:
            continue  # the waveform: bars drawn by the app
        assert 'sc-camel-view-box="0 0 24 24"' in attrs, attrs
        # The record button's dot is a variable of the page: the app draws it.
        inner = re.sub(r'<circle cx="12" cy="12" r="4" fill="\{\{recDot\}\}" stroke="none"></circle>', '', inner)
        if 'fill="currentColor"' in attrs:
            key, paint = 'fill', 'fill="currentColor"'
        else:
            key = re.search(r'stroke-width="([^"]+)"', attrs).group(1)
            assert re.search(r'stroke="(currentColor|#FFFFFF|#E3E6EE)"', attrs), attrs
            paint = f'fill="none" stroke="currentColor" stroke-width="{key}"'
            for extra in ('stroke-linecap="round"', 'stroke-linejoin="round"'):
                if extra in attrs:
                    paint += ' ' + extra
        name = NAMES[(inner, key)]
        svg = f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" {paint}>{inner}</svg>\n'
        assert done.setdefault(name, svg) == svg, name
for name, svg in done.items():
    open(f'assets/svg/{name}.svg', 'w').write(svg)
assert len(done) == len(NAMES), set(NAMES.values()) - set(done)
print(len(done), 'icons')

"""Writes assets/svg/<name>.svg for every distinct inline <svg> of the design's
HTML: the design's own markup, with `sc-camel-view-box` spelt `viewBox`, the
namespace added, the size set to the view box and the colour `currentColor`.
Run from examples/atlas."""
import re, glob, sys
NAMES = {
 ('<rect x="3" y="5" width="18" height="14" rx="2"></rect><path d="M3 7l9 6 9-6"></path>','1.6'):'mail',
 ('<rect x="5" y="11" width="14" height="9" rx="2"></rect><path d="M8 11V8a4 4 0 0 1 8 0v3"></path>','1.6'):'lock',
 ('<circle cx="12" cy="8" r="4"></circle><path d="M4 20c1.5-4 5-5 8-5s6.5 1 8 5"></path>','1.6'):'user',
 ('<path d="M7 3h7l4 4v14H7z"></path><path d="M14 3v4h4"></path>','1.6'):'file',
 ('<path d="M7 3h7l4 4v14H7z"></path><path d="M14 3v4h4"></path>','1.8'):'file-chip',
 ('<path d="M9 6l6 6-6 6"></path>','2'):'chevron-right',
 ('<circle cx="12" cy="12" r="9"></circle><path d="M3 12h18M12 3c3 3.2 3 14.8 0 18M12 3c-3 3.2-3 14.8 0 18"></path>','1.6'):'globe',
 ('<rect x="3" y="5" width="18" height="14" rx="2"></rect><circle cx="8.5" cy="10" r="1.5"></circle><path d="M21 16l-5-5-8 8"></path>','1.6'):'image',
 ('<path d="M6 9l6 6 6-6"></path>','2'):'chevron-down',
 ('<path d="M6 9l6 6 6-6"></path>','1.9'):'minimise',
 ('<path d="M12 5v14M5 12h14"></path>','1.8'):'plus',
 ('<rect x="9" y="3" width="6" height="11" rx="3"></rect><path d="M5 11a7 7 0 0 0 14 0M12 18v3"></path>','1.7'):'mic',
 ('<rect x="9" y="3" width="6" height="11" rx="3"></rect><path d="M5 11a7 7 0 0 0 14 0M12 18v3"></path>','1.6'):'mic-large',
 ('<path d="M12 19V5M6 11l6-6 6 6"></path>','2'):'send',
 ('<path d="M3 11l9-8 9 8v9a1 1 0 0 1-1 1h-5v-6h-6v6H4a1 1 0 0 1-1-1z"></path>','1.6'):'home',
 ('<path d="M3 11l9-8 9 8v9a1 1 0 0 1-1 1h-5v-6h-6v6H4a1 1 0 0 1-1-1z"></path>','1.9'):'home-active',
 ('<circle cx="12" cy="12" r="9"></circle><path d="M15.5 8.5l-2 5-5 2 2-5z"></path>','1.6'):'compass',
 ('<circle cx="12" cy="12" r="9"></circle><path d="M15.5 8.5l-2 5-5 2 2-5z"></path>','1.9'):'compass-active',
 ('<path d="M5 4v16M9.5 4v16M14 5.5l3.5 14"></path>','1.6'):'library',
 ('<path d="M15 6l-6 6 6 6"></path>','1.9'):'back',
 ('<path d="M5 12.5l4.5 4.5L19 7.5"></path>','2.4'):'check',
 ('<path d="M5 12.5l4.5 4.5L19 7.5"></path>','3'):'check-bold',
 ('<path d="M4 10v4M8 6v12M12 3v18M16 7v10M20 10v4"></path>','2'):'waveform',
 ('<path d="M4 10v4M8 6v12M12 3v18M16 7v10M20 10v4"></path>','1.6'):'waveform-light',
 ('<path d="M4 8h3l2-3h6l2 3h3v11H4z"></path><circle cx="12" cy="13" r="3.5"></circle>','1.6'):'camera',
 ('<path d="M6 6l12 12M18 6L6 18"></path>','2.2'):'close',
 ('<circle cx="11" cy="11" r="7"></circle><path d="M20 20l-3.5-3.5"></path>','1.6'):'search',
 ('<path d="M4 6h9M8.5 4v2M11 6c-.5 3.5-3 6-6.5 7M6 6c.5 3.5 3 6 6 7M13 20l4-9 4 9M14.5 17h5"></path>','1.6'):'translate',
 ('<rect x="3" y="4" width="18" height="16" rx="2"></rect><path d="M3 10h18M9 4v16"></path>','1.6'):'table',
}
done=set()
for f in sorted(glob.glob('design/html/*.html')):
    for m in re.finditer(r'<svg ([^>]*)>(.*?)</svg>', open(f).read(), re.S):
        attrs, inner = m.group(1), m.group(2)
        assert 'sc-camel-view-box="0 0 24 24" fill="none"' in attrs and 'stroke-linecap="round" stroke-linejoin="round"' in attrs
        width = re.search(r'stroke-width="([^"]+)"', attrs).group(1)
        name = NAMES[(inner, width)]
        if name in done: continue
        done.add(name)
        open(f'assets/svg/{name}.svg','w').write(
            f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round">{inner}</svg>\n')
assert len(done)==len(NAMES), set(NAMES.values())-done
print(len(done),'icons')

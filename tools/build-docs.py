#!/usr/bin/env python3
"""Builds the documentation site (site/) from the Markdown in docs/.

    python3 tools/build-docs.py            write site/**/*.html and the search index
    python3 tools/build-docs.py --check    fail if site/ is not what the sources give

Needs nothing but Python 3 (standard library). ADR 0044 says why.

What is a source and what is output:

  docs/**/*.md              the prose, once. Pages are listed in docs/site.json.
  examples/docs-snippets    the code samples, compiled with the workspace.
  crates/**/*.rs            the tables of methods, read from the doc comments.
  site/assets, site/shots   written by hand or by tools/docs-screenshots.sh.
  site/**/*.html            OUTPUT. Never edit: change the source and rebuild.
  site/assets/search-index.js   OUTPUT.

Directives, each on a line of its own in a Markdown file:

  {{snippet path#name}}             a region of a source file, between
                                    `// docs:begin name` and `// docs:end name`
  {{snippet path#name title="x"}}   the same under a file-name bar
  {{include path}}                  a whole text file as a code block
  {{api path Target}}               the public methods of `impl Target` (or of
                                    `macro:name`), with the first sentence of
                                    each doc comment; `only=a,b` and
                                    `except=a,b` choose among them
  {{variants path Enum}}            an enum's variants and their doc comments
  {{fields path Struct}}            a struct's public fields
  {{cards}} ... {{/cards}}          a list of `[Title](link): text` as cards
  {{steps}} ... {{/steps}}          an ordered list as numbered steps
  {{shots}} ... {{/shots}}          images as a row of phone frames

Every relative link is checked: one that leads to a Markdown page of the site
becomes a link to that page, one that leads into site/ a link to that file,
and one that leads to another file of the repository is shown as its path
(the site cannot link into a private repository). A link to nothing, or to a
heading that does not exist, fails the build.
"""

import html
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DOCS = os.path.join(ROOT, "docs")
SITE = os.path.join(ROOT, "site")
NAME = "Morningstar"


class BuildError(Exception):
    pass


def read(path):
    with open(os.path.join(ROOT, path), encoding="utf-8") as file:
        return file.read()


def esc(text):
    return html.escape(text, quote=False)


def attr(text):
    return html.escape(text, quote=True)


# --------------------------------------------------------------- highlighting

RUST_KEYWORDS = set(
    "as async await break const continue crate dyn else enum extern false fn for if impl in let "
    "loop match mod move mut pub ref return self Self static struct super trait true type unsafe "
    "use where while".split()
)
JS_KEYWORDS = set(
    "async await break case catch class const continue default else export false for from "
    "function if import let new null return switch throw true try typeof undefined var while".split()
)
TOKEN = {
    "rust": re.compile(
        r"(?P<c>//[^\n]*|/\*.*?\*/)"
        r'|(?P<s>b?r#*"(?:.|\n)*?"#*|b?"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\')'
        r"|(?P<a>#!?\[[^\]\n]*\])"
        r"|(?P<m>[A-Za-z_][A-Za-z0-9_]*!)"
        r"|(?P<w>[A-Za-z_][A-Za-z0-9_]*)"
        r"|(?P<n>\b\d[\d_]*(?:\.\d[\d_]*)?(?:[a-z]\w*)?)",
        re.S,
    ),
    "js": re.compile(
        r"(?P<c>//[^\n]*|/\*.*?\*/)"
        r"|(?P<s>\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'|`(?:\\.|[^`\\])*`)"
        r"|(?P<w>[A-Za-z_$][A-Za-z0-9_$]*)"
        r"|(?P<n>\b\d[\d_]*(?:\.\d+)?)",
        re.S,
    ),
}


def highlight(code, lang):
    """Colours code with the five classes the design uses: k s c f t."""
    lang = {"javascript": "js", "json": "js", "bash": "sh", "shell": "sh", "console": "sh"}.get(lang, lang)
    if lang in ("rust", "js"):
        keywords = RUST_KEYWORDS if lang == "rust" else JS_KEYWORDS
        out, last = [], 0
        for match in TOKEN[lang].finditer(code):
            out.append(esc(code[last : match.start()]))
            last = match.end()
            kind, word = match.lastgroup, match.group()
            if kind == "c":
                out.append('<span class="c">%s</span>' % esc(word))
            elif kind in ("s", "n"):
                out.append('<span class="s">%s</span>' % esc(word))
            elif kind in ("a", "m"):
                out.append('<span class="k">%s</span>' % esc(word))
            elif word in keywords:
                out.append('<span class="k">%s</span>' % esc(word))
            elif word[0].isupper():
                out.append('<span class="t">%s</span>' % esc(word))
            elif code[last : last + 1] == "(":
                out.append('<span class="f">%s</span>' % esc(word))
            else:
                out.append(esc(word))
        out.append(esc(code[last:]))
        return "".join(out)
    if lang == "sh":
        lines = []
        for line in code.split("\n"):
            stripped = line.lstrip()
            if stripped.startswith("#"):
                lines.append('<span class="c">%s</span>' % esc(line))
                continue
            comment = ""
            cut = re.search(r"\s#\s", line)
            if cut:
                line, comment = line[: cut.start()], '<span class="c">%s</span>' % esc(line[cut.start() :])
            match = re.match(r"(\s*(?:[A-Z_][A-Z0-9_]*=\S*\s+)*)([\w./-]+)(.*)", line, re.S)
            if match and match.group(2):
                rest = re.sub(r"(&quot;.*?&quot;|'[^']*')", r'<span class="s">\1</span>', attr(match.group(3)))
                rest = rest.replace("&#x27;", "'")
                lines.append("%s<span class=\"f\">%s</span>%s%s" % (esc(match.group(1)), esc(match.group(2)), rest, comment))
            else:
                lines.append(esc(line) + comment)
        return "\n".join(lines)
    if lang == "toml":
        lines = []
        for line in code.split("\n"):
            if line.lstrip().startswith("#"):
                lines.append('<span class="c">%s</span>' % esc(line))
            elif re.match(r"\s*\[.*\]\s*$", line):
                lines.append('<span class="t">%s</span>' % esc(line))
            else:
                match = re.match(r"(\s*)([\w.-]+)(\s*=\s*)(.*)", line)
                if match:
                    value = re.sub(r"(&quot;.*?&quot;)", r'<span class="s">\1</span>', attr(match.group(4)))
                    value = re.sub(r"(\s#\s.*)$", r'<span class="c">\1</span>', value)
                    lines.append("%s<span class=\"f\">%s</span>%s%s" % (match.group(1), esc(match.group(2)), esc(match.group(3)), value))
                else:
                    lines.append(esc(line))
        return "\n".join(lines)
    return esc(code)


def code_block(code, lang, title=None):
    code = code.rstrip("\n")
    head = '<div class="code-head"><span>%s</span></div>' % esc(title) if title else ""
    return '<div class="code%s">%s<pre><code>%s</code></pre></div>' % (
        " titled" if title else "",
        head,
        highlight(code, lang or ""),
    )


# ------------------------------------------------------------ reading sources


def region(path, name):
    """The lines of `path` between `// docs:begin name` and `// docs:end name`."""
    lines = read(path).split("\n")
    begin = end = None
    for index, line in enumerate(lines):
        mark = line.strip()
        if mark == "// docs:begin " + name:
            begin = index + 1
        elif mark == "// docs:end " + name and begin is not None:
            end = index
            break
    if begin is None or end is None:
        raise BuildError("%s has no region `%s`" % (path, name))
    body = [line for line in lines[begin:end] if not line.strip().startswith("// docs:")]
    indent = min((len(line) - len(line.lstrip()) for line in body if line.strip()), default=0)
    return "\n".join(line[indent:] for line in body)


def rustdoc_inline(text):
    """A doc comment's sentence as inline HTML: intra-doc links become code."""
    text = re.sub(r"\[(`[^`]+`)\]\([^)]*\)", r"\1", text)
    text = re.sub(r"\[(`[^`]+`)\](?:\[[^\]]*\])?", r"\1", text)
    text = re.sub(r"\[([^\]]+)\]\((?:Self|crate|super)[^)]*\)", r"\1", text)
    return inline(text, None)


def first_sentence(doc_lines):
    paragraph = []
    for line in doc_lines:
        if not line.strip():
            break
        if line.strip().startswith("```"):
            break
        paragraph.append(line.strip())
    text = " ".join(paragraph)
    match = re.search(r"(?<=[^.A-Z][.:])\s+(?=[A-Z`\[(])", text)
    if match and match.start() > 20:
        text = text[: match.start()]
    return text.rstrip(":").rstrip()


def block_of(path, start_pattern):
    lines = read(path).split("\n")
    for index, line in enumerate(lines):
        if re.match(start_pattern, line):
            for end in range(index + 1, len(lines)):
                if lines[end].startswith("}"):
                    return lines[index + 1 : end]
    raise BuildError("%s has nothing matching %s" % (path, start_pattern))


def documented(lines, item_pattern):
    """(match, doc lines) for every line of `lines` that matches, with the
    `///` lines right above it. Items marked `#[doc(hidden)]` are left out."""
    doc, hidden, found = [], False, []
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("///"):
            doc.append(stripped[3:].lstrip(" ") if stripped != "///" else "")
            continue
        if stripped.startswith("#["):
            hidden = hidden or "doc(hidden)" in stripped
            continue
        match = re.match(item_pattern, line)
        if match and not hidden:
            found.append((match, doc, index))
        doc, hidden = [], False
    return found


def api_table(path, target, only=None, hide=None):
    if target.startswith("macro:"):
        lines = block_of(path, r"^macro_rules! %s\b" % re.escape(target[6:]))
    else:
        lines = block_of(path, r"^impl(<[^>]*>)?\s+%s(<[^>]*>)?\s*\{" % re.escape(target))
    rows = []
    for match, doc, index in documented(lines, r"^\s*pub (?:const )?fn (\w+)"):
        name = match.group(1)
        if (only and name not in only) or (hide and name in hide):
            continue
        signature = lines[index].strip()
        step = index
        while "{" not in signature and not signature.endswith(";") and step < index + 12:
            step += 1
            signature += " " + lines[step].strip()
        signature = re.sub(r"\s*(\{|where\b).*$", "", signature)
        signature = re.sub(r"^pub (const )?fn ", "", signature)
        signature = re.sub(r"\(\s*(?:mut )?self,?\s*", "(", signature)
        signature = re.sub(r"\(\s*&(?:mut )?self,?\s*", "(", signature)
        signature = re.sub(r"\s*->\s*Self$", "", signature)
        signature = re.sub(r",\s*\)", ")", signature).replace("( ", "(")
        signature = signature.replace("std::borrow::", "").replace("std::time::", "").replace("std::hash::", "")
        signature = signature.replace("morningstar_core::", "").replace("impl Into<Cow<'static, str>>", "impl Into<Cow<str>>")
        rows.append((signature, first_sentence(doc)))
    if not rows:
        raise BuildError("no public functions found for %s in %s" % (target, path))
    if only:
        missing = set(only) - {re.match(r"\w+", row[0]).group() for row in rows}
        if missing:
            raise BuildError("%s in %s has no %s" % (target, path, ", ".join(sorted(missing))))
    body = "".join(
        "<tr><td><code>%s</code></td><td>%s</td></tr>" % (esc(signature), rustdoc_inline(doc))
        for signature, doc in rows
    )
    return '<div class="table"><table class="api"><thead><tr><th>Method</th><th>What it does</th></tr></thead><tbody>%s</tbody></table></div>' % body


def members_table(path, kind, name):
    lines = block_of(path, r"^pub %s %s\b" % (kind, re.escape(name)))
    pattern = r"^    (\w+)" if kind == "enum" else r"^    pub (\w+): (.*?),?$"
    rows = []
    for match, doc, _ in documented(lines, pattern):
        label = match.group(1)
        if kind == "struct":
            label += ": " + match.group(2).replace("morningstar_core::", "")
        rows.append((label, first_sentence(doc)))
    if not rows:
        raise BuildError("no members found for %s %s in %s" % (kind, name, path))
    head = "Variant" if kind == "enum" else "Field"
    body = "".join("<tr><td><code>%s</code></td><td>%s</td></tr>" % (esc(a), rustdoc_inline(b)) for a, b in rows)
    return '<div class="table"><table class="api"><thead><tr><th>%s</th><th>Meaning</th></tr></thead><tbody>%s</tbody></table></div>' % (head, body)


# ------------------------------------------------------------------ Markdown


def slug(text, used):
    base = re.sub(r"[^\w\- ]", "", re.sub(r"<[^>]+>", "", text).lower()).strip().replace(" ", "-") or "section"
    candidate, count = base, 1
    while candidate in used:
        candidate = "%s-%d" % (base, count)
        count += 1
    used.add(candidate)
    return candidate


class Page:
    def __init__(self, source, out):
        self.source = source  # path from the repository root
        self.out = out  # path inside site/
        self.title = ""
        self.nav_title = ""
        self.headings = []  # (level, id, html)
        self.ids = {"search", "results", "nav", "content"}  # the page shell's own ids
        self.links = []  # (target out path, fragment, source line text)
        self.sections = []  # (id, heading text, plain text) for the search index
        self.body = ""
        self.section = None
        self.group = None
        self.lead = ""


PAGES = {}  # source path -> Page
CURRENT = None


def rel(from_out, to_out):
    return os.path.relpath(os.path.join(SITE, to_out), os.path.dirname(os.path.join(SITE, from_out))).replace(os.sep, "/")


def resolve(target, page):
    """An href for a link written in a Markdown source, or None if the target
    is a file of the repository that the site does not hold."""
    if re.match(r"[a-z][a-z0-9+.-]*:", target):
        return target
    path, _, fragment = target.partition("#")
    if not path:
        page.links.append((page.out, fragment, target))
        return "#" + fragment
    absolute = os.path.normpath(os.path.join(ROOT, os.path.dirname(page.source), path))
    inside = os.path.relpath(absolute, ROOT).replace(os.sep, "/")
    if inside.startswith(".."):
        raise BuildError("%s links outside the repository: %s" % (page.source, target))
    if os.path.isdir(absolute) and os.path.exists(os.path.join(absolute, "README.md")):
        inside = inside + "/README.md"
        absolute = os.path.join(absolute, "README.md")
    if inside in PAGES:
        page.links.append((PAGES[inside].out, fragment, target))
        return rel(page.out, PAGES[inside].out) + ("#" + fragment if fragment else "")
    if not os.path.exists(absolute):
        raise BuildError("%s links to %s, which does not exist" % (page.source, target))
    if inside.startswith("site/"):
        return rel(page.out, inside[5:])
    return None


def inline(text, page):
    """Inline Markdown to HTML. `page` is None for text that has no links to
    resolve (a doc comment)."""
    stash = []

    def keep(markup):
        stash.append(markup)
        return "\x00%d\x00" % (len(stash) - 1)

    def code(match):
        return keep("<code>%s</code>" % esc(match.group(2).strip() if match.group(1) == "``" else match.group(2)))

    text = re.sub(r"(``|`)(.+?)\1", code, text)

    def image(match):
        alt, target = match.group(1), match.group(2)
        src = resolve(target, page) if page else target
        if src is None:
            raise BuildError("%s shows an image that is not in site/: %s" % (page.source, target))
        return keep('<img src="%s" alt="%s" loading="lazy">' % (attr(src), attr(alt)))

    text = re.sub(r"!\[([^\]]*)\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)", image, text)

    def link(match):
        label, target = match.group(1), match.group(2)
        label_html = inline_rest(label)
        if page is None:
            return keep(label_html)
        href = resolve(target, page)
        if href is None:
            path = os.path.relpath(os.path.normpath(os.path.join(ROOT, os.path.dirname(page.source), target.partition("#")[0])), ROOT)
            plain = re.sub(r"\x00\d+\x00", "", label)
            shown = label_html if path in plain or plain in path else "%s (<code>%s</code>)" % (label_html, esc(path))
            return keep('<span class="repo" title="A file of the repository: %s">%s</span>' % (attr(path), shown))
        outside = ' rel="noopener"' if re.match(r"https?:", href) else ""
        return keep('<a href="%s"%s>%s</a>' % (attr(href), outside, label_html))

    def inline_rest(piece):
        piece = esc(piece)
        piece = re.sub(r"\*\*(.+?)\*\*", r"<strong>\1</strong>", piece)
        piece = re.sub(r"(?<![\w*])\*(?!\s)(.+?)(?<!\s)\*(?![\w*])", r"<em>\1</em>", piece)
        piece = re.sub(r"(?<![\w_])_(?!\s)([^_]+?)(?<!\s)_(?![\w_])", r"<em>\1</em>", piece)
        return piece

    text = re.sub(r"\[((?:[^\[\]]|\[[^\]]*\])+)\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)", link, text)
    text = re.sub(r"<(https?://[^>\s]+)>", lambda m: keep('<a href="%s" rel="noopener">%s</a>' % (attr(m.group(1)), esc(m.group(1)))), text)
    text = inline_rest(text)
    while "\x00" in text:
        text = re.sub(r"\x00(\d+)\x00", lambda m: stash[int(m.group(1))], text)
    return text


def plain(markup):
    return html.unescape(re.sub(r"<[^>]+>", "", markup))


LIST_ITEM = re.compile(r"^(\s*)([-*+]|\d+[.)])\s+(.*)$")
DIRECTIVE = re.compile(r"^\{\{(/?\w+)(.*?)\}\}\s*$")


def split_row(line):
    cells, cell, in_code, index = [], "", False, 0
    line = line.strip()
    if line.startswith("|"):
        line = line[1:]
    if line.endswith("|") and not line.endswith("\\|"):
        line = line[:-1]
    while index < len(line):
        char = line[index]
        if char == "`":
            in_code = not in_code
        if char == "\\" and line[index + 1 : index + 2] == "|":
            cell += "|"
            index += 2
            continue
        if char == "|" and not in_code:
            cells.append(cell.strip())
            cell = ""
        else:
            cell += char
        index += 1
    cells.append(cell.strip())
    return cells


def directive_args(text):
    args, options = [], {}
    for token in re.findall(r'(\w+="[^"]*"|\S+)', text):
        key, eq, value = token.partition("=")
        if eq and re.match(r"^\w+$", key):
            options[key] = value.strip('"')
        else:
            args.append(token)
    return args, options


def blocks(lines, page, top=False):
    """Block-level Markdown to HTML."""
    out, index = [], 0

    def paragraph_end(at):
        line = lines[at]
        return (
            not line.strip()
            or line.lstrip().startswith(("```", "#", "|", ">"))
            or LIST_ITEM.match(line)
            or DIRECTIVE.match(line.strip())
            or re.match(r"^\s*(---+|\*\*\*+)\s*$", line)
        )

    while index < len(lines):
        line = lines[index]
        stripped = line.strip()
        if not stripped:
            index += 1
            continue
        fence = re.match(r"^(\s*)```+\s*(\w*)(.*)$", line)
        if fence:
            indent, lang = len(fence.group(1)), fence.group(2)
            _, options = directive_args(fence.group(3))
            body, index = [], index + 1
            while index < len(lines) and not lines[index].strip().startswith("```"):
                body.append(lines[index][indent:] if lines[index][:indent].strip() == "" else lines[index])
                index += 1
            index += 1
            out.append(code_block("\n".join(body), lang, options.get("title")))
            continue
        heading = re.match(r"^(#{1,5})\s+(.*?)\s*#*\s*$", line)
        if heading:
            level, markup = len(heading.group(1)), inline(heading.group(2), page)
            if level == 1 and top and not page.title:
                page.title = plain(markup)
                out.append("<h1>%s</h1>" % markup)
            else:
                level = max(level, 2)
                identifier = slug(markup, page.ids)
                if top:
                    page.headings.append((level, identifier, markup))
                out.append('<h%d id="%s"><a class="anchor" href="#%s">%s</a></h%d>' % (level, identifier, identifier, markup, level))
            index += 1
            continue
        if re.match(r"^\s*(---+|\*\*\*+)\s*$", line):
            out.append("<hr>")
            index += 1
            continue
        directive = DIRECTIVE.match(stripped)
        if directive:
            name = directive.group(1)
            args, options = directive_args(directive.group(2))
            index += 1
            if name in ("cards", "steps", "shots"):
                inner = []
                while index < len(lines) and lines[index].strip() != "{{/%s}}" % name:
                    inner.append(lines[index])
                    index += 1
                if index >= len(lines):
                    raise BuildError("%s: {{%s}} is not closed" % (page.source, name))
                index += 1
                out.append(wrapped(name, inner, page))
            elif name == "snippet":
                path, _, part = args[0].partition("#")
                lang = {"rs": "rust", "js": "js", "toml": "toml", "sh": "sh"}.get(path.rsplit(".", 1)[-1], "")
                out.append(code_block(region(path, part), lang, options.get("title")))
            elif name == "include":
                lang = {"rs": "rust", "js": "js", "toml": "toml", "sh": "sh"}.get(args[0].rsplit(".", 1)[-1], "")
                out.append(code_block(read(args[0]), lang, options.get("title")))
            elif name == "api":
                only = options.get("only", "").split(",") if options.get("only") else None
                hide = options.get("except", "").split(",") if options.get("except") else None
                out.append(api_table(args[0], args[1], only, hide))
            elif name == "variants":
                out.append(members_table(args[0], "enum", args[1]))
            elif name == "fields":
                out.append(members_table(args[0], "struct", args[1]))
            else:
                raise BuildError("%s: unknown directive {{%s}}" % (page.source, name))
            continue
        if stripped.startswith("|") and index + 1 < len(lines) and re.match(r"^\s*\|?\s*:?-{2,}", lines[index + 1]):
            head = split_row(line)
            index += 2
            rows = []
            while index < len(lines) and lines[index].strip().startswith("|"):
                rows.append(split_row(lines[index]))
                index += 1
            table = "<thead><tr>%s</tr></thead><tbody>%s</tbody>" % (
                "".join("<th>%s</th>" % inline(cell, page) for cell in head),
                "".join("<tr>%s</tr>" % "".join("<td>%s</td>" % inline(cell, page) for cell in row) for row in rows),
            )
            out.append('<div class="table"><table>%s</table></div>' % table)
            continue
        if stripped.startswith(">"):
            quote = []
            while index < len(lines) and lines[index].strip().startswith(">"):
                quote.append(re.sub(r"^\s*>\s?", "", lines[index]))
                index += 1
            out.append('<div class="note"><svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7.5v.5"/></svg><div>%s</div></div>' % blocks(quote, page))
            continue
        item = LIST_ITEM.match(line)
        if item:
            base = len(item.group(1))
            ordered = item.group(2)[0].isdigit()
            items = []
            while index < len(lines):
                match = LIST_ITEM.match(lines[index])
                if not match or len(match.group(1)) != base or match.group(2)[0].isdigit() != ordered:
                    break
                width = base + len(match.group(2)) + 1
                body = [match.group(3)]
                index += 1
                while index < len(lines):
                    following = lines[index]
                    if not following.strip():
                        ahead = index + 1
                        while ahead < len(lines) and not lines[ahead].strip():
                            ahead += 1
                        if ahead < len(lines) and len(lines[ahead]) - len(lines[ahead].lstrip()) > base:
                            body.append("")
                            index += 1
                            continue
                        break
                    indent = len(following) - len(following.lstrip())
                    if indent <= base and (LIST_ITEM.match(following) or paragraph_end(index)):
                        break
                    body.append(following[min(indent, width) :] if indent > base else following.strip())
                    index += 1
                items.append(body)
            tag = "ol" if ordered else "ul"
            rendered = []
            for body in items:
                inner = blocks(body, page)
                if inner.count("<p>") == 1 and inner.startswith("<p>") and inner.endswith("</p>"):
                    inner = inner[3:-4]
                elif inner.startswith("<p>") and "</p><ul>" in inner and inner.count("<p>") == 1:
                    inner = inner[3:].replace("</p>", "", 1)
                rendered.append("<li>%s</li>" % inner)
            out.append("<%s>%s</%s>" % (tag, "".join(rendered), tag))
            continue
        if re.match(r"^<(div|section|figure|details|p|table|svg|img|picture)\b", stripped):
            raw = []
            while index < len(lines) and lines[index].strip():
                raw.append(lines[index])
                index += 1
            out.append("\n".join(raw))
            continue
        paragraph = [stripped]
        index += 1
        while index < len(lines) and not paragraph_end(index):
            paragraph.append(lines[index].strip())
            index += 1
        out.append("<p>%s</p>" % inline(" ".join(paragraph), page))
    return "".join(out)


def wrapped(name, lines, page):
    if name == "steps":
        return '<div class="steps">%s</div>' % blocks(lines, page)
    if name == "cards":
        cards = []
        for line in lines:
            match = re.match(r"^\s*[-*]\s+\[([^\]]+)\]\(([^)]+)\):?\s*(.*)$", line)
            if not match:
                if line.strip():
                    raise BuildError("%s: a card is `- [Title](link): text`, not %r" % (page.source, line))
                continue
            href = resolve(match.group(2), page)
            if href is None:
                raise BuildError("%s: a card links outside the site: %s" % (page.source, match.group(2)))
            cards.append('<a class="card" href="%s"><span class="card-title">%s</span><span class="card-text">%s</span></a>' % (attr(href), inline(match.group(1), None), inline(match.group(3), page)))
        return '<div class="cards">%s</div>' % "".join(cards)
    shots = []
    for line in lines:
        match = re.match(r"^\s*!\[([^\]]*)\]\(([^)\s]+)\)\s*$", line)
        if not match:
            if line.strip():
                raise BuildError("%s: a shot is `![caption](path)`, not %r" % (page.source, line))
            continue
        src = resolve(match.group(2), page)
        if src is None:
            raise BuildError("%s shows an image that is not in site/: %s" % (page.source, match.group(2)))
        caption = match.group(1)
        shots.append('<figure class="phone"><div class="phone-frame"><img src="%s" alt="%s" loading="lazy"></div><figcaption>%s</figcaption></figure>' % (attr(src), attr("Screenshot: " + caption), esc(caption)))
    return '<div class="shots">%s</div>' % "".join(shots)


# ----------------------------------------------------------------- the pages


def out_path(source):
    inside = source[len("docs/") :]
    if inside.endswith("README.md"):
        inside = inside[: -len("README.md")] + "index.md"
    return inside[:-3] + ".html"


def load_site():
    config = json.loads(read("docs/site.json"))
    for section in config["sections"]:
        for group in section["groups"]:
            for entry in group["pages"]:
                source = entry if isinstance(entry, str) else entry["src"]
                page = Page(source, out_path(source))
                page.section, page.group = section, group
                if not isinstance(entry, str):
                    page.nav_title = entry.get("title", "")
                PAGES[source] = page
        for pattern in section.get("also", []):
            directory, _, expression = pattern.rpartition("/")
            for name in sorted(os.listdir(os.path.join(ROOT, directory))):
                source = directory + "/" + name
                if re.fullmatch(expression, name) and source not in PAGES:
                    page = Page(source, out_path(source))
                    page.section, page.group = section, None
                    page.parent = section["also_parent"]
                    PAGES[source] = page
    return config


def section_text(body):
    """(id, heading, text) per heading of a rendered body, for the search."""
    parts = re.split(r'<h[2-5] id="([^"]+)">(.*?)</h[2-5]>', body, flags=re.S)
    found = [("", "", plain(re.sub(r"<pre>.*?</pre>", " ", parts[0], flags=re.S)))]
    for at in range(1, len(parts), 3):
        text = plain(re.sub(r"<pre>.*?</pre>", " ", parts[at + 2], flags=re.S))
        found.append((parts[at], plain(parts[at + 1]), text))
    return [(a, b, re.sub(r"\s+", " ", c).strip()) for a, b, c in found]


def render(page):
    text = read(page.source)
    page.body = blocks(text.split("\n"), page, top=True)
    if not page.title:
        raise BuildError("%s has no `# Title`" % page.source)
    page.nav_title = page.nav_title or page.title
    page.body = page.body.replace("</h1><p>", '</h1><p class="lead">', 1)
    lead = re.search(r'<p class="lead">(.*?)</p>', page.body, re.S)
    page.lead = plain(lead.group(1)) if lead else ""
    page.sections = section_text(page.body)


SEARCH_ICON = '<svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/></svg>'
MENU_ICON = '<svg aria-hidden="true" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M4 7h16M4 12h16M4 17h16"/></svg>'


def document(page, config, version, order):
    def to(target):
        return rel(page.out, target)

    section = page.section
    first_of = {s["id"]: PAGES[s["groups"][0]["pages"][0] if isinstance(s["groups"][0]["pages"][0], str) else s["groups"][0]["pages"][0]["src"]] for s in config["sections"]}
    tabs = "".join(
        '<a class="tl%s" href="%s"%s>%s</a>' % (" on" if s is section else "", attr(to(first_of[s["id"]].out)), ' aria-current="true"' if s is section else "", esc(s["title"]))
        for s in config["sections"]
    )
    nav = []
    for group in section["groups"]:
        nav.append('<p class="nh">%s</p>' % esc(group["title"]))
        for entry in group["pages"]:
            other = PAGES[entry if isinstance(entry, str) else entry["src"]]
            current = other is page or getattr(page, "parent", None) == other.source
            nav.append('<a class="ni%s" href="%s"%s>%s</a>' % (" on" if current else "", attr(to(other.out)), ' aria-current="page"' if other is page else "", esc(other.nav_title)))
    crumbs = [section["title"]]
    if page.group:
        crumbs.append(page.group["title"])
    elif getattr(page, "parent", None):
        parent = PAGES[page.parent]
        crumbs.append('<a href="%s">%s</a>' % (attr(to(parent.out)), esc(parent.nav_title)))
    crumbs.append('<span class="here">%s</span>' % esc(page.nav_title))
    crumbs = '<span aria-hidden="true">/</span>'.join(c if c.startswith("<") else "<span>%s</span>" % esc(c) for c in crumbs)
    toc = "".join(
        '<a class="ti%s" href="#%s">%s</a>' % (" sub" if level > 2 else "", identifier, re.sub(r"</?(a|code)[^>]*>", "", markup))
        for level, identifier, markup in page.headings
        if level <= 3
    )
    at = order.index(page) if page in order else -1
    previous = order[at - 1] if at > 0 else None
    following = order[at + 1] if 0 <= at < len(order) - 1 else None
    pager = ""
    if previous or following:
        pager = '<nav class="pager" aria-label="Previous and next page">%s%s</nav>' % (
            '<a href="%s"><span>Previous</span>%s</a>' % (attr(to(previous.out)), esc(previous.nav_title)) if previous else "<span></span>",
            '<a class="next" href="%s"><span>Next</span>%s</a>' % (attr(to(following.out)), esc(following.nav_title)) if following else "",
        )
    title = NAME + " documentation" if page.out == "index.html" else "%s · %s" % (page.title, NAME)
    return """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="{description}">
<meta name="color-scheme" content="dark">
<link rel="icon" href="{assets}/favicon.svg" type="image/svg+xml">
<link rel="icon" href="{assets}/favicon-32.png" sizes="32x32" type="image/png">
<link rel="apple-touch-icon" href="{assets}/icon-180.png">
<link rel="stylesheet" href="{assets}/site.css">
</head>
<body>
<a class="skip" href="#content">Skip to the content</a>
<header class="top">
<button class="menu" type="button" aria-label="Pages of this section" aria-expanded="false" aria-controls="nav">{menu}</button>
<a class="brand" href="{home}"><img src="{assets}/logo-tile.svg" alt="" width="36" height="36"><span>{name}</span></a>
<span class="version">v{version}</span>
<div class="srch" role="search">
{search_icon}
<input type="search" id="search" placeholder="Search the docs" aria-label="Search the docs" autocomplete="off" spellcheck="false" aria-controls="results">
<kbd>Ctrl K</kbd>
<div class="results" id="results" role="listbox" aria-label="Search results" hidden></div>
</div>
<nav class="tls" aria-label="Sections">{tabs}</nav>
<a class="cta" href="{start}">Get started</a>
</header>
<div class="shell">
<nav class="nav" id="nav" aria-label="{section}">
<div class="nav-tabs" aria-label="Sections">{tabs}</div>
{nav}
</nav>
<main id="content">
<div class="crumbs">{crumbs}</div>
<article>
{body}
</article>
{pager}
<footer class="foot">Built from the repository's own Markdown by <code>tools/build-docs.py</code>. Source of this page: <code>{source}</code>.</footer>
</main>
<aside class="toc" aria-label="On this page">{toc}</aside>
</div>
<script>window.MS_ROOT = "{root}";</script>
<script src="{assets}/site.js" defer></script>
</body>
</html>
""".format(
        title=esc(title),
        description=attr(page.lead[:300]),
        assets=to("assets"),
        home=attr(to("index.html")),
        start=attr(to(PAGES["docs/getting-started.md"].out)),
        name=NAME,
        version=esc(version),
        menu=MENU_ICON,
        search_icon=SEARCH_ICON,
        tabs=tabs,
        nav="".join(nav),
        section=attr(section["title"]),
        crumbs=crumbs,
        body=page.body,
        pager=pager,
        source=esc(page.source),
        toc='<p class="toc-title">On this page</p>' + toc if toc else "",
        root=attr(to(".")),
    )


def check_links(outputs):
    """Every href and src of the built pages leads to a file that exists,
    and every fragment to an id of that page."""
    problems = []
    ids = {out: set(re.findall(r'\bid="([^"]+)"', text)) for out, text in outputs.items() if out.endswith(".html")}
    for out, text in outputs.items():
        if not out.endswith(".html"):
            continue
        for kind, target in re.findall(r'\b(href|src)="([^"]+)"', text):
            target = html.unescape(target)
            if re.match(r"[a-z][a-z0-9+.-]*:", target):
                continue
            path, _, fragment = target.partition("#")
            inside = os.path.normpath(os.path.join(os.path.dirname(out), path)).replace(os.sep, "/") if path else out
            if inside in outputs:
                if fragment and inside in ids and fragment not in ids[inside]:
                    problems.append("%s: %s has no heading `#%s`" % (out, inside, fragment))
            elif not os.path.exists(os.path.join(SITE, inside)):
                problems.append("%s: %s=%s leads to nothing" % (out, kind, target))
    return problems


def build():
    config = load_site()
    version = re.search(r'^version = "([^"]+)"', read("Cargo.toml"), re.M).group(1)
    for page in PAGES.values():
        global CURRENT
        CURRENT = page
        render(page)
    outputs = {}
    for section in config["sections"]:
        order = [PAGES[e if isinstance(e, str) else e["src"]] for g in section["groups"] for e in g["pages"]]
        for page in PAGES.values():
            if page.section is section:
                outputs[page.out] = document(page, config, version, order)
    index = []
    for page in PAGES.values():
        for identifier, heading, text in page.sections:
            if not text and not heading:
                continue
            index.append({"t": page.title, "h": heading, "u": page.out + ("#" + identifier if identifier else ""), "s": page.section["title"], "x": text[:420]})
    outputs["assets/search-index.js"] = "window.MS_SEARCH = %s;\n" % json.dumps(index, ensure_ascii=False, separators=(",", ":"))
    problems = check_links(outputs)
    if problems:
        raise BuildError("broken links:\n  " + "\n  ".join(problems))
    return outputs


def generated_on_disk():
    found = set()
    for directory, _, names in os.walk(SITE):
        for name in names:
            if name.endswith(".html"):
                found.add(os.path.relpath(os.path.join(directory, name), SITE).replace(os.sep, "/"))
    return found


def main():
    check = "--check" in sys.argv[1:]
    try:
        outputs = build()
    except BuildError as error:
        print("build-docs: %s" % error, file=sys.stderr)
        return 1
    stale = generated_on_disk() - set(outputs)
    if check:
        wrong = sorted(stale)
        for out, text in outputs.items():
            path = os.path.join(SITE, out)
            if not os.path.exists(path) or open(path, encoding="utf-8").read() != text:
                wrong.append(out)
        if wrong:
            print("build-docs: site/ is not what the sources give. Run `python3 tools/build-docs.py`. Differing:", file=sys.stderr)
            for out in wrong[:20]:
                print("  site/" + out, file=sys.stderr)
            return 1
        print("build-docs: site/ is up to date (%d pages, links checked)." % (len(outputs) - 1))
        return 0
    written = 0
    for out, text in outputs.items():
        path = os.path.join(SITE, out)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if not os.path.exists(path) or open(path, encoding="utf-8").read() != text:
            with open(path, "w", encoding="utf-8") as file:
                file.write(text)
            written += 1
    for out in stale:
        os.remove(os.path.join(SITE, out))
    print("build-docs: %d pages, %d files written, %d stale removed, links checked." % (len(outputs) - 1, written, len(stale)))
    return 0


if __name__ == "__main__":
    sys.exit(main())

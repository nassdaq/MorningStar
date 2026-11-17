# 0044 — The documentation site: static files built from the repository's Markdown by one script

Status: Accepted (2026-11-17). Built and looked at on the development machine in headless Chrome at desktop and phone widths. Not published anywhere.

## Context

The documentation was three Markdown guides, the decision records and `AGENTS.md`. There was nothing to hand to a person who wants to learn the framework from the beginning, no reference of the widgets, and no pictures of what apps look like. The owner supplied a design for a documentation site (four pages: an overview, an installation page, a widget page and a reference page; dark, with a gold accent) and a logo.

The design's own text describes a framework that does not exist: a `#[view]` macro and signals, an `mstar` command, iOS support, hot reload on devices, a version 0.9.2. It was treated as a picture of a layout. Every statement of the site had to come from this repository.

Three things can go wrong with documentation, and each has happened to others: the prose says something the code no longer does; a code sample does not compile; a picture shows a screen that has since changed.

## Decision

**A static site under `site/`, generated from Markdown under `docs/` by `tools/build-docs.py`, which needs only Python 3's standard library.** The generated HTML is committed, so the site opens from a file manager and can be served by any static host with no build step.

- **The prose lives once.** `docs/site.json` lists the pages. The three existing guides and all decision records are pages of the site as they are; nothing was copied into a second format. A relative link between Markdown files becomes a link between pages. A link to a file the site does not hold (a source file, an example's `SCREENS.md`) is shown as its path, because the repository is private and there is nothing to link to.
- **Code samples are compiled.** New samples are regions of `examples/docs-snippets`, a workspace member, marked `// docs:begin name` and `// docs:end name`, and pulled into a page by `{{snippet path#name}}`. `cargo clippy --workspace`, which CI runs, compiles them; those with behaviour have tests. The samples inside the three older guides were not moved and are not compiled.
- **Method tables are read from the source.** `{{api path Target}}` lists the public methods of an `impl` block or a macro with the first sentence of each doc comment; `{{variants ..}}` and `{{fields ..}}` do the same for enums and structs. A method that is renamed changes the table at the next build, and one that is named in a page and no longer exists fails it.
- **Tool help is the tool's output**, captured into `docs/reference/help/` by `tools/docs-screenshots.sh`.
- **Screenshots are frames from the real renderer.** `tools/docs-screenshots.sh` runs each example's own screenshot program, and generates the starter app into a throwaway workspace under `target/`, then scales each picture to 540 px wide as a JPEG in `site/shots/`. Nothing is captured from a screen or a phone.
- **Links are checked at every build.** A link to a page, a file or a heading that does not exist fails the build, and `--check` also fails when `site/` is not what the sources give.
- **The look follows the supplied design**: its colours, its three typefaces (Inter, Space Grotesk and JetBrains Mono, the Latin ranges only, under the SIL Open Font License, whose texts are kept beside them), its three-column shell, code blocks, tables, cards, callouts and numbered steps. Dark only, as designed.

**Why not a documentation generator** (mdBook, Docusaurus, Zola): each would be a dependency to install and keep working for every contributor and for CI, would bring its own theme to fight to reach the supplied design, and none reads method tables out of Rust doc comments or checks that a sample is a region of a compiled crate. The script is about 700 lines, handles the Markdown these documents actually use, and fails loudly on what it does not understand.

**Why not rustdoc alone**: rustdoc documents items, and is still the place for that. It does not hold a guide, pictures of screens, the tools, the examples or the platform's status.

## Where the site departs from the design

- **Sections**: Guide, Widgets, Reference and Examples. The design's "Blog" has no content to hold and is not there.
- **No "Is this page useful?" buttons**: there is nowhere to send an answer.
- **The version pill** shows the workspace's real version from `Cargo.toml`.
- **A menu button on narrow screens.** The design hides the navigation below 780 px and offers no way to reach it.
- **Search works**, over an index built with the pages and loaded on first use.
- **The widget page's "live preview" boxes** are replaced by compiled samples; a few pages show real screenshots in phone frames, which the design does not draw.
- **The logo** was traced as a vector from the supplied picture, so that it is sharp at 36 px and at 512.

## Consequences

- A change to a guide, a sample, a doc comment or a tool's help must be followed by `python3 tools/build-docs.py`, or `--check` fails. A change to a screen should be followed by `tools/docs-screenshots.sh`; nothing detects a stale picture.
- `site/` is generated output in the repository: about 100 HTML files and under 4 MB in all. Reviews of a documentation change show the Markdown diff and the HTML diff.
- The Markdown converter is a subset. A construct it does not handle (reference-style links, nested block quotes, HTML mixed into paragraphs) renders as text or fails the build; extend the script when a page needs one.
- The site is in English only.

## Verification

- `python3 tools/build-docs.py --check`: passes; every internal link and heading anchor resolves.
- `cargo clippy -p docs-snippets --all-targets -- -D warnings`: clean. `cargo test -p docs-snippets`: 6 pass.
- Every page was rendered in headless Chrome at 1440 and 390 px wide and a sample of them looked at; the home page was compared with the design's first page.

Not verified: the site in any browser but Chrome; with a screen reader; served by a static host (it was opened from `file://` only); that `--check` passes in CI (it is not yet a CI step).

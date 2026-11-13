# Atlas: the screens

An AI-assistant app reproduced from a design of ten pages
(`design/README.md`) and made to work end to end, in two ways:

- **With its server** (`server/`: a small Node program that holds a
  DeepSeek key, optionally an ElevenLabs key, and a PostgreSQL database)
  the chat is answered by a real model, chats are kept and listed in the
  Library, the model and plan are saved, and, if the server has a voice,
  live voice records what you say and the answer is spoken. See "With a
  server" below.
- **Without it** (the server does not answer, or nothing behind the app
  can make a request, as in a test or a screenshot) it is the scripted
  app: answers come from `src/assistant.rs`, live voice plays the design's
  own line, and nothing leaves the phone or is stored.

In both there is **no account system and no file picker**: sign-in accepts
anything, the attachment is the design's sample (its *name* is all the
server is told, and the model is instructed to say that it cannot read
it), and a plan is "chosen", not bought. The app holds no secret.

```sh
cargo test -p atlas -j2                                               # fidelity, journeys, accessibility, the app against a scripted server
cargo run -q -p atlas --example screenshot -- out/ --phone --online --sheet   # the states with a server, on a Galaxy A27
(cd examples/atlas/server && npm install && npm start)                # the server, on 127.0.0.1:8788 (its README says how)
cargo test -p atlas --test online -- --ignored --nocapture a_real_typed_chat  # the app against that server, through curl
cargo run -q -p atlas --example screenshot -- out/ 03 --diff          # one page against the design
cargo run -q -p atlas --example screenshot -- out/ --phone --states --sheet   # everything on a Galaxy A27
cargo run -q -p atlas --example icon                                  # redraw assets/icon.png from the orb
```

## The design's pages

Rendered at 780 x 1688 px and compared with `design/reference` by
`tests/fidelity.rs` with `morningstar-diff`'s defaults (colour error over
2, edges within 1 px). The limit is the measured figure rounded up to the
next 0.05.

| Page | Module | Measured | Limit | What it is |
| --- | --- | --- | --- | --- |
| `01-onboarding` | `screens/onboarding.rs` | 0.0482% | 0.05% | The 176 dp orb under a wash, the promise, two buttons |
| `02-sign-in` | `screens/sign_in.rs` | 0.1309% | 0.15% | Google button, email and password fields, sign in |
| `03-home` | `screens/home.rs` | 0.0505% | 0.10% | The 96 dp orb, three suggestions, the model chip, the composer, the tab bar |
| `04-chat` | `screens/chat.rs` | 0.0092% | 0.05% | A conversation three steps into its answer |
| `05-model-picker` | `screens/model_picker.rs` | 0.0172% | 0.05% | A bottom sheet over a dimmed screen |
| `06-live-voice` | `screens/live_voice.rs` | 0.0506% | 0.10% | The 200 dp orb, what was heard, three round buttons |
| `07-explore` | `screens/explore.rs` | 0.0731% | 0.10% | Search, filter chips, six tools |
| `08-plans` | `screens/plans.rs` | 0.1600% | 0.20% | The recommended plan and three others |
| `09-home-simplified` | `screens/home.rs` | 0.0505% | 0.10% | Page 03 again |
| `10-chat-simplified` | `screens/chat.rs` | 0.0092% | 0.05% | Page 04 again |

At the design's size nothing scrolls and nothing moves (tested): a page
opened with `Atlas::designed(name)` is a still frame until it is touched.

### Pages 09 and 10

They are pages 03 and 04. Their markup differs in **three `href`s and
nothing else**; their computed layouts are identical and their reference
pictures are the same bytes (tested). In 09 the send button leads to 10
where 03's leads to 04, and the Home tab leads to 09 itself; in 10 the back
arrow leads to 09. Every other link of the pair (Explore, the profile, the
model picker, voice) is the one 03 and 04 have, and those pages link back
to 03 and 04.

So they are one view with two identities, not two views: `Screen::Home(Flow)`
and `Screen::Chat(Flow)`, where `Flow` is `Standard` or `Simplified`, drawn
by the same two functions. The flow decides only which chat a send opens
and which home a chat's arrow returns to; leaving the pair by another link
and tapping Home lands on 03, as in the design (`the_simplified_pair_links_
to_itself` in `tests/e2e.rs`). The app starts in the standard flow; the
simplified one is reached only by opening the app as page 09 or 10, because
nothing in pages 01 to 08 links to it.

### What the remainder is made of

Every page's layout matches Chrome's to the pixel row: what differs is how
glyphs and curves are rasterised.

- **Accent-coloured and white-on-gradient text** is most of it: "Forgot
  password?" and "Create an account" (02), "Recommended" and "Get Plus"
  (08), "Listening" (06), the labels of the gradient buttons. The glyph
  weighting of ADR 0023 was fitted to black and to white text on the two
  earlier designs; text in the mid-luminance accent (`#A0287A`) and white
  text over the mid-tone gradient come out a little lighter or heavier
  than Chrome's at their edges (mean error 6 to 10 over 200 to 700 pixels
  per label). The curve was left alone: refitting it moves the other
  examples' figures, which this work was required not to do.
- **Icons**: the 1.6 px strokes of the accent-coloured icons on white (03,
  07) differ along their edges by a mean error of 4 to 5; the white
  microphone on the gradient disc (06) likewise.
- **Ink text**: a few pixels per word, as in every example.
- **The orb**: under 60 pixels on the screen it fills (01): its rim, where
  the one-pixel inner ring meets the anti-aliased edge, and the lower left
  of the small orb beside the name. The blurred glow and the inner lights
  are within the threshold everywhere.

## What every control does

Navigation follows the design's links; where the design has no page for a
link, the app does something a person would expect and says so here.

| Where | Control | What it does |
| --- | --- | --- |
| Onboarding | "Get started", "I already have an account" | Both open sign-in (both are `href="SignIn"` in the design) |
| Sign in | "Continue with Google" | Home, as the design's link: there is no account system |
| | Email, Password | Real fields: the email keyboard with Next, a hidden password with Done, which signs in |
| | "Sign in" | Home if the address looks like one and the password has six characters; otherwise a line under the fields says which is wrong (app's own, in the design's link style). Home replaces the stack: Back does not return to sign-in |
| | "Forgot password?", "Create an account" | A toast saying what this demo does instead (`href="#"` in the design) |
| Home, Explore, Library | Profile button | The plans page, as the design's link (there is no separate profile screen) |
| Home, chat, voice | The model's name | The model picker rises over that screen with the scrim fading in |
| Picker | A model | Chooses it and closes the sheet; the name changes on every screen, and scripted answers end with it. Online it is sent with every message and saved to the profile |
| | The scrim, system Back | Closes the sheet |
| | "All models and providers" | The models screen (app's own) |
| Home | A suggestion | Starts a chat with that request |
| Home, chat | "+" | Attaches the design's sample ("3 PDFs"); a chip above the composer shows it and removes it when tapped (app's own). There is no file picker |
| | The field | Typing swaps the microphone for the send button, as the design's script does |
| | Send, the keyboard's Send | Adds the message (with its attachment) and asks the assistant; from home, opens a new chat. Online the message is posted to the server; while one message of a chat is being answered the next stays in the field |
| Home | "Live" / "Offline" beside the model chip | Only when there is a server to be live with or offline from: opens the Server screen (app's own) |
| | Microphone | Live voice |
| Chat | Back | Home |
| | The step card | Unfolds into the list of steps, as in the design's markup; "Cancel task" stops the answer. Online it is there from "Thinking" on and abandons the request |
| | "Retry" (online) | Under a message the server did not answer, beside one line that says why: sends the same message again |
| | "Stop" (online) | Beside an answer being spoken: stops it. Leaving the chat stops it too |
| | "Delete this chat" (online) | Asks once more ("Tap again to delete it for good"), then deletes the chat on the server and returns to where it was opened from |
| | An attachment chip | A toast: attachments are a sample |
| Live voice | (by itself) | "Hears" the design's line a word every quarter second while the orb breathes, holds it a moment, then sends it to the chat it was opened from (or a new one) |
| | Microphone | Pauses and resumes; the label says "Paused" |
| | Cross | Ends at once and sends what was heard so far |
| Live voice, server with a voice | (by itself) | Asks for the microphone and records, the orb breathing; stops by itself after 55 s |
| | Microphone, cross | Stop recording and send it; the transcript shows in the design's quotes for a moment, then the chat opens with it and the answer is spoken. After a failure the microphone records again and "Send again" resends the recording |
| | Chevron, system Back | Leaves and sends nothing |
| | Camera | A toast: no camera in this demo |
| Explore | Search | Filters the tools by their two lines as it is typed |
| | Chips | Filter by kind; the row scrolls sideways. Which tool is of which kind is the app's own sorting |
| | A tool | Starts a chat about it |
| Plans | "Get Plus", a row of "Other plans" | Records that plan as chosen: the button or row says so and a toast says nothing was charged. Online it is saved to the profile (still a demo: nothing is paid) |
| Library (online) | A row | Opens that chat and fetches its messages from the server |
| Models | "Server" | The Server screen |
| Server | The address, "Check connection", "Use the default address" | Where the server is (remembered on the phone), and asking it how it is |
| Tab bar | Home, Explore, Library | The tab's root; Library is the app's own |
| Anywhere | System Back | Closes the keyboard, then the sheet, then pops; on a tab's root it is left to the system |

**The chat is a real conversation, locally.** A message becomes a bubble
at once with a "Thinking" card; the answer "arrives" 0.6 s later through
`Message::Answered`, shows its first line, and goes through its steps on
the timer ("Reading documents · Step 1 of 4" and on, 0.8 s each, the
spinner turning) until the reply appears; the list follows the newest
message down. The timer runs only while something moves and the app is
idle otherwise (tested).

That paragraph is the app without a server. With one, the same card and
the same timer play out what the server sent.

## With a server

`server/API.md` is the contract, `src/api.rs` the client (requests built,
answers decoded and cleaned; nothing from a server is trusted), `src/lib.rs`
the state. `src/json.rs` is a copy of CallReady's small JSON reader.

**Start-up.** `GET /v1/health`, then `/v1/profile`, then `/v1/chats`. If all
three answer, the app is *online* (`Link::Online`): the model chip and the
plan are the profile's, the Library lists the server's chats. If the server
does not answer, or has no model key or database, the app is *offline*
(`Link::Offline`, with the reason on the Server screen) and behaves exactly
as the scripted app; it asks again each time Home or the Library is
reached. With nothing behind the app that can make a request (`Ui::new`: the
fidelity tests, the screenshots) the first question fails at once and the
app is *standalone*: no mark, no "demo" label, the design's pixels.

**A message.** Sending posts it (`POST /v1/chats` for a new chat,
`…/messages` after) with the model chosen and the name of the attachment.
The card says "Thinking" with the spinner for as long as the server takes;
when the answer arrives the chat takes the server's title, the card runs
through the server's two to four steps on the same timer (0.8 s each), and
then the answer shows. If the choice was Auto, a small line under the
answer says which model took it ("Answered by Smart").

**The answer's formatting.** The server keeps the model to plain text with
paragraphs, `- ` and `1. ` items, `` `inline code` `` and fenced code
blocks, and the app renders exactly those (`src/format.rs`): paragraphs
and list items in the design's 16 px text, code on a tinted card. There is
no monospace face among the app's fonts, so code is set in Plus Jakarta
Sans (inline code a weight heavier); its line breaks are kept. An answer
with none of the marks is laid out exactly as before.

**When it fails.** The bubble keeps what was typed, the card says "No
answer", and one line under it says why with a "Retry" pill that sends
the same message again. A failed message stores nothing on the server. If
the server was not reached at all, the app is offline until it answers
again. An answer that arrives after "Cancel task", after a retry, or for a
chat that is gone, changes nothing (every request is numbered). A request
cannot be recalled, though: a cancelled message the server had already
answered is in the chat when it is next fetched.

**Voice.** When `health.voice` is true the microphone screen records
(`out.start_recording`), posts the recording with `X-Speak: 1`, shows the
server's transcript, opens the chat with that message and its answer, and
plays the spoken answer (`out.play`) with "Stop" beside it. A refused
microphone is explained in a toast and the screen becomes the demo; a
recording that is too short or does not get through is explained on the
screen. When the server has no voice, or the app is offline, live voice is
the simulation and its status says "demo" (the simulated line is then
sent to the server as a typed message when the app is online).

**The model behind each choice** is the server's business
(`server/src/model.js`): Fast, Smart and Code are DeepSeek models and
settings, Auto picks one per message.

`tests/online.rs` drives all of it with fingers and a scripted server at
the Galaxy A27's frame and at 360 x 640 dp: the journeys, every failure,
late answers, the offline fallback and the address change. Its one ignored
test carries the app's requests to a real server with `curl`.

## The app's own design

Not from the design, built from its parts and tokens:

- **Library** (`screens/library.rs`): the server's chats when it answers,
  otherwise the conversations of this run, under
  Explore's heading and in its list card, with a "New chat" button. The
  design's tab bar has the tab and no page for it.
- **Models** (`screens/models.rs`): where "All models and providers"
  leads. The picker's four models again, three invented providers that
  switch nothing on, the way to the Server screen, and a note that says
  where answers come from right now.
- **Server** (`screens/server.rs`): the server's address in the sign-in
  page's field, how the app stands with it, the design's two buttons, and
  how to run one. Reached from Models and from the mark on home.
- **Everything a server adds**: the "Live"/"Offline" mark beside home's
  model chip; in the chat, "Thinking" with "Cancel task", the failure line
  with "Retry", the formatted answer, "Answered by …", "Speaking the
  answer" with "Stop", "Loading this chat…" and "Delete this chat"; on
  live voice, the "Starting", "Sending", "Heard", "Not sent" and "demo"
  statuses, the small line under them and "Send again"; the Library's
  server rows and its "No chats yet". All in the design's type, pills and
  cards; none of it is drawn when the app is standalone.
- **States the design does not picture**: the sign-in message; the
  attachment waiting above the composer; the step card while thinking, when
  done and when cancelled; "Paused" on live voice; Explore with nothing
  found; a chosen plan; the toast. The selected Library tab uses the
  design's Library icon at the selected tabs' stroke width.
- **Motion**: the design declares none. The spinner's turning, the steps,
  the orb's breathing, the sheet's rise and the screens' slides are the
  app's, and all stop with the system's "reduce motion" where the
  framework animates them.
- **On a phone shorter than 844 dp** the fixed bars stay and the middle
  scrolls; at the design's size the layout is the design's exactly. Page
  05's backdrop is the design's own abbreviated home (the orb 4 dp lower
  than on home, no top bar, onboarding's wash) and is drawn only when the
  app is opened as page 05; in use the sheet is over the real screen.

## Hand-tuning

One thing is placed by hand, because the framework cannot do what the CSS
says:

| Where | By hand | The CSS it stands in for |
| --- | --- | --- |
| Plans, "per month" | `.offset(0.0, -1.0)` on a row aligned to its bottom | `align-items: baseline` between a 38 px and a 14 px text. The layout engine has no baseline alignment |

Three things are drawn another way than the CSS draws them, with the same
picture:

- The spinner (`border-top-color` on a round box) is two arcs
  (`assets/svg/spinner-*.svg`): a box has one border colour.
- The glow and the highlight of the orb (`border-radius: 50%` on boxes that
  are not square) are pills: their gradients are transparent at the
  ellipse's edge, so the shape never shows (ADR 0034).
- Hairlines (`border-top`, `border-bottom`) are dividers, as in every
  example.

No size is pinned and no text is nudged.

## Accessibility

`tests/accessibility.rs` runs the framework's check on every screen and on
the states a journey adds, at the design's size and on the Galaxy A27, and
makes the main journey (sign in, pick a model, ask, hear the answer arrive,
speak, choose a plan) by the semantics tree and its actions alone. Icon
buttons have names (the design's own `aria-label`s), tabs and the picker's
rows say what is selected, the microphone of live voice is a switch, the
orb says nothing, and answers and toasts are announced.

The check finds nothing but touch targets under 48 dp that are the
design's own sizes. They are allowed in the test, each with its reason, and
left as designed, because enlarging them would move the pixels the fidelity
tests hold:

| Target | The design's size |
| --- | --- |
| Back and minimise arrows | 44 x 44 |
| Profile button | 46 x 46 (44 and a border) |
| The composer's "+", microphone and send | 44 x 44 |
| The model chip | 44 high |
| Explore's chips | 44 high |
| "Forgot password?" | 44 high |
| "I already have an account" | 46 high |
| "Cancel task" | 44 high |
| Attachment chips | 34 high (32 and a border) |
| "New to Atlas? Create an account" | one 14 px line; the whole line is the target, as a tap target inside a sentence does not exist |

No contrast finding: every text of the design passes.

The controls a server adds are the app's own, so they are made 48 dp tall
rather than allowed: "Retry", "Stop", "Send again", "Delete this chat" and
the "Live" mark. A second test puts the app online against scripted
answers and checks those states and names the same way, with no allowance
added.

## Not verified

Everything here was seen on macOS/Metal only. **Nothing has run on a
phone**: no APK was built. That includes all of the online behaviour: the
app has talked to its real server only through the test that carries its
requests with `curl`; a phone's own HTTP stack, `adb reverse`, the
microphone permission and recording, the M4A a phone records being
transcribed, and the spoken answer playing are untried on a device. Unseen on a device are every screen, the
keyboard over the fields, the sheet, the timer-driven motion, the new
shader paths (radial gradients, the blurred glow, inner shadows) on Vulkan
or GLES and their frame times, the sideways drag of the chips, TalkBack,
and the launcher icon (looked at only in `morningstar-icon --preview`).

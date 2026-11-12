# The CallReady screens

Thirteen screens. Five are reproduced from the design and compared with its
pictures by `tests/fidelity.rs`; seven are **the app's own design, not from
the design file**, added so that every control of the design leads
somewhere. `tests/e2e.rs` drives all of them with a finger, with no server;
`tests/online.rs` drives them with a stand-in for the server. Read
`docs/building-screens.md` first, in particular "Turned things,
illustrations and text effects"; `examples/fundidesk/SCREENS.md` describes
the loop and the bar, which are the same here.

```sh
cargo run -q -p callready --example screenshot -- "$OUT" 02 --diff      # render, compare, write the side-by-side
cargo run -q -p callready --example screenshot -- "$OUT" --phone --states --sheet   # every screen on a Galaxy A27, on one sheet
cargo run -q -p callready --example screenshot -- "$OUT" --phone --online --sheet   # the states with a server behind it
cargo test -p callready
```

## Online and offline

The app is a client of the CallReady server (`server/`; the contract is
`server/API.md`). It holds no secrets and identifies itself with the
installation's id (`X-Device-Id`).

- **Start-up**: `GET /v1/health`, then `/v1/profile` and `/v1/calls`. When
  all three answer the app is **online**: name, settings, plan, numbers and
  history are the server's; edits are `PUT` to it (shown at once, tried a
  second time quietly, put back with a line if that fails too).
- **Online, a call is a conversation with an AI caller**: `POST /v1/calls`,
  a turn per reply, `POST …/end` for the report. It is **typed** when the
  server has no voice key (`health.voice` false) and **spoken** when it has
  one; the app needs no change between the two.
- **Offline** (any of the three failed) the app is what it was before it
  had a server: the scripted calls of `src/script.rs` over the sample
  history, scored on the phone. Home and the live call carry a small
  "offline · practice script"; nothing blocks. The server is asked again
  each time the learner arrives on Home, and from the Server screen. A call
  that cannot be started online is the scripted call, with a line saying so.
- **Where no request can be made at all** (a screenshot, `tests/e2e.rs`,
  `tests/fidelity.rs`: no host behind the app) the app is the scripted app
  and shows no note: the designed screens are pixel for pixel what they
  were.
- **Premium** is a demo online too: it sets `plan` on the server; no
  payment exists.

With a phone in development: run the server on the computer
(`cd examples/callready/server && npm start`, on `127.0.0.1:8787`), connect
the phone over USB, run `adb reverse tcp:8787 tcp:8787`. The app's default
address `http://127.0.0.1:8787` then reaches it; More, Server changes and
checks the address. Put `ELEVENLABS_API_KEY` in the server's `.env` and
restart it, and calls are spoken.

Code: `src/json.rs` (a small JSON reader; no dependency was added for it),
`src/api.rs` (requests built and answers decoded, pure), `src/online.rs`
(a call played by the server, as a state machine that returns commands),
`src/lib.rs` (`update` queues what to ask, `effects` asks, each answer
returns as a `Net` message carrying the numbers it was asked with; an
answer whose numbers are no longer current changes nothing).

## Structure

```text
Welcome (first launch only)
  └─ Home ─────────── tab ─┬─ Progress ─ tab
      │                    ├─ History ── tab
      │                    └─ More ───── tab ─ Server
      ├─ Profile ─ Scenario settings, Premium
      ├─ Notifications ─ Call report, Progress, Premium
      ├─ Scenario settings
      └─ Live call ─ Call report ─ Transcript
```

- **Tabs**: Home, Progress, History and More carry the tab bar, with their
  own tab in the accent colour. Home is the root; the other three sit one
  step above it, so Back from any tab is Home.
- **Everything else is pushed** on a history stack (a `Navigator<Screen>`;
  `CallReady::stack` is what is under the screen showing).
  Its back arrow, its cross and the system's Back all return to where the
  learner came from (`CallReady::go_back`, which is `Application::back`).
  Back on Home, or on Welcome, is not handled: the host leaves the app.
- **Welcome** is not behind Home: after "Let's go" it is gone for the
  session.
- **A call** takes its report's place when "Practise again" is pressed and
  gives its place to its report when it ends, so Back never returns into a
  call that is over. Back during a call abandons it; nothing is scored.
- **State** is one `Model` (`src/model.rs`): the name, the settings, the
  plan, the history, the bell, the call in progress and the report being
  looked at. Screens are views of it and only say what was tapped
  (`src/screens/*.rs`); `lib.rs` changes it.
- **Every screen scrolls between its fixed bars** when it does not fit
  (home's tab bar, the live call's header and call bar, the report's and
  the offer's buttons, welcome's button). At the design's 390 x 844 dp
  nothing scrolls.

## The screens

| Screen | File | From | Pixels differing | What it is |
| --- | --- | --- | --- | --- |
| 01 welcome | `screens/welcome.rs` | design | 0.0095% | turned cut-out letters, handwriting, doodles, synthetic italic, a ringed word |
| 02 home | `screens/home.rs` | design | 0.0135% | the disc (the design's SVG, loaded), edge-cut rings, tilted double frames, striped block, shadowed and outlined numerals, tab bar |
| 03 live call | `screens/live_call.rs` | design | 0.0060% | progress bars, rich text, chips, a four-colour avatar, a turned icon; a call that runs |
| 04 call report | `screens/call_report.rs` | design | 0.0374% | score plates, five bars with tabular numbers, tilted cards; scores derived from the call |
| 05 premium | `screens/premium.rs` | design | 0.0076% | cut-out letters, a list, plan cards whose selection moves |
| 06 profile | `screens/profile.rs` | own | | avatar, the name in a `TextInput`, three numbers on tilted cards, the set-up, the plan |
| 07 notifications | `screens/notifications.rs` | own | | three notes on tilted cards, each leading somewhere |
| 08 history | `screens/history.rs` | own | | past calls: score, scenario, day, length |
| 09 settings | `screens/settings.rs` | own | | accent, difficulty and call length as chips |
| 10 transcript | `screens/transcript.rs` | own | | a finished call line by line, the chosen words coloured |
| 11 progress | `screens/progress.rs` | own | | the average in the report's plates, the skills averaged, the latest call |
| 12 more | `screens/more.rs` | own | | the menu, and what kind of app this is (live calls, or the script) |
| 13 server | `screens/server.rs` | own | | the server's address in a `TextInput`, "Check connection" and its result |

The live call, the report and the transcript each have a second form for a
call the server played (the app's own design, in the same language): see
"A call the server plays" below.

The percentages were measured after text placement and compositing became
the browser's (ADR 0022, ADR 0023) and the hand corrections those made
unnecessary were deleted; with the corrections, before, they were 0.0685,
0.2064, 0.0140, 0.0965 and 0.0775%.

## What every control does

| Where | Control | Does |
| --- | --- | --- |
| Welcome | Let's go | Home, for good |
| Home | avatar and name | Profile |
| | bell | Notifications; clears the bell's dot |
| | call button | starts today's call, Missing delivery |
| | gear | Scenario settings |
| | scenario cards | start Angry customer, Refund request, Billing problem |
| | tab bar | Home, Progress, History, More |
| Live call | back | abandons the call |
| | loudspeaker | the customer repeats the line (shown for two seconds) |
| | tip chips | answer with that tip: the sentence is completed and becomes the line said last, the customer's next line comes; the last answer ends the call |
| | microphone | mutes; a muted learner cannot answer |
| | red button | ends the call: its report |
| Call report | back | where the learner came from |
| | Transcript | the transcript |
| | Practise again | the same scenario, from the start |
| Premium | cross | back, nothing recorded |
| | plan cards | select a plan |
| | button | records the selected plan (a demo: no payment) and goes back |
| Profile | name | typed with the keyboard; shows on home |
| | set-up | Scenario settings |
| | plan | Premium |
| Notifications | a note | the latest report, Progress, or Premium |
| History, Progress | a call | its report |
| Settings | chips | set the accent, difficulty and length of the next call |
| More | rows | Profile, Notifications, Scenario settings, Premium, Server |
| Server | address | typed with the keyboard; Go checks it |
| | Check connection | uses the address, stores it, asks the server: "connected · text mode", "connected · voice" or "unreachable" and why |
| | Use the default | `http://127.0.0.1:8787`, checked at once |
| Live call, online | reply field, send | a typed reply (up to 600 characters), sent by the button or the keyboard's Send |
| | big microphone | tap to record, tap again to send |
| | Aa / small microphone | type instead / speak again |
| | Retry | sends again the turn, or the request for the report, that failed |
| | loudspeaker | plays the caller's line again (lights it up when there is no sound) |
| | sound button | sound off: playback stops and is not started; lines still show |
| | red button | ends the call: the server's report |
| Call report, online | Try again | fetches again the details of a report opened from the history |

A call (`model::Call`): the clock counts seconds, delivered by the
runtime's timer (`Application::tick`) only while a call runs; the bar in
the header is the share of the script answered and the thin bar the share
of the target length used. The report's scores (`model::Scores::of`)
derive from the answers, the time and the difficulty, and are always the
same for the same call.

### A call the server plays

- It rings ("Calling Dana…") until `POST /v1/calls` answers; then the
  server's title, caller, goal, number, opening line and two tips show.
- **The tips are advice, not buttons**: dashed chips that cannot be tapped
  (chosen over inserting a starter phrase: the server's tips name an
  approach, "Take ownership", not words to say).
- **Typed**: a field and a send button fixed above the call bar. The words
  stay in the field until the caller has answered. The field grows to
  three lines and then scrolls inside itself, the caret in view.
- **Spoken**: the microphone is tapped to start and tapped again to send;
  it shows the seconds, stops itself at 60, and waits while the caller
  speaks. The caller's audio is fetched and played; the line shows
  meanwhile. A refused microphone turns the call into a typed one.
- What was said last sits dimmed above the caller's new line, cut to a few
  lines; each new line is a new scroll view starting at its top, so the
  newest line is in view under the header. After the first turn the goal
  moves below the conversation.
- The header's bar is the time used of the call's minutes.
- `done`, or the red button, ends the call; the report shows when it has
  arrived (and, in a spoken call, the closing line has been said). A call
  ended before the learner said anything shows "Not scored".
- A failure never loses what was typed or recorded: one line says what
  happened, with Retry where sending again can help. `409` goes to the
  report; a call the server no longer knows (`404`) is left with a line.
- Back abandons the call: nothing is scored, and its late answers are
  ignored. (The server keeps such a call open; it has no way to drop one.)
- The scripted call is unchanged, including that its tips can land below
  the fold on a short phone: only an online call restarts at its top.

Taps that rightly do nothing: the tab that is showing, the chip or plan
that is already selected, and a tip while the learner is muted.

Opened directly (`CallReady::on(Screen::LiveCall)`, `Message::Open`), the
live call and the report are **the design's moment**: the call stands at
02:14 with the design's lines until it is answered, and the report is the
design's 82. Those two states are drawn with pieces placed by measurement,
exact at the design's width; every other state re-flows.

## Tests

`tests/fidelity.rs`: the five designed screens against their references
(each limit the measured value rounded up to 0.05), no overflow at four densities, nothing scrolls at the
design's size, and the designed moments' buttons.

Unit tests: `src/json.rs` (the reader), `src/api.rs` (every example of
API.md decoded, every request built, malformed and hostile answers),
`src/online.rs` (the call's state machine: every event and failure).

`tests/online.rs` drives the app with pointer and text events while the
test answers its requests as the server, the microphone and the speaker
(`Ui::testing`), on the same two phones, checking every frame for overflow:

| Test | Proves |
| --- | --- |
| `start_up_asks_the_server_and_takes_its_word` | health, profile, history in turn; online only after all three; the server's learner everywhere |
| `a_device_seen_for_the_first_time_has_nothing_yet` | an empty profile and history draw on every tab |
| `without_a_server_the_app_is_the_scripted_app` | seven ways start-up fails; the local model; a scripted call; the re-check on Home |
| `a_typed_call_from_health_to_history` | start, three typed turns (button and keyboard Send), `done`, end, report, refresh, transcript, history details, the bell |
| `the_red_button_ends_the_call_and_an_unanswered_call_has_no_score` | the unscored report; practise again; a scored early end |
| `a_spoken_call_from_health_to_the_report` | audio fetched and played, microphone held back, recording posted under its own `Content-Type`, replay, sound off, the closing line heard out |
| `the_microphones_failures_are_explained_and_survivable` | refused, busy, interrupted, too short, the 60 s cap, no speech (400), type instead and back |
| `a_failed_typed_turn_loses_nothing_and_can_be_retried` | offline, timeout, 502, 429, a malformed answer |
| `a_failed_spoken_turn_keeps_the_recording` | Retry resends the same bytes; 503 turns to typing |
| `a_call_that_has_ended_on_the_server_goes_to_its_report` | 409; a report that fails and is asked for again |
| `a_call_the_server_has_lost_is_left` | 404; the notice goes by itself |
| `a_call_that_cannot_start_online_is_the_practice_script` | offline and 500 at call start |
| `late_answers_are_ignored` | hung up while ringing, left mid-recording, another call's answers, a retried turn, an older check |
| `edits_are_saved_to_the_server_and_put_back_when_they_fail` | settings, name, plan; the quiet second try; the rollback |
| `the_server_address_is_typed_checked_and_remembered` | the field, a bad address, the check's result, the default, the stored address at launch |
| `a_report_that_cannot_be_loaded_can_be_tried_again` | the history's details failing, and another call's report refused |
| `the_call_fits_with_the_keyboard_up_and_with_long_text` | the field above a 300 dp keyboard; capped titles, names, lines and tips |
| `a_real_typed_call` (ignored) | a typed call against the real server on 127.0.0.1:8787, carried by `curl` |

`tests/e2e.rs` drives `Ui<CallReady>` with pointer events at positions
from `Ui::rect_of`, text events, `Ui::back` and passing time, a frame
after each, on a Galaxy A27 (1080 x 2340 px at 2.8125, insets 102 and
42 px) and on 360 x 640 dp:

| Test | Proves |
| --- | --- |
| `a_complete_journey` | welcome, home, today's call answered chip by chip, derived report, transcript, back, practise again, ended early, report, home |
| `every_control_can_be_reached_and_does_something` | each control in the table can be scrolled into view, tapped on the visible screen, and navigates or changes state |
| `the_table_names_every_id_in_the_source` | the table lists every `.id(..)` written in `src/screens` and `src/components.rs` |
| `only_what_is_already_so_does_nothing` | the only inert taps are the ones listed above |
| `tall_screens_scroll_between_their_fixed_bars` | a drag scrolls by what the finger moved, to the end and back; bars do not move; the last control comes wholly into view; the live call scrolls on the small phone |
| `a_drag_over_a_control_is_not_a_tap` | scrolling from a chip does not choose it |
| `the_profile_name_is_edited_by_typing` | the name through the keyboard path, its limit, Done, and home |
| `settings_reach_home_and_the_next_call` | settings on home and in the next call and its score |
| `each_scenario_starts_its_own_call` | four different calls; Back abandons |
| `the_bell_clears_and_its_notes_lead_somewhere` | the dot, the notes' targets, the bell after a call |
| `history_grows_and_opens_reports` | the list grows; rows and Progress open reports |
| `premium_is_recorded_and_shown` | cross, button, Free; More and the offer change |
| `back_returns_to_where_the_learner_came_from` | the stack, tabs, and Back unhandled on Home |
| `the_clock_runs_with_the_call_and_stops_with_it` | the timer ticks once a second in a call and the app is idle outside one |
| `nothing_overflows_at_four_sizes` | no layout overflow on any screen or journey state at four sizes |

## Accessibility

Every screen is described to a screen reader ([ADR 0032](../../docs/architecture/0032-accessibility.md)); `tests/accessibility.rs` holds it to the framework's check on the test phone and at the design's size, and makes the main journey (welcome, home, a call, an answer, the red button, the report) with nothing but lookups by role and label and `ui.perform`. `cargo run -p callready --example screenshot -- out/ 02 --phone --semantics` draws a screen as a reader finds it.

What was added, without moving a pixel (the fidelity numbers are the same to four decimals):

- **Names for the controls that are only a picture**: Back, Close, Notifications (", unread" while the dot shows), Scenario settings, Start call, Hear the customer again, Mute and Sound off (switches that say whether they are on), End call, Send, Speak instead, Type instead, the microphone ("Speak your answer" / "Stop and send"), and the four tabs (Home, Progress, History, More; tabs, the current one selected).
- **Headings**: each page's title, "Scenarios", today's call, the call's goal, "Call report", "CallReady Pro".
- **Collages read as what they say**: the welcome headline is "Get job-ready with real calls" (not "real", a heart and five letters); "You charged me twice for the same bill" is one sentence; the score's two turned plates are "82 out of 100"; "Call №1 · Angry customer" is "Call number 1 · Angry customer"; a scenario card is its title and "Starts a practice call", without its numeral.
- **Values**: each skill bar is "Clarity, 88 out of 100"; the call's progress bar "Call time used, 32 percent".
- **States**: the chosen plan card, the chosen chip of each setting and the current tab are selected; the tips are disabled while muted.
- **Live regions**: the caller's line, the call's status line ("YOU · SPEAKING", "RECORDING · 0:07 OF 1:00", "CONNECTING…") and the toast are read out when they change.

Changed in the app's own screens, which the design does not draw: the option chips of the settings and the "Retry" pill are still 40 dp tall to the eye and are now tapped through a 48 dp box around them.

**Findings that are the design's own.** The check asks for touch targets of 48 x 48 dp. The design draws these smaller; enlarging them would move designed pixels, so they are left, listed here, and allowed by name in `tests/accessibility.rs`:

| Control | Size in the design | Where |
| --- | --- | --- |
| Back arrow | 44 x 44 | live call, call report, and the app's own pushed screens, which reuse it |
| Close | 44 x 44 | premium |
| Bell | 44 x 44 | home |
| Gear | 44 x 44 | home |
| Loudspeaker ("Hear the customer again") | 44 x 44 | live call |
| Profile link | 46 tall (`min-height: 44px`, the avatar's 46) | home |
| Tip chips | 34 tall (`padding: 6px 12px`) | live call |

The design's colours pass: no text on any of the thirteen screens is below 4.5:1 against a background the check can know (text over the illustrations is not judged).

## What the remaining difference is

- **Turned text** is rasterised for its angle and placed on the pixel
  grid like upright text (ADR 0029); before that it was a resampled
  upright bitmap and was most of what was left on 01, 04 and 05 (0.0274%,
  0.0591% and 0.0147% then). What remains of it is glyph edges a level or
  two off the browser's, as on upright text. Turned *icons* and pictures
  are still resampled.
- **Card 2 on home** is one device pixel wider than in the browser, which
  rounds the frame's width before turning it.
- **`№`, `✓` and `→` on 03** were drawn by the browser with a system font
  (`design/README.md`); here they are small vector icons traced from the
  reference.

## Things done by hand that the framework does not do yet

No size is pinned to a browser's measurement of a hugging text any more, no
line carries a sub-pixel nudge, and no translucent colour is painted opaque:
boxes hug their text with the design's padding, lines take the design's
`line-height`, and `opacity` is an alpha (ADR 0022, ADR 0023).

These are honest costs of the match, and where the next framework work is.
Each is commented where it occurs.

- **No inline boxes in a paragraph** (the ring around "twice", the blank to
  fill in, the handwritten "!!"): each is a separate piece placed by
  measurement, and will not re-flow. The one width still taken from
  Chrome's layout, `CHARGED_WIDTH` on welcome, is the text before such a
  box; and the live call's "Call №1" is three texts and a drawn mark
  spaced by hand, because Chrome took `№` from a system font. So the live call's designed moment is
  exact from 384 dp wide; at 360 its tick sits on the wrong word. A
  running call's blank is a run of low lines, which flows.
- **A view could know the screen's size and does not use it yet.** Pieces
  that must not be cut on a narrow phone (welcome's letters, "PRO", the
  report's note) are still pinned to an edge or placed with `offset`,
  which layout does not count as overflow. `Application::view_in` now
  hands a view the surface's size, and `Viewport::measure` a text's, so
  they could be scaled to fit; the screens' `view` functions do not take
  the viewport yet.
- **Nothing lists the tappable widgets of a frame.** The end-to-end tests
  hold their table of controls to the source text instead.

### No longer done by hand (ADR 0021)

- **`flex-wrap`**: the tip chips are one row that wraps
  (`row![..].gap(8.0).line_gap(8.0).wrap()`), in the scripted call and in
  the server's. At the design's width that is the design's two lines,
  pixel for pixel (`03-live-call` differs from its reference in the same
  0.0064% of pixels with the rows written out and with the row that
  wraps). Where chips are shorter or the phone wider they share a line, as
  in a browser: on the owner's phone a call's first two tips now sit
  beside "tip", and that moment no longer needs to scroll.
- **Scrolling to the caller's newest line**: the online call's scroll view
  keeps one id, and when the caller says a new line the application asks
  for it (`Effects::scroll(ScrollTo::top(..))`, queued by `update` in
  `lib.rs`). The newest things are still first.
- **The reply field** is a `TextInput` with `.max_lines(3)`: it grows to
  three lines and then scrolls inside itself with the caret in view.
  Nothing overflows any more, and `tests/online.rs` allows no overflow.
- **Measuring text**: the tips no longer switch form on a count of
  characters. A tip takes the next line when it does not fit and wraps
  inside its chip when it is wider than the screen; the layout decides.
- **The history stack** is a `Navigator<Screen>`
  (`morningstar::ui::Navigator`), with `go_back` built on its `back`.
- **`Ui::rect_of`** answers for a widget scrolled wholly out of its scroll
  view, so the end-to-end tests no longer scroll to look for a control
  (`Ui::visible_rect_of` is what can be seen).
- **The keyboard and the focus** can be read in a view
  (`Viewport::keyboard_inset`, `Viewport::is_focused`); nothing here needs
  them, since the call's fixed bars fit above the keyboard.

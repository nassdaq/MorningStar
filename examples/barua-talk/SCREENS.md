# Barua Talk: the screens

A rooms, chat and calls app reproduced from a design of five pages
(`design/README.md`) and made to work end to end, in two ways:

- **With its server** (`server/`, a small program that holds the Barua Talk
  API key and a database; `server/API.md` is the contract) the rooms, their
  messages, the people, calls from one app to another and calls to phone
  numbers are real, and reach other phones. See "With a server" below.
- **Without it** (the server does not answer, or nothing behind the app can
  make a request, as in a test or a screenshot) it is the scripted app: the
  design's own people, rooms and messages (`src/sample.rs`). A message is
  seen and answered, a person who is "online" answers a call two seconds
  later, a phone "picks up". Nothing leaves the phone.

**A real call carries sound; no call carries pictures.** The ticket the
server hands out for a call or a room (`token`, `url`) is what the app
joins a live room with (the framework's live-room service,
`docs/architecture/0038-live-rooms.md`: LiveKit's client, on Android), and
the people in that room hear each other. See "Sound" below for when it
joins, what its buttons do and what it says. There is no video yet: a real
call shows the other person's face instead of a video picture and no
camera picture of the user, and says "Video needs the next version." In
the scripted app nothing is joined and nothing sounds: a call there is its
script, and the call screen is the design's frame, whose video still of
Baraka and camera tile are the design's pictures, not a camera.

**None of the sound has been heard.** It is written, tested against a
stand-in on the development machine and built into an APK; it has not run
on a phone, and the real server could not hand out a ticket while it was
written. "The test on a phone" below is what remains to be done.

There is no account: the app asks for a name once and the server knows the
installation by an id. The app holds no secret.

```sh
cargo test -p barua-talk -j2                                              # fidelity, journeys, accessibility, the app against a stand-in server, the sound of its calls
MORNINGSTAR_APP=barua-talk MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh   # the APK, with the live-room service its Cargo.toml declares
cargo run -q -p barua-talk --example screenshot -- out/ 03 --diff         # one page against the design
cargo run -q -p barua-talk --example screenshot -- out/ --phone --states --sheet   # everything on a Galaxy A27
cargo run -q -p barua-talk --example screenshot -- out/ --phone --online --sheet   # the states with a server
cargo run -q -p barua-talk --example icon                                 # redraw assets/icon.png from the app's mark
python3 design/extract-icons.py && python3 design/prepare-images.py       # from examples/barua-talk: the icons and the PNG pictures again
```

## The design's pages

Rendered at 780 x 1688 px and compared with `design/reference` by
`tests/fidelity.rs` with `morningstar-diff`'s defaults (colour error over
2, edges within 1 px). Nothing is masked: the photographs are compared
with everything else. The limit is the measured figure rounded up to the
next 0.05.

| Page | Module | Measured | Limit | What it is |
| --- | --- | --- | --- | --- |
| `01-home` | `screens/home.rs` | 0.0847% | 0.10% | The greeting, five rings, three tabs, five rows, the bar: twelve photographs |
| `02-room` | `screens/room.rs` | 0.6900% | 0.70% | A voice message, a photograph, a notice, the user's bubble, the message field |
| `03-incoming` | `screens/incoming.rs` | 0.0154% | 0.05% | The caller over their blurred picture, two rings, three buttons |
| `04-video-call` | `screens/video_call.rs` | 0.2860% | 0.30% | A video still edge to edge, the camera tile, the volume, the arched bar |
| `05-phone-call` | `screens/phone_call.rs` | 0.0499% | 0.05% | The portrait, the clock, the card of three facts, three controls, the red button |

At the design's size nothing scrolls and nothing moves (tested): a page
opened with `BaruaTalk::designed(name)` is a still frame until it is
touched.

### What the remainder is made of

Every page's layout matches Chrome's to the pixel row; what differs is how
pictures, glyphs and curves are sampled and rasterised.

- **Photographs reduced a lot** are nearly all of pages 02 and 04. The
  shop (1000 x 750 drawn 472 px wide) differs in about 8,200 pixels with a
  mean error of 5.5, the camera tile (570 x 800 drawn 208 px wide) in
  about 3,700: the renderer reduces a picture through mipmaps, which is
  softer in fine detail than Chrome's filter. Edges, colours and the crop
  agree. The portraits (480 px drawn at 42 to 264 px) differ in a few
  hundred pixels each at a mean error of 3 to 4, mostly at the round edge.
  The video still, which is enlarged (627 px to 780), is within the
  threshold everywhere, and so is the blurred backdrop of page 03.
  Shipping the pictures at exactly the reference's size would shrink these
  figures and help no phone: they are shipped as the design has them.
- **Text**: a few pixels per word, as in every example; a little more on
  white text over the accent (the user's bubble, mean error 5 to 8).
- **Icons and round edges**: single pixels.

## What every control does

Navigation follows the design's links; where the design has no page for a
link, the app does something a person would expect and says so here.

| Where | Control | What it does |
| --- | --- | --- |
| Home | The user's picture | The "You and the server" screen (app's own). A dot on it says whether the server answers, only when there is one to speak of |
| | Magnifier | Opens a search field where the tabs are; typing filters the list by name and second line; the button, or Back, closes it (app's own) |
| | "Dial a phone number" | The dial pad (app's own) |
| | "New room" | The form that makes a room (app's own) |
| | A ring | Opens that room. The row scrolls sideways when there are more rings than fit |
| | All, Rooms, Calls | Filter the list: everything, conversations, calls of both kinds. The count beside All is the unread messages in all rooms |
| | A row: a room | Opens the room and marks it read |
| | A row: a call in the app | Calls that person again, as the design's `href="Call.dc.html"` |
| | A row: a phone call | Scripted: calls the number again, as the design's link to its Phone call page. With a server: puts the number on the dial pad, because a phone call costs money and should take a press of its own |
| | The first row (Baraka, scripted) | The design links it to its Incoming page: Baraka rings. With a server there is no such row; a call rings when someone really calls |
| Bar | Home, Notifications, Contacts | The tab's root. Notifications and Contacts (People) are the app's own |
| | "+" ("Start a call or room") | A sheet with "New room", "Call someone", "Dial a number" (app's own); its scrim and Back close it |
| | Calls | Home, with its Calls tab chosen (there is no separate page) |
| Room | Back | Home (or wherever it was opened from) |
| | Phone ("Add a phone call to this room") | The dial pad, as the design's link. The call is its own: the contract cannot put a phone into a room (`server/APP-NEEDS.md`) |
| | Camera ("Start video in this room") | The call screen as the room's call. With a server a ticket for the room is taken and the room's sound is joined with it |
| | Play | Scripted sample only. The accent runs along the bars and the length counts down; a toast says the sample has no sound; a second tap stops it |
| | Paperclip | A toast: no file picker in this version |
| | The field, Send, the keyboard's Send | Adds the message and follows it down. Scripted: it is "Seen" a second later and answered a second after that. With a server it is posted (below) |
| | "Not sent … Tap to try again" (server) | Sends the same message again under the same id, so that it is never stored twice |
| Incoming | Accept | The call screen, its clock at 00:00 |
| | Decline | Ends it; back to where one was. Scripted, the call joins the list as missed |
| | "Reply with a message instead" | Declines. Scripted: opens the caller's conversation. With a server: says that messages between two people need the next version (rooms are the only place to write, and everyone sees them) |
| | Cancel (a call one makes) | Gives it up. The app's own state of this screen: "Inaita…" and one button |
| Video call | Back | Ends the call (or leaves the room's call, the design's "Back to chat"), leaves its sound, and goes back. There is no call in the background in this version |
| | Add someone | A toast: needs the next version |
| | The volume | Its upper half turns it up a step, its lower half down, six steps; a screen reader gets a slider. It changes a number only: on a phone the volume keys set a real call's volume |
| | Microphone | A switch. With a server it mutes and opens the microphone in the call's live room; a call muted before its sound has connected connects muted |
| | Camera | A switch. Scripted, the camera tile shows the design's picture or, off, an icon. With a server the tile never shows a picture: there is no video |
| | Share screen | A toast: needs the next version |
| | Record | Starts and stops the recording; the pill "Recording · TSh 30 / min" shows while it runs, as in the design's own script. With a server it is a real, priced recording (below) |
| | Red button | Hangs up, which leaves the call's sound; back to where the call was started |
| Phone call | Mute, Speaker | Switches. With a server Mute closes and opens the microphone in the call's live room, and Speaker moves its sound between the earpiece (where a phone call starts) and the loudspeaker |
| | Keypad | A sheet with twelve keys and the keys pressed (app's own). They change the line only: tones need the media that is not there |
| | Red button, Back | Scripted: ends the call. With a server that cannot end a phone call (`can_hang_up` false): leaves it, is read as "Leave call", and says "The call ends when the other side hangs up." |
| Anywhere | System Back | Closes the keyboard, then a sheet or the search, then pops; ends a call from its screen, and with it the call's sound; on a tab's root it is left to the system |

## With a server

`server/API.md` is the contract, `src/api.rs` the client (requests built,
answers decoded and cleaned; nothing from a server is trusted, and a ticket
is never printed), `src/lib.rs` the state. `src/json.rs` is a copy of
CallReady's small JSON reader. `server/APP-NEEDS.md` lists what the design
shows that the contract cannot say.

**Start-up.** `GET /v1/health`; then `PUT /v1/me` with the user's name,
before anything else, so that the identity the server derives is the
name's; then `GET /v1/rooms` and `GET /v1/activity`. If all four answer and
the database is up, the app is *online* (`Link::Online`): the rings are the
server's rooms, the list is the server's activity, the people are fetched
when their screen opens, and `GET /v1/account` gives the caller id and the
two prices. If the server does not answer, the app is *offline*
(`Link::Offline`, with the reason on the server screen) and behaves exactly
as the scripted app; it asks again each time Home is reached. With nothing
behind the app that can make a request (`Ui::new`: the fidelity tests, the
screenshots) the first question fails at once and the app is *standalone*:
no dot, the design's pixels. The scripted world and the server's are never
mixed: going online or offline drops whatever room or call was open.

**Polling**, on the app's one timer (four ticks a second, only while
something needs it; online it always does):

| Asked | Every | While |
| --- | --- | --- |
| `GET /v1/calls/incoming` | 2.5 s | No call is in progress. A call found opens the Incoming screen |
| `GET /v1/rooms` + `/v1/activity` | 5 s | Home or Notifications is on top; also on reaching them and after a call |
| `GET …/messages?after=N&read=1` | 2 s | A room is on top (the first asking has no `after`) |
| `GET /v1/people` | 5 s | The People screen is on top |
| `GET /v1/calls/{id}` | 2 s ringing, 5 s answered | A call with a person |
| `GET /v1/phone-calls/{id}` | 4 s | A phone call |

A poll is never sent while its last asking is unanswered.

**A message** is posted with a `client_id` made from the installation and
a counter kept on the phone. It shows at once as "Sending…", then "Sent";
if the post fails it stays, says why, and a tap sends it again under the
same id. A poll that brings the user's own message before its post is
answered does not show it twice. Messages are text: the voice message, the
photograph and the server's notice of the design exist only in the sample.

**A call to a person.** `POST /v1/calls`, then the Incoming screen's
"Inaita…" until a poll says `active` (the call screen), `declined`,
`missed` or `ended` (a toast, and back). Ringing in: accept and decline
are posted; accepting a call the caller has given up says so. Hanging up
is posted and not waited for. A call given up before the server has
answered that it exists is hung up when that answer comes. The clock is the
app's ticks, corrected by the server's `seconds` at each poll.

**A room's call.** `POST …/ticket` with `video: true`; the call screen
shows the room, and when the ticket comes the room's sound is joined with
it. There is nothing to ring and nothing to poll.

**Recording** (TSh 30 a minute) is `POST /v1/recordings` for the call or
the room and `POST …/stop`; what the server refuses (`402`, `409`, `503`)
is said in a toast with the server's own sentence, and nothing is shown as
recording that is not. A stop that fails leaves it shown as recording.

**A phone call** (TSh 300 a minute) is `POST /v1/phone-calls` with a
`request_id` that is new for every press, from the same counter, so that
one press never dials twice. A refusal (`phone_unavailable`,
`balance_empty`, `no_caller_number`, …) brings the dial pad back with the
server's sentence under the number. When it ends the toast gives its
length and its price.

**Times.** The app has no clock or time zone, so with a server it says how
long ago by the server's `now` ("Sasa", "5 min", "3 h", "Jana", "Ijumaa",
"2/10") and never a time of day; the greeting is "Karibu" rather than the
design's "Habari za asubuhi".

## Sound

`src/lib.rs`, "the sound". The app joins one live room at a time
(`Effects::join_live`) with the ticket it holds, and one function,
`follow_call`, run after every message, keeps "the call in progress" and
"the room the app asked for" the same: whatever ended a call, its room is
left there, in one place.

| The call | Its room is joined | Microphone | Sound comes out of |
| --- | --- | --- | --- |
| Between two people (ringing in or out) | When the call becomes `active`: on accepting, or when a poll says the other side answered. Not while it rings | As the microphone button stands | The loudspeaker (the call screen is looked at) |
| A room's | When the room's ticket arrives | The same | The loudspeaker |
| To a phone | As soon as the server answers the dial with its ticket, while the other phone still rings, so that the first words of whoever picks up are heard | As Mute stands | The earpiece, until Speaker is pressed |

**Left** when the red button, the back arrow or Back ends the call; when
the other side ends it, declines or does not answer; when the server stops
answering and the app falls back to its script; when a second call takes
its place. A call that is declined, missed or refused by the server never
joins anything. **The scripted app** (offline, standalone, the design's
pages) **never joins anything.**

What the call screen says (`sound_note`, the pill under the header on the
call screen, the line under the header on a phone call; id `sound-note`):

| State (`Sound`) | The sentence |
| --- | --- |
| A room's call waiting for its ticket | "Connected. Waiting for the sound's ticket…" |
| The server sent no ticket | "Connected without sound: the server sent no ticket." |
| `Joining` (the microphone may be being asked for) | "Connecting sound…" |
| `On`, nobody else in the room yet | "Sound is live. Waiting for the other side." |
| `On` | "Sound is live." On a call with video: "Sound is live. Video needs the next version." |
| `Reconnecting` | "Reconnecting sound…" |
| `Away` (the app left the screen; the platform left the room) | "Sound paused while the app was away." The next tick, which only comes when the app is on screen again, joins the room anew, muted if Mute is on |
| The microphone was refused | "No sound: the microphone is not allowed. Allow it in Settings, then call again." |
| The connection failed | "No sound: the live connection failed. Hang up and call again." |
| The connection was lost for good | "The sound was lost with the connection. Hang up and call again." |
| The room was closed from the other side | "The sound was ended from the other side." |
| The app was built without live rooms, or runs on iOS | "No sound: live audio is not part of this build of the app." |

A failure is also announced to a screen reader. After a failure the call
itself goes on (it is the server's, and can still be recorded and hung
up) and its room is **not** joined again by itself: hanging up and calling
again is the way, as on a phone. Events of a room that was left for
another call are told apart by the call's serial and change nothing.

Not there: video; a call that continues behind another app or a locked
screen (the screen is kept on during a call instead); a Bluetooth headset;
the proximity sensor; tones from the keypad; a ringing sound; any sound on
iOS.

`tests/sound.rs` (27 tests) drives all of this with fingers against the
stand-in server while the test plays the room.

`tests/online.rs` drives all of it with fingers against a stand-in server
(`tests/support`) that answers as the contract describes: the order of the
first requests, polling, messages and their failures, calls in every
ending, a late answer, recording, phone calls and their request ids, the
offline fallback and return. **The app has not talked to the real server.**

## The app's own design

Not from the design, built from its parts and tokens:

- **Welcome** (`screens/welcome.rs`): the first run's question, "Your
  name", with the app's mark (the design's send plane on a disc of the
  accent, which is also the launcher icon). Shown once; the name is kept on
  the phone.
- **Dial** (`screens/dial.rs`): the phone call's header and pill, the
  number in the clock's type, twelve round keys in the control buttons'
  colours (plus, zero and a delete key in the last row), the price, and a
  green button like the design's Accept.
- **New room** (`screens/new_room.rs`): a room's header, its message field
  as a name field, the main button.
- **People** (`screens/people.rs`): the Contacts tab. Home's rows with a
  dot for who is online and round buttons for a voice call, a video call,
  or, for a contact with only a phone number, their phone.
- **Notifications** (`screens/notifications.rs`): the Notifications tab.
  Home's rows that have unread messages or are a missed call.
- **You and the server** (`screens/server.rs`): the name, the address in
  use, how the app stands with the server in plain words, two buttons, and
  how to run one.
- **The sheet of things to start, the keypad during a phone call, the
  search field, the toast**, the main and the outlined button, the text
  field (the room's message field).
- **States the design does not picture**: a call being made ("Inaita…",
  "Inaunganisha…", one button); a voice call's pill and icons; a control
  switched on (the colours swapped, as the bar's middle button is drawn)
  with a struck-through icon for the microphone and camera; the camera
  tile without a picture; the record button's dot in the text colour when
  not recording (from the page's script); the phone call before it is
  answered; a message of words from someone else (the first bubble's
  padding, the caption's type); "Sending…", "Sent", "Not sent"; an empty
  list, room or people; a room or person without a photograph (initials on
  a disc in the "+1" disc's colours); the server's dot on the user's
  picture; the line that says how a real call's sound stands.
- **Motion**: the design declares none. The screens' slides, the sheets'
  rise, the toast and the bars of a playing voice message are the app's,
  and stop with the system's "reduce motion" where the framework animates
  them.
- **On a phone shorter than 844 dp** the bars stay and the middle scrolls.
  The call screen moves its volume up to stay clear of the bar and closes
  its five buttons up on a phone narrower than 390 dp. The phone call
  gives up the status bar's share of its top padding and a part of each
  gap, so that the red button stays on a Galaxy A27's screen; at the
  design's size the layout is the design's exactly.

## Hand-tuning

Two things are placed by arithmetic of the app's own, because the framework
cannot do what the CSS says:

| Where | By hand | The CSS it stands in for |
| --- | --- | --- |
| Home, "Free, however long"; a bubble's time | The smaller text is moved down by the difference of the two ascents, each rounded to a device pixel (`theme::inter_baseline_drop`: 2 dp and 1 dp) | `align-items: baseline` between 14 or 13 px and 12 px text. The layout engine has no baseline alignment |

Five things are drawn another way than the CSS draws them, with the same
picture:

- The blurred backdrop of page 03 (`filter: blur(40px) brightness(0.45)`
  on an `img`) is a picture made beforehand by `design/prepare-images.py`:
  the framework blurs a box's background, not a picture. Away from what
  is drawn over it, it agrees with Chrome's to a level or two of 255.
- The two pictures of page 04 with an `object-position` are cut to the
  window it shows by the same script: the app's pictures are centred.
- The arched top of page 04's bar (`border-radius: 50% 50% 0 0 / 28px …`)
  and the squeezed red button of page 05 (a 72 by 49.5 ellipse) are vector
  pictures: a box's corners are round, not elliptical.
- The waveform of the voice message (an SVG of two paths) is sixteen
  boxes, so that the played part can move.
- Hairlines (`border-top`, `border-bottom`) are dividers, as in every
  example.

No size is pinned and no text is nudged otherwise. **No framework crate
was changed for the design**: everything it needs was already there. (The
sound of a call is a framework service added later, ADR 0038.)

## Accessibility

`tests/accessibility.rs` runs the framework's check on every screen and on
the states a journey adds, at the design's size and on the Galaxy A27, and
makes the main journey (open a room, write, call someone, mute, turn the
volume up, hang up, dial a number, accept an incoming call) by the
semantics tree and its actions alone. Icon buttons have names (the
design's own `aria-label`s), a row is one stop that says everything about
it, the tabs say which is selected, mute, camera and speaker are switches,
the volume is a slider, the photograph in the chat has the design's
description, and toasts, the call's state and the dial pad's number are
announced.

The check finds nothing but touch targets under 48 dp that are the
design's own sizes. They are allowed in the test, each with its reason,
and left as designed, because enlarging them would move the pixels the
fidelity tests hold:

| Target | The design's size |
| --- | --- |
| Back arrows | 44 x 44 |
| Search, "Dial a phone number" | 44 x 44 |
| A room's phone and camera buttons | 44 x 44 |
| The play button of a voice message | 44 x 44 |
| The paperclip | 44 x 44 |
| "Add someone to the call" | 44 x 44 |
| Home's three tabs | 44 high |

No contrast finding: every text of the design passes. The controls that
are the app's own are 48 dp or more, and a second test puts the app online
against the stand-in server and checks those states with nothing more
allowed.

## The test on a phone

**Not done.** Nothing below has been tried: the app's sound has never run
on a phone, and while it was written the Talk API answered 500 to making
rooms and calls, so there was no ticket to join anything with. This is the
test to do once it answers, and the log line that proves each step. All
lines have the tag `Morningstar` (`adb logcat -s Morningstar`); the ones
that begin `live:` are the host's (`MorningstarLive.kt`), the ones that
begin `service:` are the Rust side's. `N` is the join's number.

Build and install as usual (`MORNINGSTAR_APP=barua-talk tools/build-android.sh
--install`; the script's last line should say `with services: live`), give
the app a name, and on "You and the server" check that it is online.

**0. Without any server: the door.** Install `services-check` the same way
and press "Join a live room". Expected: Android's microphone question the
first time; then the line under the button says `the live room failed: …`
(nothing listens at `ws://127.0.0.1:7880`). In the log:

```
service: join live room microphone=live sound=loudspeaker id=N
live: join id=N microphone=live sound=loudspeaker
live: id=N failed: …                       (a connection error, never a ticket)
live: id=N room released
service: id=N live room: the live room failed: …
live: no room is joined, audio mode normal
```

This proves that LiveKit's libraries load beside the app's, that the
permission question is asked from a join, and that a failure comes back.
Refuse the question once (reinstall, or revoke it in Settings) and the
line must be `the microphone is not allowed`, with `live: id=N not joined:
microphone permission denied`.

**1. A room, with the console.** Open `barua.tz/talk/console` on a computer
and join a room there with its microphone on. On the phone open the same
room and press the camera button (top right). Expected on the screen:
"Connecting sound…", the microphone question the first time, then "Sound
is live." with "Video needs the next version."; speak on each side and
listen on the other.

```
service: join live room microphone=live sound=loudspeaker id=N
live: join id=N microphone=live sound=loudspeaker
live: id=N connected, microphone published, 1 other(s) in the room
service: id=N live room connected
live: id=N someone is in the room (1 other(s) now)
service: id=N live room: someone is in it
live: id=N audio mode in communication, sound on the loudspeaker
```

- **Mute** on the phone: the console stops hearing it. `live: id=N
  microphone muted`, then `microphone on`.
- **Leave on the console**: the phone says "Sound is live. Waiting for the
  other side." `live: id=N someone left (0 other(s) now)`.
- **Back to chat** on the phone: `service: leave live room id=…`, `live:
  leave`, `live: id=N room released`, `service: id=N live room over: the
  room was left`, and a second later `live: no room is joined, audio mode
  normal`. Music played in another app before and after should sound as it
  did: the call's audio mode is given back.

**2. A call between two phones** (free). Two phones with the app, two
names. On one, Contacts, the other person's phone button. The other rings;
accept. Expected: nothing is joined while it rings (no `live:` line), and
on accepting both phones join and say "Sound is live." Talk both ways.
Then, one at a time:

- **Hang up on one**: that phone logs `live: leave` and `room released` at
  once; the other, within five seconds (its poll), logs the same and goes
  back, and says "Call ended".
- **Decline** a second call: neither phone logs a `live: join`.
- **Leave the screen during a call** (Home): `live: id=N left because the
  app left the screen`, `room released`, `service: id=N live room over: the
  app left the screen`; the other side hears nothing more. Come back: the
  note says "Sound paused while the app was away." for an instant, then a
  new `live: join id=M`, and sound again.
- **Aeroplane mode for ten seconds, then off**: "Reconnecting sound…" and
  `live: id=N reconnecting`, then `reconnected` and sound again. Left on
  for a minute: "The sound was lost with the connection. Hang up and call
  again." and `live: id=N disconnected by the room (…)`.
- **Refuse the microphone** (revoke it in Settings first): "No sound: the
  microphone is not allowed. Allow it in Settings, then call again." The
  call's clock still runs and the red button still hangs up.
- **A wired headset** plugged in during a call takes the sound.

**3. A phone call** (TSh 300 a minute: the owner's decision). Dial a
number. Expected: `live: join id=N microphone=live sound=earpiece` as soon
as the dial is answered by the server, the ringing or the other person in
the earpiece, **Speaker** moving it (`live: id=N sound to the loudspeaker`,
then `… to the earpiece`), **Mute** silencing the microphone for the other
phone.

**4. No secret in the log, and nothing in an app that did not ask.**

```sh
adb logcat -d -s Morningstar | grep -c 'eyJ\|wss://'    # 0: no ticket, no address
MORNINGSTAR_APP=atlas MORNINGSTAR_ABIS=arm64-v8a tools/build-android.sh
unzip -l templates/android/app/build/outputs/apk/debug/app-debug.apk | grep -ci 'livekit\|jingle\|kotlin'   # 0
```

What to write down: the phone and its Android version, which steps were
done, and for anything that differs the lines around it. Then move the
rows of `docs/architecture/README.md` that say "Not verified" for live
rooms.

## Not verified

Everything here was seen on macOS/Metal only. **Nothing has run on a
phone.** The APK is built (34.2 MB for arm64 with the live-room service,
of which the service is 19.6 MB) and was never installed. That includes
all of the online behaviour: **the app has never talked to the real
server** (which was being written at the same time), only to the stand-in
of `tests/support`, which was written from the same `API.md` by the same
hand as the client and so cannot catch a misreading of it. A phone's own
HTTP stack, `adb reverse`, the polling's cost in battery and in the
server's allowance, and two phones calling each other are untried.

**No sound has been heard.** The live room was joined only by the test
driver: no microphone was opened, no LiveKit server was connected to (the
real API answered 500 to making rooms and calls, so no ticket existed),
and what the host does with a join (`MorningstarLive.kt`) compiles and has
never run. Whether the ticket's identity, the room's name and the moment
the app joins fit what the server and the other side expect is unknown:
in particular, that a phone call's line is in the room the caller's ticket
leads to, and that both sides of a call between two people are given the
same room. On iOS there is no sound at all: the join is refused and the
app says so.

Unseen on a device are every screen, the photographs (their mipmaps on
Vulkan or GLES and about 20 MB of textures), the keyboard over the fields,
the sheets, the sideways drag of the rings, TalkBack, and the launcher
icon (looked at only in `morningstar-icon --preview`).

# What the app would need from the server

Written by the app's side (`examples/barua-talk`, not `server/`). The app is
coded against `API.md` as it is written; nothing below is assumed to exist.
Each item is something the design shows, or a person would expect, that the
contract has no way to say. Where the app does something else instead, that
is noted, and `../SCREENS.md` says the same to its reader.

1. **A notice in a room that is not a person's message.** The design's room
   has a pill in the middle of the chat, "server · Rehema joins in 2
   minutes". A message in the contract has `from`, `from_name`, `mine` and
   `text`, and nothing that tells a server's notice from what a person
   wrote. *Today:* every message from the server is drawn as a person's
   bubble; the pill is only in the scripted sample. *Would do:* a `kind`
   (`"message"` | `"notice"`) on a message, or a documented reserved `from`.

2. **Who has read a message.** The design shows "Seen by 3" under the
   user's last message. The contract has a read mark per person and an
   `unread` count for oneself, not how many others have read up to a
   message. *Today:* the app says "Sent" under a stored message. *Would do:*
   `seen_by` (a count) on a message, or on the page of messages for the
   latest one that is `mine`.

3. **How many people are in a room now.** The design's header says "4
   active". The contract has `members` (ever) and `live` (something
   happened in ten minutes), and says why it cannot know who is connected.
   *Today:* the header says "3 members". Nothing asked for; noted so that
   the difference from the design is not a surprise.

4. **A conversation between two people.** The design's Home lists people
   (Baraka Mushi, Juma Kileo) beside rooms, and its Incoming page has "Reply
   with a message instead". Rooms are the only place to write, and every
   person sees every room. *Today:* with a server, "Reply with a message
   instead" declines the call and says that messages between two people
   need the next version; it does not make a room that everyone could read.
   *Would do:* a room only its members see, or a `POST /v1/people/{identity}/messages`.

5. **A phone call into a room.** The design's room has "Add a phone call to
   this room", and upstream says a phone call *is* a room with a phone leg.
   `POST /v1/phone-calls` takes only `to`. *Today:* the button opens the
   dial pad and the call is its own. *Would do:* an optional `"room"` in
   that body.

6. **Pictures of people and rooms.** The design is full of portraits; a
   person in the contract is an identity and a name. *Today:* initials on a
   disc. Nothing asked for in this version.

7. **Voice messages and pictures in a room.** The design's room shows both;
   the contract's chat is text. *Today:* they exist only in the scripted
   sample. Nothing asked for in this version.

8. **The time of day.** The design writes "08:02" and "Jana". The contract
   gives UTC times and `now`, and the app has no clock or time zone of its
   own (the framework gives an application none), so it says how long ago
   instead ("5 min", "3 h", "Jana", "Ijumaa"). That is the app's limit, not
   the server's; it is here only so that nobody looks for a missing field.

Two things the contract already covers and the app relies on, said here so
that they stay: `client_id` and `request_id` make a repeated send or dial
harmless (the app numbers both with a counter it keeps on the phone, so
that an id is never used twice by one installation), and a polled answer
carries `now`.

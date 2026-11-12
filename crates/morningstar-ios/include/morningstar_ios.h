// C interface of the Morningstar iOS host (crates/morningstar-ios/src/lib.rs).
// Every function must be called on the main thread.
//
// Frames are produced on demand. After every call into this interface, ask
// morningstar_ios_needs_frame and pause or resume the CADisplayLink to match:
// the answer changes only inside these calls, and when the time given by
// morningstar_ios_redraw_delay has passed.
#ifndef MORNINGSTAR_IOS_H
#define MORNINGSTAR_IOS_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct MorningstarHost MorningstarHost;

// Creates the host. Destroy with morningstar_ios_destroy. The application
// is handed nothing at setup: no stored values, a placeholder installation
// id. A host that offers platform services (below) uses
// morningstar_ios_create_with_state instead.
MorningstarHost *morningstar_ios_create(void);

// Creates the host and hands the application what it may read when it is
// set up: install_id, this installation's identifier (32 hexadecimal digits,
// generated once with the system's secure random and kept by the host), and
// the values the application stored on earlier runs, as stored_count
// NUL-terminated UTF-8 strings that are keys and values in turn. Everything
// is copied before the call returns.
MorningstarHost *morningstar_ios_create_with_state(const char *install_id,
                                                   const char *const *stored,
                                                   size_t stored_count);

// Attaches to a CAMetalLayer*. The layer must stay alive until
// morningstar_ios_detach_layer or morningstar_ios_destroy. Sizes are in
// physical pixels. Returns false on failure.
bool morningstar_ios_attach_layer(MorningstarHost *host, void *metal_layer,
                                  uint32_t width_px, uint32_t height_px);

void morningstar_ios_resize(MorningstarHost *host, uint32_t width_px,
                            uint32_t height_px);

void morningstar_ios_detach_layer(MorningstarHost *host);

// Physical pixels per point: the view's contentScaleFactor. Call before
// attaching the layer and whenever it may have changed.
void morningstar_ios_set_scale(MorningstarHost *host, float scale);

// The view's safeAreaInsets, converted to physical pixels. Call whenever they
// may have changed.
void morningstar_ios_set_insets(MorningstarHost *host, float left_px,
                                float top_px, float right_px,
                                float bottom_px);

// Benchmark mode: draw at every display refresh even if nothing changed.
void morningstar_ios_set_continuous(MorningstarHost *host, bool continuous);

// True when the next display refresh should call morningstar_ios_frame. While
// false, the display link should be paused.
bool morningstar_ios_needs_frame(const MorningstarHost *host);

// Pass false before the app is backgrounded: iOS forbids GPU work there.
void morningstar_ios_set_active(MorningstarHost *host, bool active);

// Touch phases for morningstar_ios_pointer.
enum {
  MORNINGSTAR_PHASE_DOWN = 0,
  MORNINGSTAR_PHASE_MOVED = 1,
  MORNINGSTAR_PHASE_UP = 2,
  MORNINGSTAR_PHASE_CANCELLED = 3,
};

// Delivers one touch event. pointer_id must stay the same for one finger from
// down to up. Coordinates are in physical pixels relative to the layer.
void morningstar_ios_pointer(MorningstarHost *host, uint32_t phase,
                             uint64_t pointer_id, float x_px, float y_px);

// A "back" gesture, as the host defines one (iOS has no system Back; a
// swipe from the leading edge is the usual choice). Returns true if the
// application handled it (it went to its previous screen): a frame follows,
// so ask morningstar_ios_needs_frame. Returns false if it had nowhere to go
// back to; the host then does nothing. Answers at once: no frame is built
// inside the call.
bool morningstar_ios_back(MorningstarHost *host);

// Produces one frame if one is needed. Call from a CADisplayLink callback,
// then ask morningstar_ios_needs_frame whether to keep the link running.
void morningstar_ios_frame(MorningstarHost *host);

// ---------------------------------------------------------------- text input
//
// The application decides, frame by frame, whether a text field has focus.
// After every call into this interface the host asks
// morningstar_ios_take_text_input (as it asks morningstar_ios_needs_frame)
// and shows, reconfigures or hides the keyboard accordingly. What the user
// types goes back through morningstar_ios_text_insert and
// morningstar_ios_text_delete_backward (UIKeyInput). Marked text
// (composition, UITextInput) is not part of this interface.

// The keyboard's overlap with the bottom of the layer, in physical pixels;
// 0 when it is hidden. Call from the keyboard frame notifications.
void morningstar_ios_set_keyboard_inset(MorningstarHost *host,
                                        float bottom_px);

// UIKeyInput.insertText: NUL-terminated UTF-8. A lone "\n" is the Return key.
void morningstar_ios_text_insert(MorningstarHost *host, const char *utf8);

// UIKeyInput.deleteBackward.
void morningstar_ios_text_delete_backward(MorningstarHost *host);

// The view resigned first responder without being asked to by a
// MORNINGSTAR_TEXT_HIDE command: the focused field gives up focus.
void morningstar_ios_text_focus_lost(MorningstarHost *host);

// MorningstarTextInput.command.
enum {
  MORNINGSTAR_TEXT_HIDE = 0,   // resign first responder
  MORNINGSTAR_TEXT_SHOW = 1,   // become first responder with these traits
  MORNINGSTAR_TEXT_UPDATE = 2, // the text changed: only has_text matters
};

// MorningstarTextInput.keyboard.
enum {
  MORNINGSTAR_KEYBOARD_TEXT = 0,
  MORNINGSTAR_KEYBOARD_NUMBER = 1,
  MORNINGSTAR_KEYBOARD_DECIMAL = 2,
  MORNINGSTAR_KEYBOARD_PHONE = 3,
  MORNINGSTAR_KEYBOARD_EMAIL = 4,
};

// MorningstarTextInput.capitalization.
enum {
  MORNINGSTAR_CAPITALIZE_NONE = 0,
  MORNINGSTAR_CAPITALIZE_SENTENCES = 1,
  MORNINGSTAR_CAPITALIZE_WORDS = 2,
  MORNINGSTAR_CAPITALIZE_CHARACTERS = 3,
};

// MorningstarTextInput.action: the Return key.
enum {
  MORNINGSTAR_ACTION_NEXT = 1,
  MORNINGSTAR_ACTION_DONE = 2,
  MORNINGSTAR_ACTION_GO = 3,
  MORNINGSTAR_ACTION_SEARCH = 4,
  MORNINGSTAR_ACTION_SEND = 5,
};

typedef struct MorningstarTextInput {
  uint32_t command;
  uint32_t keyboard;
  uint32_t capitalization;
  uint32_t action;
  bool secure;
  bool multiline;
  bool autocorrect;
  bool has_text; // for UIKeyInput.hasText
} MorningstarTextInput;

// Fills *out and returns true if the keyboard must be shown, reconfigured or
// hidden; returns false (the usual case) when there is nothing to do.
bool morningstar_ios_take_text_input(MorningstarHost *host,
                                     MorningstarTextInput *out);

// morningstar_ios_text_command's command: the standard edit actions of
// UIResponder and the shortcuts of a hardware keyboard. The application's
// field does them; copy and paste then arrive as service requests
// (MORNINGSTAR_SERVICE_SET_CLIPBOARD, MORNINGSTAR_SERVICE_READ_CLIPBOARD).
enum {
  MORNINGSTAR_EDIT_SELECT_ALL = 0,
  MORNINGSTAR_EDIT_COPY = 1,
  MORNINGSTAR_EDIT_CUT = 2,
  MORNINGSTAR_EDIT_PASTE = 3,
  MORNINGSTAR_EDIT_UNDO = 4,
  MORNINGSTAR_EDIT_REDO = 5,
  MORNINGSTAR_EDIT_DELETE_FORWARD = 6,
};

void morningstar_ios_text_command(MorningstarHost *host, uint32_t command);

// morningstar_ios_text_move's movement: a key that moves the caret.
enum {
  MORNINGSTAR_MOVE_LEFT = 0,
  MORNINGSTAR_MOVE_RIGHT = 1,
  MORNINGSTAR_MOVE_WORD_LEFT = 2,
  MORNINGSTAR_MOVE_WORD_RIGHT = 3,
  MORNINGSTAR_MOVE_UP = 4,
  MORNINGSTAR_MOVE_DOWN = 5,
  MORNINGSTAR_MOVE_LINE_START = 6,
  MORNINGSTAR_MOVE_LINE_END = 7,
  MORNINGSTAR_MOVE_TEXT_START = 8,
  MORNINGSTAR_MOVE_TEXT_END = 9,
};

// extend is Shift: select from where the caret was.
void morningstar_ios_text_move(MorningstarHost *host, uint32_t movement,
                               bool extend);

// Seconds until the app wants a frame with nothing else happening (a caret's
// blink), or negative when it wants none. While morningstar_ios_needs_frame
// is false and this is not negative, arm a one-shot timer for that long and,
// when it fires, ask morningstar_ios_needs_frame again.
double morningstar_ios_redraw_delay(const MorningstarHost *host);

// ----------------------------------------------- system bars, reduce motion

// UIAccessibility.isReduceMotionEnabled. Call at start and from
// reduceMotionStatusDidChangeNotification; then ask
// morningstar_ios_needs_frame.
void morningstar_ios_set_reduce_motion(MorningstarHost *host, bool reduce);

// Whether the status bar's content and the home indicator should be dark
// (over a light screen).
typedef struct MorningstarSystemBars {
  bool dark_status_content;
  bool dark_navigation_content;
} MorningstarSystemBars;

// Fills *out and returns true when the frame last built wants the bars to
// look different from how the host was last told; false (the usual case)
// otherwise. Call after every morningstar_ios_frame; on true, answer
// preferredStatusBarStyle accordingly and call
// setNeedsStatusBarAppearanceUpdate.
bool morningstar_ios_take_system_bars(MorningstarHost *host,
                                      MorningstarSystemBars *out);

// -------------------------------------------------------------- accessibility
//
// The application draws its own pixels, so VoiceOver sees one opaque view
// unless it is described (ADR 0032). While accessibility is on, every frame
// is described by a tree of nodes, and what changed in it is handed out
// after the frame as one buffer. The host keeps the tree, vends one
// UIAccessibilityElement per node from the view, and sends actions back.

// UIAccessibility.isVoiceOverRunning (or Switch Control). Call at start and
// from voiceOverStatusDidChangeNotification; then ask
// morningstar_ios_needs_frame. While false nothing is built.
void morningstar_ios_set_accessibility(MorningstarHost *host, bool enabled);

// Points *data at *len bytes and returns true when the meaning of the
// screen changed since the host was last told; false (the usual case)
// otherwise. Call after every morningstar_ios_frame while accessibility is
// on. The bytes stay valid until the next call to this function,
// morningstar_ios_set_accessibility or morningstar_ios_destroy.
//
// Little-endian. Node ids are positive int32s; the root, which stands for
// the view, is 0. A string is a uint32 byte count and that many bytes of
// UTF-8.
//
//   uint32  version (MORNINGSTAR_A11Y_VERSION)
//   uint32  flags: bit 0 = forget every node held before applying this
//   int32   the node with the input focus, or -1
//   uint32  count of removed nodes, then that many int32 ids
//   uint32  count of added or changed nodes, then for each:
//     int32   id
//     int32   parent id (-1 for the root)
//     uint8   role (MORNINGSTAR_ROLE_*)
//     uint8   what changed (MORNINGSTAR_CHANGED_*)
//     uint16  states (MORNINGSTAR_STATE_*)
//     uint16  actions (bit n set: action code n can be performed)
//     uint16  0
//     float   left, top, right, bottom: bounds in pixels of the layer
//     float   scroll position, scroll range in points (-1, -1 if not a list)
//     int32   selection start, end in UTF-16 units of the value (-1, -1 if
//             none)
//     uint32  count of children, then that many int32 ids, in reading order
//     string  label
//     string  value (bullets for a password field, never its characters)
//     string  hint
bool morningstar_ios_take_semantics(MorningstarHost *host,
                                    const uint8_t **data, size_t *len);

enum { MORNINGSTAR_A11Y_VERSION = 1 };

enum {
  MORNINGSTAR_ROLE_GROUP = 0,
  MORNINGSTAR_ROLE_TEXT = 1,
  MORNINGSTAR_ROLE_HEADING = 2,
  MORNINGSTAR_ROLE_BUTTON = 3,
  MORNINGSTAR_ROLE_LINK = 4,
  MORNINGSTAR_ROLE_IMAGE = 5,
  MORNINGSTAR_ROLE_TEXT_FIELD = 6,
  MORNINGSTAR_ROLE_CHECKBOX = 7,
  MORNINGSTAR_ROLE_SWITCH = 8,
  MORNINGSTAR_ROLE_RADIO = 9,
  MORNINGSTAR_ROLE_TAB = 10,
  MORNINGSTAR_ROLE_LIST = 11,
  MORNINGSTAR_ROLE_PROGRESS = 12,
  MORNINGSTAR_ROLE_SLIDER = 13,
};

enum {
  MORNINGSTAR_STATE_DISABLED = 1 << 0,
  MORNINGSTAR_STATE_SELECTED = 1 << 1,
  MORNINGSTAR_STATE_CHECKABLE = 1 << 2,
  MORNINGSTAR_STATE_CHECKED = 1 << 3,
  MORNINGSTAR_STATE_FOCUSED = 1 << 4,
  MORNINGSTAR_STATE_PASSWORD = 1 << 5,
  MORNINGSTAR_STATE_MULTILINE = 1 << 6,
  MORNINGSTAR_STATE_EXPANDABLE = 1 << 7,
  MORNINGSTAR_STATE_EXPANDED = 1 << 8,
  MORNINGSTAR_STATE_LIVE_POLITE = 1 << 9,
  MORNINGSTAR_STATE_LIVE_ASSERTIVE = 1 << 10,
};

enum {
  MORNINGSTAR_CHANGED_ADDED = 1 << 0,
  MORNINGSTAR_CHANGED_BOUNDS = 1 << 1,
  MORNINGSTAR_CHANGED_LABEL = 1 << 2,
  MORNINGSTAR_CHANGED_VALUE = 1 << 3,
  MORNINGSTAR_CHANGED_STATE = 1 << 4,
  MORNINGSTAR_CHANGED_CHILDREN = 1 << 5,
  MORNINGSTAR_CHANGED_SCROLL = 1 << 6,
  MORNINGSTAR_CHANGED_SELECTION = 1 << 7,
};

// morningstar_ios_accessibility_action's action. A node offers the action
// with code n when bit n of its actions is set.
enum {
  MORNINGSTAR_A11Y_ACTIVATE = 0,
  MORNINGSTAR_A11Y_LONG_PRESS = 1,
  MORNINGSTAR_A11Y_SCROLL_FORWARD = 2,
  MORNINGSTAR_A11Y_SCROLL_BACKWARD = 3,
  MORNINGSTAR_A11Y_FOCUS = 4,
  MORNINGSTAR_A11Y_SET_TEXT = 5,      // text: the new text
  MORNINGSTAR_A11Y_SET_SELECTION = 6, // start, end: UTF-16 units of the value
  MORNINGSTAR_A11Y_INCREMENT = 7,
  MORNINGSTAR_A11Y_DECREMENT = 8,
  MORNINGSTAR_A11Y_DISMISS = 9,
};

// Does to a node what VoiceOver asked for. text may be NULL. Returns true
// if the application carried it out; then ask morningstar_ios_needs_frame.
bool morningstar_ios_accessibility_action(MorningstarHost *host, int32_t node,
                                          uint32_t action, const char *text,
                                          uint32_t start, uint32_t end);

// ---------------------------------------------------------- platform services
//
// The application asks for work that takes time: an HTTP request, a
// recording, playing a clip, a stored value (ADR 0019). Requests appear only
// inside calls into this interface, so after every call the host drains
// morningstar_ios_take_service_request (as it asks morningstar_ios_needs_frame
// and morningstar_ios_take_text_input), does the work off the main thread,
// and answers ON THE MAIN THREAD with one of the *_result functions.
//
// Every request of a kind marked "answered" must be answered exactly once,
// with an error if it cannot be carried out: the application is waiting.
// Never call a *_result function after morningstar_ios_destroy: the host
// must drop answers that arrive for a host it has destroyed.

// MorningstarServiceRequest.kind.
enum {
  // Answered with morningstar_ios_http_result. text1: method ("GET", ...);
  // text2: URL; text3: headers, "Name: value" lines each ended by '\n';
  // data: body (empty for none); timeout_ms: for the whole exchange.
  MORNINGSTAR_SERVICE_HTTP = 0,
  // Answered with morningstar_ios_audio_result, kind STARTED, once the
  // microphone is running or it is known why not.
  MORNINGSTAR_SERVICE_START_RECORDING = 1,
  // Answered with morningstar_ios_audio_result, kind RECORDING.
  MORNINGSTAR_SERVICE_STOP_RECORDING = 2,
  // Answered with morningstar_ios_audio_result, kind PLAYED, when the clip
  // has finished or failed. text1: media type; data: the audio file.
  MORNINGSTAR_SERVICE_PLAY = 3,
  // Not answered; the clip that was playing is answered INTERRUPTED.
  MORNINGSTAR_SERVICE_STOP_PLAYBACK = 4,
  // Not answered. text1: key; text2: value.
  MORNINGSTAR_SERVICE_STORE = 5,
  // Not answered. text1: key.
  MORNINGSTAR_SERVICE_REMOVE = 6,
  // Answered with morningstar_ios_load_result. text1: key.
  MORNINGSTAR_SERVICE_LOAD = 7,
  // Not answered. text1: plain text for UIPasteboard.general.
  MORNINGSTAR_SERVICE_SET_CLIPBOARD = 8,
  // Answered with morningstar_ios_clipboard_result. Read the pasteboard
  // only now: iOS tells the user when an app reads what another copied.
  MORNINGSTAR_SERVICE_READ_CLIPBOARD = 9,
  // Not answered. text1: what VoiceOver should say
  // (UIAccessibility.post(.announcement)); text2: "assertive" to interrupt
  // what it is saying, empty otherwise.
  MORNINGSTAR_SERVICE_ANNOUNCE = 10,
};

// One request. Strings are NUL-terminated UTF-8 and never NULL (unused ones
// are empty); data is NULL when data_len is 0. Everything pointed to stays
// valid until the next morningstar_ios_take_service_request or
// morningstar_ios_destroy: copy what must outlive that.
typedef struct MorningstarServiceRequest {
  uint64_t id; // give this back with the answer
  uint32_t kind;
  uint32_t timeout_ms;
  const char *text1;
  const char *text2;
  const char *text3;
  const uint8_t *data;
  size_t data_len;
} MorningstarServiceRequest;

// Fills *out with the next request and returns true, or returns false (the
// usual case) when there is none. Call in a loop until it returns false.
bool morningstar_ios_take_service_request(MorningstarHost *host,
                                          MorningstarServiceRequest *out);

// morningstar_ios_http_result's error.
enum {
  MORNINGSTAR_HTTP_OK = 0,          // the server answered (any status)
  MORNINGSTAR_HTTP_OFFLINE = 1,     // no network, no such host, refused
  MORNINGSTAR_HTTP_TIMEOUT = 2,
  MORNINGSTAR_HTTP_INVALID_URL = 3, // not an absolute http(s) URL
  MORNINGSTAR_HTTP_TOO_LARGE = 4,   // body over 10 MB
  MORNINGSTAR_HTTP_UNSUPPORTED = 5,
  MORNINGSTAR_HTTP_FAILED = 6,      // anything else; message says what
};

// The answer to a MORNINGSTAR_SERVICE_HTTP request. With MORNINGSTAR_HTTP_OK:
// status, headers ("Name: value" lines) and body. Otherwise those are
// ignored and message (may be NULL) describes a MORNINGSTAR_HTTP_FAILED.
// Everything is copied. Main thread; then ask morningstar_ios_needs_frame.
void morningstar_ios_http_result(MorningstarHost *host, uint64_t id,
                                 uint32_t error, uint32_t status,
                                 const char *headers, const uint8_t *body,
                                 size_t body_len, const char *message);

// morningstar_ios_audio_result's kind: which request is being answered.
enum {
  MORNINGSTAR_AUDIO_KIND_STARTED = 0,
  MORNINGSTAR_AUDIO_KIND_RECORDING = 1,
  MORNINGSTAR_AUDIO_KIND_PLAYED = 2,
};

// morningstar_ios_audio_result's error.
enum {
  MORNINGSTAR_AUDIO_OK = 0,
  MORNINGSTAR_AUDIO_PERMISSION_DENIED = 1,
  MORNINGSTAR_AUDIO_BUSY = 2,          // already recording
  MORNINGSTAR_AUDIO_UNSUPPORTED = 3,
  MORNINGSTAR_AUDIO_NOT_RECORDING = 4, // stop with nothing to stop
  MORNINGSTAR_AUDIO_INTERRUPTED = 5,   // playback ended before the clip did
  MORNINGSTAR_AUDIO_FAILED = 6,        // anything else; message says what
};

// The answer to a recording or playback request. For a stopped recording
// (kind RECORDING, error OK): data is the audio file, mime its media type
// ("audio/mp4") and duration_ms its length. Everything is copied. Main
// thread; then ask morningstar_ios_needs_frame.
void morningstar_ios_audio_result(MorningstarHost *host, uint64_t id,
                                  uint32_t kind, uint32_t error,
                                  const char *message, const uint8_t *data,
                                  size_t data_len, const char *mime,
                                  uint64_t duration_ms);

// The answer to a MORNINGSTAR_SERVICE_LOAD request: the value, or found
// false when the key holds nothing. Main thread.
void morningstar_ios_load_result(MorningstarHost *host, uint64_t id,
                                 bool found, const char *value);

// The answer to a MORNINGSTAR_SERVICE_READ_CLIPBOARD request: the
// pasteboard's string, or found false when it holds no text. Main thread.
void morningstar_ios_clipboard_result(MorningstarHost *host, uint64_t id,
                                      bool found, const char *utf8);

void morningstar_ios_destroy(MorningstarHost *host);

#endif

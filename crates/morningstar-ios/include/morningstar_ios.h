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
#include <stdint.h>

typedef struct MorningstarHost MorningstarHost;

// Creates the host. Destroy with morningstar_ios_destroy.
MorningstarHost *morningstar_ios_create(void);

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

// Seconds until the app wants a frame with nothing else happening (a caret's
// blink), or negative when it wants none. While morningstar_ios_needs_frame
// is false and this is not negative, arm a one-shot timer for that long and,
// when it fires, ask morningstar_ios_needs_frame again.
double morningstar_ios_redraw_delay(const MorningstarHost *host);

void morningstar_ios_destroy(MorningstarHost *host);

#endif

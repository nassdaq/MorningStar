# Ui and testing

`Ui` runs an `Application`. In an app it appears once, in `morningstar::app!(Ui::new(..))`. In a test it is the handle on everything: the app's state, the requests it made, the layout of the last frame, and the semantics tree. [Testing](../guide/testing.md) is the guide.

## Creating one

{{api crates/morningstar-ui/src/ui.rs Ui only=new,testing,with_services}}

`Ui::new` answers every request with `Unsupported` when no host is attached. `Ui::testing` holds requests for the test to answer.

## The application

{{api crates/morningstar-ui/src/ui.rs Ui only=app,app_mut,send,back}}

## Answering effects

{{api crates/morningstar-ui/src/ui.rs Ui only=pending_effects,effects_in_flight,resolve,answer_http,answer_recording_started,answer_recording,answer_playback,deliver_effects,stored,set_stored}}

## Live rooms

{{api crates/morningstar-ui/src/ui.rs Ui only=live_room,live_event,live_app_hidden,live_hang_up_from_notice}}

## Layout and scrolling

{{api crates/morningstar-ui/src/ui.rs Ui only=rect_of,visible_rect_of,overflows,scroll_to,scroll_offset,max_scroll_offset,is_animating,set_reduce_motion}}

## Focus, selection and the clipboard

{{api crates/morningstar-ui/src/ui.rs Ui only=has_focus,is_focused,clear_focus,caret,composing,selection,select,selection_menu_shown,selection_handles,selection_menu,clipboard,set_clipboard}}

## Accessibility

{{api crates/morningstar-ui/src/ui.rs Ui only=set_accessibility,semantics,accessibility_issues,announcements,perform,activate}}

## HostState

What the phone knows before the app starts, for `Frame::for_app_with_host`.

{{api crates/morningstar-core/src/services.rs HostState}}

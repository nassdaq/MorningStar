//! [`Effects`]: how an [`Application`](crate::Application) starts work
//! whose result comes back later as a message, and the bookkeeping [`Ui`]
//! keeps for it: ids, the handlers waiting for answers, and the driver that
//! stands in for a host in tests.
//!
//! [`Ui`]: crate::Ui

use std::collections::{HashMap, VecDeque};

use morningstar_core::{
    AudioClip, AudioError, HttpError, HttpRequest, HttpResponse, LiveAudioRoute, LiveDisconnect,
    LiveError, LiveEvent, LiveJoin, Recording, ServiceCall, ServiceId, ServiceRequest,
    ServiceResult,
};

use crate::scroll::ScrollTo;

/// Turns the platform's answer into the application's message.
type Handler<M> = Box<dyn FnOnce(ServiceResult) -> M + Send>;

/// Turns each event of a live room into the application's message. Unlike
/// a [`Handler`] it is called as often as there are events, and dropped
/// after the one that ends the room.
type LiveHandler<M> = Box<dyn FnMut(LiveEvent) -> M + Send>;

/// What a stand-in for the host answers to each request: `Some` to answer,
/// `None` to leave it pending for the test to answer by hand.
pub(crate) type FakeHost = Box<dyn FnMut(&ServiceRequest) -> Option<ServiceResult> + Send>;

const WRONG_KIND: &str = "the host answered with a result of the wrong kind";

enum Entry<M> {
    Request(ServiceRequest, Option<Handler<M>>),
    /// A live room to join: the one request with a handler that is called
    /// more than once.
    Join(LiveJoin, LiveHandler<M>),
    Forget,
}

/// The live room an application has joined under the test driver
/// ([`Ui::live_room`](crate::Ui::live_room)): what it was joined with, and
/// how the application has set the microphone and the loudspeaker since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveRoom {
    /// What the application joined with. `microphone` and `speaker` in it
    /// are the values at the join; the two fields below are the current
    /// ones.
    pub join: LiveJoin,
    /// Whether the microphone is live now.
    pub microphone: bool,
    /// Whether sound goes to the loudspeaker now.
    pub speaker: bool,
    /// Whether a [`LiveEvent::Connected`] has been delivered.
    pub connected: bool,
    /// Whether the application has the camera on now (ADR 0039).
    pub camera: bool,
    /// How often it has asked for the other camera.
    pub camera_switches: u32,
    /// Whether it is sharing the screen now.
    pub screen_share: bool,
    /// What it has sent to the room, oldest first: topic and payload.
    pub sent: Vec<(String, Vec<u8>)>,
    /// The audio route it has asked for last (ADR 0042), if any.
    pub audio_route: Option<LiveAudioRoute>,
}

/// The work an application wants started: requests to the platform, each
/// with the message its result should become.
///
/// The runtime hands one to [`Application::effects`](crate::Application::effects)
/// after setup and after every `update`. Each call here queues one request;
/// nothing blocks, and nothing has happened yet when the call returns. The
/// platform does the work off the render thread, and the result arrives
/// later as an ordinary message to `update`, followed by a frame.
///
/// The last argument of each request is what makes the message: usually an
/// enum variant that holds the result (`Message::Replied`), or a closure
/// when the message needs more (`move |result| Message::Replied(turn, result)`).
/// Every request is answered exactly once, with an error if it could not
/// be carried out; a request never fails by staying silent.
pub struct Effects<M> {
    entries: Vec<Entry<M>>,
    scrolls: Vec<ScrollTo>,
}

impl<M> Effects<M> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
            scrolls: Vec::new(),
        }
    }

    fn push(&mut self, request: ServiceRequest, handler: Option<Handler<M>>) {
        self.entries.push(Entry::Request(request, handler));
    }

    /// Makes an HTTP or HTTPS request. The message gets the response (any
    /// status, 4xx and 5xx included) or why there was none: offline, timed
    /// out, invalid URL, a response over 10 MB.
    ///
    /// The request runs on the platform's own HTTP stack with the
    /// platform's certificates. Plain `http://` is refused by the hosts
    /// except to the development machine (`localhost`, `127.0.0.1`,
    /// `10.0.2.2`).
    pub fn http(
        &mut self,
        request: HttpRequest,
        on_result: impl FnOnce(Result<HttpResponse, HttpError>) -> M + Send + 'static,
    ) {
        self.push(
            ServiceRequest::Http(request),
            Some(Box::new(move |result| {
                on_result(match result {
                    ServiceResult::Http(result) => result,
                    _ => Err(HttpError::Failed(WRONG_KIND.to_owned())),
                })
            })),
        );
    }

    /// Starts recording from the microphone. The first time, the platform
    /// asks the user for permission, and the message waits for their
    /// answer. `Ok(())` means the microphone is running;
    /// [`AudioError::PermissionDenied`] that the user said no (asking
    /// again later asks them again, unless they refused for good);
    /// [`AudioError::Busy`] that a recording is already running.
    ///
    /// A recording stops by itself after 60 seconds, and when the app
    /// leaves the screen; what was recorded until then is still handed
    /// over by [`stop_recording`](Self::stop_recording).
    pub fn start_recording(
        &mut self,
        on_result: impl FnOnce(Result<(), AudioError>) -> M + Send + 'static,
    ) {
        self.push(
            ServiceRequest::StartRecording,
            Some(Box::new(move |result| {
                on_result(match result {
                    ServiceResult::RecordingStarted(result) => result,
                    _ => Err(AudioError::Failed(WRONG_KIND.to_owned())),
                })
            })),
        );
    }

    /// Stops the recording. The message gets it as the bytes of an audio
    /// file (`audio/mp4`: mono AAC in an M4A container) with its length,
    /// ready to be the body of a request, or
    /// [`AudioError::NotRecording`] if none was running.
    pub fn stop_recording(
        &mut self,
        on_result: impl FnOnce(Result<Recording, AudioError>) -> M + Send + 'static,
    ) {
        self.push(
            ServiceRequest::StopRecording,
            Some(Box::new(move |result| {
                on_result(match result {
                    ServiceResult::Recording(result) => result,
                    _ => Err(AudioError::Failed(WRONG_KIND.to_owned())),
                })
            })),
        );
    }

    /// Plays a clip (`audio/mpeg` or `audio/mp4`), replacing one that is
    /// playing. The message arrives when it has finished: `Ok(())` if it
    /// played to its end, [`AudioError::Interrupted`] if it was stopped
    /// (by [`stop_playback`](Self::stop_playback), another `play`, the app
    /// leaving the screen, a phone call), or why it could not play.
    pub fn play(
        &mut self,
        clip: AudioClip,
        on_finished: impl FnOnce(Result<(), AudioError>) -> M + Send + 'static,
    ) {
        self.push(
            ServiceRequest::Play(clip),
            Some(Box::new(move |result| {
                on_finished(match result {
                    ServiceResult::Played(result) => result,
                    _ => Err(AudioError::Failed(WRONG_KIND.to_owned())),
                })
            })),
        );
    }

    /// Stops whatever is playing. There is no message of its own: the
    /// clip's `play` message arrives with [`AudioError::Interrupted`].
    pub fn stop_playback(&mut self) {
        self.push(ServiceRequest::StopPlayback, None);
    }

    /// Keeps `value` under `key` between launches, in the app's private
    /// storage. For a few small values (an id, a setting), not for data.
    /// It is not encrypted beyond what the platform does for all of an
    /// app's files; do not put a password there.
    pub fn store(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.push(
            ServiceRequest::Store {
                key: key.into(),
                value: Some(value.into()),
            },
            None,
        );
    }

    /// Forgets the value kept under `key`.
    pub fn remove(&mut self, key: impl Into<String>) {
        self.push(
            ServiceRequest::Store {
                key: key.into(),
                value: None,
            },
            None,
        );
    }

    /// Reads the value kept under `key`: `None` if there is none. Requests
    /// are carried out in order, so a `load` after a `store` of the same
    /// key sees the new value. What was stored on earlier runs can be read
    /// without waiting, at setup: [`Setup::stored`](crate::Setup::stored).
    pub fn load(
        &mut self,
        key: impl Into<String>,
        on_loaded: impl FnOnce(Option<String>) -> M + Send + 'static,
    ) {
        self.push(
            ServiceRequest::Load { key: key.into() },
            Some(Box::new(move |result| {
                on_loaded(match result {
                    ServiceResult::Loaded(value) => value,
                    _ => None,
                })
            })),
        );
    }

    /// Puts plain text on the system clipboard, replacing what is there.
    /// (A [`TextInput`](crate::TextInput) copies and cuts by itself; this
    /// is for a "copy code" button.)
    pub fn set_clipboard(&mut self, text: impl Into<String>) {
        self.push(ServiceRequest::SetClipboard(text.into()), None);
    }

    /// Reads the system clipboard as plain text: `None` if it is empty or
    /// holds something that is not text. Ask only when the user has chosen
    /// to paste: on Android 12 and later the system shows them a notice
    /// every time an application reads what another one copied.
    pub fn clipboard(&mut self, on_text: impl FnOnce(Option<String>) -> M + Send + 'static) {
        self.push(
            ServiceRequest::ReadClipboard,
            Some(Box::new(move |result| {
                on_text(match result {
                    ServiceResult::Clipboard(text) => text,
                    _ => None,
                })
            })),
        );
    }

    /// Has the screen reader say `message`, when one is running: for
    /// something that happened and that nothing on the screen says by
    /// itself ("Report ready", "Message sent"). It waits for what is being
    /// said to finish. With no screen reader on, nothing happens. Prefer
    /// [`live`](crate::Container::live) on the widget when what changed is
    /// on the screen: this is for what is not.
    pub fn announce(&mut self, message: impl Into<String>) {
        self.announce_with(message, morningstar_core::Live::Polite);
    }

    /// As [`announce`](Self::announce), saying how urgent it is:
    /// [`Live::Assertive`](morningstar_core::Live::Assertive) interrupts
    /// what is being said, for an error or a call that dropped.
    /// `Live::Off` says nothing.
    pub fn announce_with(&mut self, message: impl Into<String>, live: morningstar_core::Live) {
        if live == morningstar_core::Live::Off {
            return;
        }
        self.push(
            ServiceRequest::Announce {
                message: message.into(),
                assertive: live == morningstar_core::Live::Assertive,
            },
            None,
        );
    }

    /// Joins a live room (ADR 0038): a conversation on a media server,
    /// where the people in it hear each other. `join` carries the server's
    /// address and the ticket the application's own server handed out,
    /// and says how the microphone and the loudspeaker start.
    ///
    /// Unlike every other request, this one is answered more than once:
    /// `on_event` makes a message from each [`LiveEvent`] of the room, in
    /// order, for as long as it lasts. The first is
    /// [`LiveEvent::Connected`], unless the room cannot be joined; the
    /// last is always exactly one [`LiveEvent::Disconnected`] or
    /// [`LiveEvent::Failed`], after which the microphone is closed and
    /// nothing more arrives.
    ///
    /// * The first time, the platform asks the user for the microphone,
    ///   also when joining muted. A refusal is
    ///   [`LiveError::PermissionDenied`].
    /// * There is one room at a time: joining another ends this one with
    ///   [`LiveDisconnect::Replaced`].
    /// * A room joined with [`LiveJoin::background`], in an app built with
    ///   the `live-background` service, goes on behind another app or a
    ///   locked screen under a notification, from which the user can end
    ///   it ([`LiveDisconnect::HungUp`]); see ADR 0042. Any other:
    /// * A room does not outlive the screen: when the app goes to the
    ///   background the host leaves it and the application is told
    ///   [`LiveDisconnect::Hidden`]. It may join again when it is back.
    /// * [`forget_pending`](Self::forget_pending) leaves the room as well
    ///   as dropping its events: a room nobody listens to is never left
    ///   connected.
    /// * Where nothing can join (no host, an app built without the live
    ///   service, iOS for now) the answer is [`LiveError::Unsupported`].
    ///
    /// ```ignore
    /// out.join_live(LiveJoin::new(ticket.url, ticket.token).speaker(true), Message::Live);
    /// // update: Message::Live(LiveEvent::Connected) => self.sound = Sound::On, ...
    /// ```
    pub fn join_live(
        &mut self,
        join: LiveJoin,
        on_event: impl FnMut(LiveEvent) -> M + Send + 'static,
    ) {
        self.entries.push(Entry::Join(join, Box::new(on_event)));
    }

    /// Leaves the live room. There is no message of its own: the room's
    /// events end with [`LiveDisconnect::Left`]. Nothing happens when no
    /// room is joined.
    pub fn leave_live(&mut self) {
        self.push(ServiceRequest::LiveLeave, None);
    }

    /// Opens (`true`) or mutes the microphone in the live room. Muted, the
    /// others hear nothing from this device. Nothing happens when no room
    /// is joined. If the platform cannot mute, it ends the room
    /// ([`LiveEvent::Failed`]) rather than leave the microphone open.
    pub fn set_live_microphone(&mut self, live: bool) {
        self.push(ServiceRequest::LiveMicrophone(live), None);
    }

    /// Sends the live room's sound to the loudspeaker (`true`) or the
    /// earpiece. A headset that is connected keeps the sound either way.
    /// Nothing happens when no room is joined.
    pub fn set_live_speaker(&mut self, on: bool) {
        self.push(ServiceRequest::LiveSpeaker(on), None);
    }

    /// Sends `payload` to everyone else in the live room, under `topic`
    /// (ADR 0039): they receive it as [`LiveEvent::Data`]. `reliable`
    /// messages arrive, and in order; the others are faster and may be
    /// lost, which suits positions and cursors, not words. There is no
    /// message for its having been sent. Nothing is sent when no room is
    /// joined, or when the payload is longer than
    /// [`MAX_LIVE_DATA_BYTES`](morningstar_core::MAX_LIVE_DATA_BYTES).
    ///
    /// ```ignore
    /// out.send_live_data("chat", text.as_bytes(), true);
    /// // update: Message::Live(LiveEvent::Data(data)) if data.topic == "chat" => ...
    /// ```
    pub fn send_live_data(
        &mut self,
        topic: impl Into<String>,
        payload: impl Into<Vec<u8>>,
        reliable: bool,
    ) {
        let request = ServiceRequest::LiveData {
            topic: topic.into(),
            payload: payload.into(),
            reliable,
        };
        self.push(request, None);
    }

    /// Turns this device's camera on or off in the live room (ADR 0039).
    /// The first time, the user is asked for the camera. What follows
    /// among the room's events: [`LiveEvent::VideoStarted`] with the
    /// track to draw the camera's own picture by
    /// ([`LiveVideo`](crate::LiveVideo)), [`LiveEvent::VideoStopped`], or
    /// [`LiveEvent::MediaFailed`] if the user refused or the app was
    /// built without `live-video`. The room goes on either way.
    pub fn set_live_camera(&mut self, on: bool) {
        self.push(ServiceRequest::LiveCamera(on), None);
    }

    /// Changes between the front and the back camera. Nothing happens
    /// while the camera is off.
    pub fn switch_live_camera(&mut self) {
        self.push(ServiceRequest::LiveSwitchCamera, None);
    }

    /// Starts or stops sharing this device's screen with the live room
    /// (ADR 0039). The system asks the user each time it is started, and
    /// shows that the screen is being shared for as long as it is. While
    /// it is shared the room is not left when the app leaves the screen
    /// (that is the point of sharing it), and the microphone stays as it
    /// was; the camera is turned off for as long as the app is hidden.
    /// What follows among the room's events is as for
    /// [`set_live_camera`](Self::set_live_camera), with
    /// [`LiveVideoSource::Screen`](morningstar_core::LiveVideoSource::Screen);
    /// an app built without `live-screen` is told
    /// [`LiveError::Unsupported`].
    pub fn set_live_screen_share(&mut self, on: bool) {
        self.push(ServiceRequest::LiveScreenShare(on), None);
    }

    /// Sends the live room's sound to `route` (ADR 0042): the earpiece,
    /// the loudspeaker, a wired headset or a Bluetooth one. There is no
    /// message of its own: [`LiveEvent::AudioRoutes`], which also arrives
    /// when the room connects and when a headset comes or goes, says which
    /// routes there are and which is in use. Nothing happens when the
    /// device has no such route now.
    ///
    /// ```ignore
    /// // update: Message::Live(LiveEvent::AudioRoutes(routes)) => self.routes = routes,
    /// out.set_live_audio_route(LiveAudioRoute::Bluetooth);
    /// ```
    pub fn set_live_audio_route(&mut self, route: LiveAudioRoute) {
        self.push(ServiceRequest::LiveAudioRoute(route), None);
    }

    /// Gives up on every request made before this call that has not been
    /// answered yet: their messages will never arrive. The platform may
    /// still finish the work (a request already sent reaches the server),
    /// but its result is dropped. For leaving a screen or signing out;
    /// playback is not stopped by this, [`stop_playback`](Self::stop_playback)
    /// does that. A live room is the exception: it is left, because its
    /// events are what would have said that it is still connected.
    pub fn forget_pending(&mut self) {
        self.entries.push(Entry::Forget);
    }

    /// Asks a scroll view to move: to its top or end, to an offset, or as
    /// far as brings a widget into view; at once or gliding. See
    /// [`ScrollTo`]. Unlike the other requests this one is not for the
    /// platform and has no answer: it is carried out in the next frame,
    /// with that frame's layout.
    pub fn scroll(&mut self, to: ScrollTo) {
        self.scrolls.push(to);
    }

    /// True if nothing has been asked for since the runtime last looked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.scrolls.is_empty()
    }
}

/// Who carries requests out.
enum Carrier {
    /// A real host, through the engine. `attached` once it has asked for
    /// requests; until then nobody is known to be there.
    Host { attached: bool },
    /// The test driver. Boxed: it is much the larger of the two, and a
    /// real application carries only the other.
    Driver(Box<Driver>),
}

/// Stands in for a host: keeps stored values in memory, lets a closure
/// answer what it wants to, and holds everything else for the test.
struct Driver {
    fake: FakeHost,
    /// Requests waiting for the test to answer them, oldest first.
    held: Vec<ServiceCall>,
    stored: HashMap<String, String>,
    /// The live room that is joined, and the id of its join.
    live: Option<(ServiceId, LiveRoom)>,
}

/// Everything [`Ui`](crate::Ui) keeps about effects.
pub(crate) struct EffectState<M> {
    /// Reused for every call to `Application::effects`.
    scratch: Effects<M>,
    next_id: u64,
    /// The requests still waiting for an answer.
    handlers: HashMap<u64, Handler<M>>,
    /// The live rooms whose events are still wanted, by the id of their
    /// join. One, except between joining a second room and hearing that
    /// the first has ended.
    rooms: HashMap<u64, LiveHandler<M>>,
    /// Requests made and not yet taken by a host.
    outbox: Vec<ServiceCall>,
    /// Answers that are known and not yet delivered: they are delivered at
    /// the start of the next frame.
    ready: VecDeque<(ServiceId, ServiceResult)>,
    carrier: Carrier,
    /// The clipboard where there is no platform to keep one: under the
    /// test driver, and with no host at all (a screenshot tool), so that
    /// what a field copies can be pasted again.
    clipboard: Option<String>,
    /// What the application asked a screen reader to say, under the test
    /// driver, oldest first.
    announced: Vec<String>,
}

impl<M> EffectState<M> {
    pub(crate) fn new() -> Self {
        Self {
            scratch: Effects::new(),
            next_id: 0,
            handlers: HashMap::new(),
            rooms: HashMap::new(),
            outbox: Vec::new(),
            ready: VecDeque::new(),
            carrier: Carrier::Host { attached: false },
            clipboard: None,
            announced: Vec::new(),
        }
    }

    pub(crate) fn with_driver(fake: FakeHost) -> Self {
        Self {
            carrier: Carrier::Driver(Box::new(Driver {
                fake,
                held: Vec::new(),
                stored: HashMap::new(),
                live: None,
            })),
            ..Self::new()
        }
    }

    /// The collector to hand to `Application::effects`.
    pub(crate) fn scratch(&mut self) -> &mut Effects<M> {
        &mut self.scratch
    }

    /// Takes in what the application just asked for: numbers each request,
    /// remembers its handler, and passes it on to whoever carries requests
    /// out. Requests to scroll are for the UI layer itself and go to
    /// `scrolls`.
    pub(crate) fn collect(&mut self, scrolls: &mut Vec<ScrollTo>) {
        scrolls.append(&mut self.scratch.scrolls);
        if self.scratch.entries.is_empty() {
            return;
        }
        let mut entries = std::mem::take(&mut self.scratch.entries);
        for entry in entries.drain(..) {
            match entry {
                Entry::Forget => self.forget(),
                Entry::Join(join, handler) => {
                    self.next_id += 1;
                    let id = ServiceId(self.next_id);
                    self.rooms.insert(id.0, handler);
                    self.dispatch(ServiceCall {
                        id,
                        request: ServiceRequest::LiveJoin(join),
                    });
                }
                Entry::Request(request, handler) => {
                    self.next_id += 1;
                    let id = ServiceId(self.next_id);
                    if let Some(handler) = handler {
                        self.handlers.insert(id.0, handler);
                    }
                    self.dispatch(ServiceCall { id, request });
                }
            }
        }
        // Give the allocation back.
        self.scratch.entries = entries;
    }

    /// A request the UI layer makes for itself (a field's copy or paste):
    /// numbered and carried out like the application's, but no handler of
    /// the application's waits for it. The caller remembers the id.
    pub(crate) fn request(&mut self, request: ServiceRequest) -> ServiceId {
        self.next_id += 1;
        let id = ServiceId(self.next_id);
        self.dispatch(ServiceCall { id, request });
        id
    }

    /// An answer to a request made with [`request`](Self::request) has
    /// been taken: nothing else is to be delivered for it.
    pub(crate) fn settle(&mut self, id: ServiceId) {
        if let Carrier::Driver(driver) = &mut self.carrier {
            driver.held.retain(|call| call.id != id);
        }
        self.ready.retain(|(ready, _)| *ready != id);
    }

    /// What was announced under the test driver.
    pub(crate) fn announced(&self) -> &[String] {
        &self.announced
    }

    pub(crate) fn clipboard(&self) -> Option<&str> {
        self.clipboard.as_deref()
    }

    pub(crate) fn set_clipboard(&mut self, text: Option<String>) {
        self.clipboard = text;
    }

    fn forget(&mut self) {
        self.handlers.clear();
        self.ready.clear();
        // Requests nobody waits for any more need not be made at all; the
        // ones that are not answered anyway (a store) still must be.
        self.outbox.retain(|call| !call.request.expects_result());
        if let Carrier::Driver(driver) = &mut self.carrier {
            driver.held.clear();
        }
        // A live room is not a request that merely finishes unheard: it
        // would stay connected, microphone open, with nobody listening
        // for its end. So giving up on it leaves it. (If its join never
        // reached the host it was dropped just above and this leaves
        // nothing, which a host ignores.)
        if !self.rooms.is_empty() {
            self.rooms.clear();
            self.next_id += 1;
            self.dispatch(ServiceCall {
                id: ServiceId(self.next_id),
                request: ServiceRequest::LiveLeave,
            });
        }
    }

    fn dispatch(&mut self, call: ServiceCall) {
        let Carrier::Driver(driver) = &mut self.carrier else {
            self.outbox.push(call);
            return;
        };
        match &call.request {
            ServiceRequest::Store { key, value } => {
                match value {
                    Some(value) => driver.stored.insert(key.clone(), value.clone()),
                    None => driver.stored.remove(key),
                };
            }
            ServiceRequest::Load { key } => {
                let value = driver.stored.get(key).cloned();
                self.ready
                    .push_back((call.id, ServiceResult::Loaded(value)));
            }
            ServiceRequest::SetClipboard(text) => self.clipboard = Some(text.clone()),
            ServiceRequest::Announce { message, .. } => self.announced.push(message.clone()),
            ServiceRequest::ReadClipboard => {
                let text = self.clipboard.clone().filter(|text| !text.is_empty());
                self.ready
                    .push_back((call.id, ServiceResult::Clipboard(text)));
            }
            ServiceRequest::StopPlayback => interrupt_playback(driver, &mut self.ready),
            // The live room behaves as on a device: one at a time, and
            // leaving it is what ends its events.
            ServiceRequest::LiveLeave => {
                end_live(driver, &mut self.ready, LiveDisconnect::Left);
            }
            ServiceRequest::LiveMicrophone(live) => {
                if let Some((_, room)) = &mut driver.live {
                    room.microphone = *live;
                }
            }
            ServiceRequest::LiveSpeaker(on) => {
                if let Some((_, room)) = &mut driver.live {
                    room.speaker = *on;
                }
            }
            ServiceRequest::LiveCamera(on) => {
                if let Some((_, room)) = &mut driver.live {
                    room.camera = *on;
                }
            }
            ServiceRequest::LiveSwitchCamera => {
                if let Some((_, room)) = &mut driver.live {
                    room.camera_switches += 1;
                }
            }
            ServiceRequest::LiveScreenShare(on) => {
                if let Some((_, room)) = &mut driver.live {
                    room.screen_share = *on;
                }
            }
            ServiceRequest::LiveAudioRoute(route) => {
                if let Some((_, room)) = &mut driver.live {
                    room.audio_route = Some(*route);
                }
            }
            ServiceRequest::LiveData { topic, payload, .. } => {
                if let Some((_, room)) = &mut driver.live
                    && payload.len() <= morningstar_core::MAX_LIVE_DATA_BYTES
                {
                    room.sent.push((topic.clone(), payload.clone()));
                }
            }
            ServiceRequest::LiveJoin(join) => {
                end_live(driver, &mut self.ready, LiveDisconnect::Replaced);
                driver.live = Some((
                    call.id,
                    LiveRoom {
                        microphone: join.microphone,
                        speaker: join.speaker,
                        connected: false,
                        camera: join.camera,
                        camera_switches: 0,
                        screen_share: false,
                        sent: Vec::new(),
                        audio_route: None,
                        join: join.clone(),
                    },
                ));
                match (driver.fake)(&call.request) {
                    Some(result) => self.ready.push_back((call.id, result)),
                    None => driver.held.push(call),
                }
            }
            request => {
                if matches!(request, ServiceRequest::Play(_)) {
                    // As on a device: a new clip replaces the one playing.
                    interrupt_playback(driver, &mut self.ready);
                }
                match (driver.fake)(request) {
                    Some(result) => self.ready.push_back((call.id, result)),
                    None => driver.held.push(call),
                }
            }
        }
    }

    /// A host asks for the requests made since it last asked.
    pub(crate) fn take_for_host(&mut self, out: &mut Vec<ServiceCall>) {
        if let Carrier::Host { attached } = &mut self.carrier {
            *attached = true;
            out.append(&mut self.outbox);
        }
    }

    /// With no host and no driver, nothing will ever carry the waiting
    /// requests out: answer them "unsupported" so the application hears.
    pub(crate) fn fail_unattached(&mut self) {
        if !matches!(self.carrier, Carrier::Host { attached: false }) {
            return;
        }
        for call in self.outbox.drain(..) {
            match call.request {
                // Copy and paste still work inside the application.
                ServiceRequest::SetClipboard(text) => self.clipboard = Some(text),
                ServiceRequest::ReadClipboard => {
                    let text = self.clipboard.clone().filter(|text| !text.is_empty());
                    self.ready
                        .push_back((call.id, ServiceResult::Clipboard(text)));
                }
                request => {
                    if let Some(result) = request.unsupported() {
                        self.ready.push_back((call.id, result));
                    }
                }
            }
        }
    }

    /// The next answer that is ready to be delivered.
    pub(crate) fn next_ready(&mut self) -> Option<(ServiceId, ServiceResult)> {
        self.ready.pop_front()
    }

    pub(crate) fn ready_len(&self) -> usize {
        self.ready.len()
    }

    /// True if answers are waiting for the next frame to deliver them.
    pub(crate) fn has_ready(&self) -> bool {
        !self.ready.is_empty()
            || (matches!(self.carrier, Carrier::Host { attached: false })
                && self.outbox.iter().any(|call| call.request.expects_result()))
    }

    /// The message the answer to `id` becomes, if the application is still
    /// waiting for it. The request is then answered: a second answer finds
    /// nothing.
    pub(crate) fn message_for(&mut self, id: ServiceId, result: ServiceResult) -> Option<M> {
        if let Carrier::Driver(driver) = &mut self.carrier {
            driver.held.retain(|call| call.id != id);
        }
        if self.rooms.contains_key(&id.0) {
            return self.live_message(id, result);
        }
        self.ready.retain(|(ready, _)| *ready != id);
        let handler = self.handlers.remove(&id.0)?;
        Some(handler(result))
    }

    /// The message one event of the live room joined under `id` becomes.
    /// The event that ends the room is the last: its handler is dropped
    /// with it, so nothing that arrives afterwards under that id reaches
    /// the application.
    fn live_message(&mut self, id: ServiceId, result: ServiceResult) -> Option<M> {
        let event = match result {
            ServiceResult::Live(event) => event,
            // A host that answers a join with something else has not
            // joined anything the application could ever hear the end of.
            _ => LiveEvent::Failed(LiveError::Failed(WRONG_KIND.to_owned())),
        };
        if let Carrier::Driver(driver) = &mut self.carrier
            && let Some((joined, room)) = &mut driver.live
            && *joined == id
        {
            if event.is_final() {
                driver.live = None;
            } else if event == LiveEvent::Connected {
                room.connected = true;
            }
        }
        if event.is_final() {
            // Other events of this room that were waiting for a frame are
            // not delivered after its end.
            self.ready.retain(|(ready, _)| *ready != id);
            let mut handler = self.rooms.remove(&id.0)?;
            Some(handler(event))
        } else {
            self.rooms.get_mut(&id.0).map(|handler| handler(event))
        }
    }

    /// The live room joined under the test driver, and the id of its join.
    pub(crate) fn live(&self) -> Option<(ServiceId, &LiveRoom)> {
        match &self.carrier {
            Carrier::Driver(driver) => driver.live.as_ref().map(|(id, room)| (*id, room)),
            Carrier::Host { .. } => None,
        }
    }

    pub(crate) fn waiting(&self) -> usize {
        self.handlers.len() + self.rooms.len()
    }

    pub(crate) fn held(&self) -> &[ServiceCall] {
        match &self.carrier {
            Carrier::Driver(driver) => &driver.held,
            Carrier::Host { .. } => &[],
        }
    }

    /// The id of the oldest held request that `wanted` accepts.
    pub(crate) fn oldest_held(
        &self,
        wanted: impl Fn(&ServiceRequest) -> bool,
    ) -> Option<ServiceId> {
        self.held()
            .iter()
            .find(|call| wanted(&call.request))
            .map(|call| call.id)
    }

    pub(crate) fn stored(&self, key: &str) -> Option<&str> {
        match &self.carrier {
            Carrier::Driver(driver) => driver.stored.get(key).map(String::as_str),
            Carrier::Host { .. } => None,
        }
    }

    /// Returns false when there is no driver to keep the value.
    pub(crate) fn set_stored(&mut self, key: String, value: String) -> bool {
        match &mut self.carrier {
            Carrier::Driver(driver) => {
                driver.stored.insert(key, value);
                true
            }
            Carrier::Host { .. } => false,
        }
    }
}

/// What a host does to the clip that is playing when another starts or the
/// application stops it.
fn interrupt_playback(driver: &mut Driver, ready: &mut VecDeque<(ServiceId, ServiceResult)>) {
    driver.held.retain(|call| {
        let playing = matches!(call.request, ServiceRequest::Play(_));
        if playing {
            ready.push_back((call.id, ServiceResult::Played(Err(AudioError::Interrupted))));
        }
        !playing
    });
}

/// What a host does to the room that is joined when the application leaves
/// it or joins another: its events end, with the reason.
fn end_live(
    driver: &mut Driver,
    ready: &mut VecDeque<(ServiceId, ServiceResult)>,
    reason: LiveDisconnect,
) {
    if let Some((id, _)) = driver.live.take() {
        driver.held.retain(|call| call.id != id);
        ready.push_back((id, ServiceResult::Live(LiveEvent::Disconnected(reason))));
    }
}

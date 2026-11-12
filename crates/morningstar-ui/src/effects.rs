//! [`Effects`]: how an [`Application`](crate::Application) starts work
//! whose result comes back later as a message, and the bookkeeping [`Ui`]
//! keeps for it: ids, the handlers waiting for answers, and the driver that
//! stands in for a host in tests.
//!
//! [`Ui`]: crate::Ui

use std::collections::{HashMap, VecDeque};

use morningstar_core::{
    AudioClip, AudioError, HttpError, HttpRequest, HttpResponse, Recording, ServiceCall, ServiceId,
    ServiceRequest, ServiceResult,
};

/// Turns the platform's answer into the application's message.
type Handler<M> = Box<dyn FnOnce(ServiceResult) -> M + Send>;

/// What a stand-in for the host answers to each request: `Some` to answer,
/// `None` to leave it pending for the test to answer by hand.
pub(crate) type FakeHost = Box<dyn FnMut(&ServiceRequest) -> Option<ServiceResult> + Send>;

const WRONG_KIND: &str = "the host answered with a result of the wrong kind";

enum Entry<M> {
    Request(ServiceRequest, Option<Handler<M>>),
    Forget,
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
}

impl<M> Effects<M> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
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

    /// Gives up on every request made before this call that has not been
    /// answered yet: their messages will never arrive. The platform may
    /// still finish the work (a request already sent reaches the server),
    /// but its result is dropped. For leaving a screen or signing out;
    /// playback is not stopped by this, [`stop_playback`](Self::stop_playback)
    /// does that.
    pub fn forget_pending(&mut self) {
        self.entries.push(Entry::Forget);
    }

    /// True if nothing has been asked for since the runtime last looked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Who carries requests out.
enum Carrier {
    /// A real host, through the engine. `attached` once it has asked for
    /// requests; until then nobody is known to be there.
    Host { attached: bool },
    /// The test driver.
    Driver(Driver),
}

/// Stands in for a host: keeps stored values in memory, lets a closure
/// answer what it wants to, and holds everything else for the test.
struct Driver {
    fake: FakeHost,
    /// Requests waiting for the test to answer them, oldest first.
    held: Vec<ServiceCall>,
    stored: HashMap<String, String>,
}

/// Everything [`Ui`](crate::Ui) keeps about effects.
pub(crate) struct EffectState<M> {
    /// Reused for every call to `Application::effects`.
    scratch: Effects<M>,
    next_id: u64,
    /// The requests still waiting for an answer.
    handlers: HashMap<u64, Handler<M>>,
    /// Requests made and not yet taken by a host.
    outbox: Vec<ServiceCall>,
    /// Answers that are known and not yet delivered: they are delivered at
    /// the start of the next frame.
    ready: VecDeque<(ServiceId, ServiceResult)>,
    carrier: Carrier,
}

impl<M> EffectState<M> {
    pub(crate) fn new() -> Self {
        Self {
            scratch: Effects::new(),
            next_id: 0,
            handlers: HashMap::new(),
            outbox: Vec::new(),
            ready: VecDeque::new(),
            carrier: Carrier::Host { attached: false },
        }
    }

    pub(crate) fn with_driver(fake: FakeHost) -> Self {
        Self {
            carrier: Carrier::Driver(Driver {
                fake,
                held: Vec::new(),
                stored: HashMap::new(),
            }),
            ..Self::new()
        }
    }

    /// The collector to hand to `Application::effects`.
    pub(crate) fn scratch(&mut self) -> &mut Effects<M> {
        &mut self.scratch
    }

    /// Takes in what the application just asked for: numbers each request,
    /// remembers its handler, and passes it on to whoever carries requests
    /// out.
    pub(crate) fn collect(&mut self) {
        if self.scratch.entries.is_empty() {
            return;
        }
        let mut entries = std::mem::take(&mut self.scratch.entries);
        for entry in entries.drain(..) {
            match entry {
                Entry::Forget => self.forget(),
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

    fn forget(&mut self) {
        self.handlers.clear();
        self.ready.clear();
        // Requests nobody waits for any more need not be made at all; the
        // ones that are not answered anyway (a store) still must be.
        self.outbox.retain(|call| !call.request.expects_result());
        if let Carrier::Driver(driver) = &mut self.carrier {
            driver.held.clear();
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
            ServiceRequest::StopPlayback => interrupt_playback(driver, &mut self.ready),
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
            if let Some(result) = call.request.unsupported() {
                self.ready.push_back((call.id, result));
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
        self.ready.retain(|(ready, _)| *ready != id);
        let handler = self.handlers.remove(&id.0)?;
        Some(handler(result))
    }

    pub(crate) fn waiting(&self) -> usize {
        self.handlers.len()
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

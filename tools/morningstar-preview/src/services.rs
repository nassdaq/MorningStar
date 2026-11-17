//! The platform services of the preview (ADR 0019): what a Mac can do for
//! an app, done; everything else answered `Unsupported` at once, and said
//! so in the terminal the first time.
//!
//! | Request | Here |
//! | --- | --- |
//! | HTTP | Really made, through the system's `curl` ([`crate::http`]). |
//! | Store, load | A file in the app's state directory ([`crate::store`]). |
//! | Clipboard | The Mac's, through `pbcopy` and `pbpaste`. |
//! | Recording, playback | `Unsupported`. |
//! | Live rooms | `Unsupported`: the join fails as on a host without the service. |
//! | Announce | Dropped: the preview runs no screen reader. |
//!
//! Work that takes time runs on a thread of its own and hands its answer
//! to [`Answer`], which the host turns into an event on its own thread.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

use morningstar_runtime::{ServiceCall, ServiceId, ServiceRequest, ServiceResult};

use crate::http;
use crate::store::Store;

/// Where an answer goes: called from any thread.
pub type Answer = Arc<dyn Fn(ServiceId, ServiceResult) + Send + Sync>;

/// Carries out an app's requests.
pub struct Services {
    store: Store,
    answer: Answer,
    scratch: PathBuf,
    /// The kinds of request already reported as not available here.
    reported: HashSet<&'static str>,
}

impl Services {
    /// Services that keep their values in `store` and answer through
    /// `answer`.
    pub fn new(store: Store, answer: Answer) -> Self {
        let scratch = if store.dir().as_os_str().is_empty() {
            std::env::temp_dir().join(format!("morningstar-preview-{}", std::process::id()))
        } else {
            store.dir().join("tmp")
        };
        Self {
            store,
            answer,
            scratch,
            reported: HashSet::new(),
        }
    }

    /// The store, for the host's own use (setup, the window's place).
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The store, to clear it.
    pub fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// Carries out one request. Returns at once.
    pub fn handle(&mut self, call: ServiceCall) {
        let ServiceCall { id, request } = call;
        match request {
            ServiceRequest::Http(request) => {
                let answer = Arc::clone(&self.answer);
                let scratch = self.scratch.clone();
                spawn("http", move || {
                    answer(id, ServiceResult::Http(http::send(&request, &scratch)));
                });
            }
            ServiceRequest::Store { key, value } => {
                if let Err(error) = self.store.set(key, value) {
                    eprintln!("preview: a value could not be stored: {error}");
                }
            }
            ServiceRequest::Load { key } => {
                let value = self.store.get(&key).map(str::to_owned);
                (self.answer)(id, ServiceResult::Loaded(value));
            }
            ServiceRequest::SetClipboard(text) => {
                spawn("clipboard", move || {
                    if let Err(error) = set_clipboard(&text) {
                        eprintln!("preview: the clipboard could not be set: {error}");
                    }
                });
            }
            ServiceRequest::ReadClipboard => {
                let answer = Arc::clone(&self.answer);
                spawn("clipboard", move || {
                    answer(id, ServiceResult::Clipboard(read_clipboard()));
                });
            }
            // Said to a screen reader, and the preview runs none.
            ServiceRequest::Announce { .. } => {}
            other => {
                if self.reported.insert(other.name()) {
                    eprintln!(
                        "preview: \"{}\" is not available in the preview; the app is told so \
                         (Unsupported). Test it on a phone.",
                        other.name()
                    );
                }
                if let Some(result) = other.unsupported() {
                    (self.answer)(id, result);
                }
            }
        }
    }
}

fn spawn(name: &str, work: impl FnOnce() + Send + 'static) {
    let built = thread::Builder::new()
        .name(format!("preview {name}"))
        .spawn(work);
    if let Err(error) = built {
        eprintln!("preview: a thread for {name} could not be started: {error}");
    }
}

fn set_clipboard(text: &str) -> std::io::Result<()> {
    let mut child = Command::new("pbcopy").stdin(Stdio::piped()).spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait().map(drop)
}

fn read_clipboard() -> Option<String> {
    let mut child = Command::new("pbpaste")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut text = String::new();
    // Not valid UTF-8, or far too much: no text, as the service says.
    let read = child
        .stdout
        .take()?
        .take(4 * 1024 * 1024)
        .read_to_string(&mut text);
    let _ = child.wait();
    read.ok().filter(|_| !text.is_empty()).map(|_| text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use morningstar_runtime::{AudioError, HttpError, HttpRequest, LiveError, LiveEvent, LiveJoin};
    use std::sync::Mutex;
    use std::sync::mpsc;
    use std::time::Duration;

    fn services() -> (Services, mpsc::Receiver<(ServiceId, ServiceResult)>) {
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let answer: Answer = Arc::new(move |id, result| {
            let _ = sender.lock().unwrap().send((id, result));
        });
        (Services::new(Store::in_memory(), answer), receiver)
    }

    fn call(id: u64, request: ServiceRequest) -> ServiceCall {
        ServiceCall {
            id: ServiceId(id),
            request,
        }
    }

    #[test]
    fn a_stored_value_is_loaded_again() {
        let (mut services, answers) = services();
        services.handle(call(
            1,
            ServiceRequest::Store {
                key: "name".into(),
                value: Some("Neema".into()),
            },
        ));
        services.handle(call(2, ServiceRequest::Load { key: "name".into() }));
        services.handle(call(
            3,
            ServiceRequest::Load {
                key: "other".into(),
            },
        ));
        assert_eq!(
            answers.try_recv().unwrap(),
            (ServiceId(2), ServiceResult::Loaded(Some("Neema".into())))
        );
        assert_eq!(
            answers.try_recv().unwrap(),
            (ServiceId(3), ServiceResult::Loaded(None))
        );
        assert!(answers.try_recv().is_err(), "storing is not answered");
        assert_eq!(services.store().get("name"), Some("Neema"));
    }

    #[test]
    fn what_a_mac_cannot_do_is_answered_unsupported_at_once() {
        let (mut services, answers) = services();
        services.handle(call(1, ServiceRequest::StartRecording));
        services.handle(call(2, ServiceRequest::StopRecording));
        services.handle(call(
            3,
            ServiceRequest::LiveJoin(LiveJoin::new("wss://example.invalid", "ticket")),
        ));
        assert_eq!(
            answers.try_recv().unwrap(),
            (
                ServiceId(1),
                ServiceResult::RecordingStarted(Err(AudioError::Unsupported))
            )
        );
        assert_eq!(
            answers.try_recv().unwrap(),
            (
                ServiceId(2),
                ServiceResult::Recording(Err(AudioError::Unsupported))
            )
        );
        assert_eq!(
            answers.try_recv().unwrap(),
            (
                ServiceId(3),
                ServiceResult::Live(LiveEvent::Failed(LiveError::Unsupported))
            )
        );
    }

    #[test]
    fn requests_that_expect_no_answer_get_none() {
        let (mut services, answers) = services();
        services.handle(call(1, ServiceRequest::StopPlayback));
        services.handle(call(2, ServiceRequest::LiveLeave));
        services.handle(call(3, ServiceRequest::LiveMicrophone(false)));
        services.handle(call(
            4,
            ServiceRequest::Announce {
                message: "Report ready".into(),
                assertive: false,
            },
        ));
        assert!(answers.try_recv().is_err());
    }

    #[test]
    fn a_request_to_no_url_is_answered_from_its_thread() {
        let (mut services, answers) = services();
        services.handle(call(9, ServiceRequest::Http(HttpRequest::get("not a url"))));
        let (id, result) = answers.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(id, ServiceId(9));
        assert_eq!(result, ServiceResult::Http(Err(HttpError::InvalidUrl)));
    }
}

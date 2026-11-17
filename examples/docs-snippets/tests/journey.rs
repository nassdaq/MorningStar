//! The samples of the testing page, and the proof that the samples with
//! behaviour behave.

use docs_snippets::effects::{LAST_ORDER, Message, Orders, SERVER};
use docs_snippets::{counter, form, navigation};
use morningstar::ui::Ui;
use morningstar::{HostState, HttpError, HttpResponse, ServiceRequest};

// docs:begin http
#[test]
fn the_server_is_asked_once_and_its_answer_is_kept() {
    // `Ui::testing` holds every request instead of sending it.
    let mut ui = Ui::testing(Orders::default());
    ui.send(Message::Refresh);
    assert!(ui.app().loading);

    // What the app asked for, as data.
    let pending = ui.pending_effects();
    assert_eq!(pending.len(), 1);
    let ServiceRequest::Http(request) = &pending[0].request else {
        panic!("expected a request");
    };
    assert_eq!(request.url, format!("{SERVER}/v1/orders/latest"));

    // The test is the server: "it replies this".
    assert!(ui.answer_http(Ok(HttpResponse::new(200).with_body("Order 1042"))));
    assert!(!ui.app().loading);
    assert_eq!(ui.app().status, "Order 1042");
    assert_eq!(
        ui.stored(LAST_ORDER),
        Some("Order 1042"),
        "kept on the phone"
    );
}

#[test]
fn no_connection_is_a_sentence_not_a_crash() {
    let mut ui = Ui::testing(Orders::default());
    ui.send(Message::Refresh);
    assert!(ui.answer_http(Err(HttpError::Offline)));
    assert_eq!(ui.app().status, "No connection.");
    assert_eq!(ui.stored(LAST_ORDER), None);
}
// docs:end http

// docs:begin host
#[test]
fn a_value_stored_on_an_earlier_run_is_there_at_setup() {
    // The phone as the app finds it: an install id and stored values.
    let host = HostState::new().with_stored(LAST_ORDER, "Order 1041");
    let mut ui = Ui::testing(Orders::default());
    let _frame = morningstar::Frame::for_app_with_host(&mut ui, &host);
    assert_eq!(ui.app().status, "Last time: Order 1041");
}
// docs:end host

#[test]
fn the_counter_counts() {
    let mut ui = Ui::testing(counter::Counter::default());
    ui.send(counter::Message::Tapped);
    ui.send(counter::Message::Tapped);
    assert_eq!(ui.app().count, 2);
}

#[test]
fn the_form_keeps_what_is_typed() {
    let mut ui = Ui::testing(form::SignIn::default());
    ui.send(form::Message::Phone("0712345678".to_owned()));
    ui.send(form::Message::Password("siri".to_owned()));
    ui.send(form::Message::Submit);
    assert_eq!(ui.app().phone, "0712345678");
    assert!(ui.app().submitted);
}

#[test]
fn back_closes_an_order_and_then_is_the_systems() {
    let mut ui = Ui::testing(navigation::Shop::default());
    ui.send(navigation::Message::Open(1042));
    assert_eq!(*ui.app().screens.current(), navigation::Screen::Order(1042));
    assert!(ui.back(), "the app closes the order");
    assert_eq!(*ui.app().screens.current(), navigation::Screen::Orders);
    assert!(!ui.back(), "on the first screen Back is the system's");
}

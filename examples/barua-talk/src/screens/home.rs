//! Screen 01: home. `design/html/01-home.html`.
//!
//! The user's picture and name with two round buttons, the rings of the
//! live rooms, three tabs over the list of conversations and calls, and
//! the bar at the bottom. The bar stays put; everything above it scrolls
//! on a phone shorter than the design's.

use morningstar::ui::prelude::*;

use crate::components::{
    MARGIN, TOP, count, face, field, list_row, outlined_icon_button, page, sheet, tab_bar,
};
use crate::icons::{self, icon};
use crate::model::{Filter, Room};
use crate::theme::{
    ACCENT, INK, LINE, MUTED, PAGE, QUIET, SOFT, SURFACE, grotesk, inter, inter_baseline_drop,
};
use crate::{BaruaTalk, Link, Message, Screen, Tab};

/// Whether the server answers, on the user's picture: a 12 dp dot with a
/// ring in the page colour at its lower right, the accent when rooms and
/// calls are real and the quiet grey when the server is not answering.
/// **The app's own.** It is there only when there is a server to be
/// connected to or cut off from: with nothing behind the app, home is the
/// design's.
fn link_dot(link: &Link) -> Option<Container<Message>> {
    let color = match link {
        Link::Online { .. } => ACCENT,
        Link::Offline(_) => QUIET,
        Link::Standalone | Link::Checking => return None,
    };
    Some(
        Container::new(
            Container::empty()
                .size(8.0, 8.0)
                .radius(4.0)
                .background(color),
        )
        .center()
        .size(12.0, 12.0)
        .radius(6.0)
        .background(PAGE)
        .place(Alignment::BOTTOM_RIGHT),
    )
}

/// `display: flex; align-items: center; gap: 12px`: the 48 dp picture,
/// the greeting (`font-size: 12px; color: #9AA1B2`) over the name
/// (`font-family: 'Space Grotesk'; font-size: 20px; font-weight: 600`,
/// `gap: 2px`), and the two 44 dp outlined buttons.
fn header(app: &BaruaTalk) -> Container<Message> {
    let link = app.link();
    let label = match link {
        Link::Online { .. } => "Your name and the server. Connected",
        Link::Offline(_) => "Your name and the server. The server is not answering",
        _ => "Your name and the server",
    };
    let me = Stack::new()
        .push(face(app, &app.my_face(), 48.0))
        .push_maybe(link_dot(link))
        .size(48.0, 48.0)
        .on_press(Message::OpenServer)
        .label(label)
        .id("me");
    row![
        me,
        col![
            inter(app.greeting(), 12.0).color(MUTED),
            grotesk(app.name().to_owned(), 20.0)
                .max_lines(1)
                .fill_width()
                .heading(),
        ]
        .gap(2.0)
        .fill_width(),
        outlined_icon_button(
            &icons::SEARCH,
            if app.search().is_some() {
                "Close search"
            } else {
                "Search"
            },
            Message::ToggleSearch
        )
        .id("search"),
        outlined_icon_button(&icons::DIAL, "Dial a phone number", Message::OpenDial).id("dial"),
    ]
    .gap(12.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .padding_ltrb(MARGIN, TOP, MARGIN, 0.0)
}

/// A ring of "Live rooms": `display: flex; flex-direction: column;
/// align-items: center; gap: 8px; width: 60px` with the label in
/// `font-size: 12px; color: #C9CEDA`.
fn ring(disc: Container<Message>, label: String) -> Container<Message> {
    col![disc, inter(label, 12.0).color(SOFT).max_lines(1),]
        .gap(8.0)
        .cross_align(CrossAlign::Center)
        .width(60.0)
}

/// "New room": a `button` with `width: 60px; height: 60px; border-radius:
/// 50%; border: 1.5px dashed #4A5166; background: #171A24` and a 22 dp
/// plus.
fn new_room() -> Container<Message> {
    ring(
        Container::new(icon(&icons::PLUS, 22.0, INK))
            .center()
            .size(60.0, 60.0)
            .radius(30.0)
            .background(SURFACE)
            .border_dashed(1.5, QUIET),
        "New room".to_owned(),
    )
    .on_press(Message::OpenNewRoom)
    .label("Create a room")
    .id("new-room")
}

/// A room's ring: an `img` with `width: 52px; height: 52px; border-radius:
/// 50%; padding: 2px; border: 2px solid {{accent}}`, 60 dp in all; the
/// border is `#4A5166` for a room where nothing is happening.
fn room_ring(app: &BaruaTalk, room: &Room) -> Container<Message> {
    let color = if room.live { ACCENT } else { QUIET };
    ring(
        Container::new(face(app, &room.face, 52.0))
            .center()
            .size(60.0, 60.0)
            .radius(30.0)
            .border(2.0, color),
        room.short.clone(),
    )
    .on_press(Message::OpenRoom(room.name.clone()))
    .label(format!(
        "{}, {}",
        room.title,
        if room.live { "live" } else { "quiet" }
    ))
    .id(("ring", room.name.clone()))
}

/// The three tabs:
///
/// ```css
/// display: flex; gap: 4px; padding: 4px; background: #171A24; border-radius: 999px
/// /* each */ flex: 1; height: 44px; border-radius: 999px; font-size: 14px;
/// /* chosen */ background: #0D0F15; color: #F4F5F8; font-weight: 600; gap: 8px
/// /* others */ background: transparent; color: #C9CEDA; font-weight: 500
/// ```
fn tabs(app: &BaruaTalk) -> Container<Message> {
    let unread = app.unread();
    let tab = |filter: Filter| {
        let chosen = app.filter() == filter;
        let label = inter(filter.title(), 14.0);
        let label = if chosen {
            label.semibold()
        } else {
            label.medium().color(SOFT)
        };
        let mut tab = Row::new()
            .push(label)
            .push_maybe((filter == Filter::All && chosen && unread > 0).then(|| count(unread)))
            .gap(8.0)
            .center()
            .fill_width()
            .height(44.0)
            .radius(22.0)
            .on_press(Message::SetFilter(filter))
            .role(Role::Tab)
            .selected(chosen)
            .id(("filter", filter));
        if chosen {
            tab = tab.background(PAGE);
        }
        if filter == Filter::All && unread > 0 {
            tab = tab.label(format!("All, {unread} unread"));
        }
        tab
    };
    Row::new()
        .extend(Filter::ALL.map(tab))
        .gap(4.0)
        .fill_width()
        .padding(4.0)
        .radius(26.0)
        .background(SURFACE)
}

/// Home's search, where the tabs are while it is open: the room's message
/// field with a cross that closes it. **The app's own.**
fn search(query: &str) -> Container<Message> {
    Container::new(
        field(
            TextInput::new(query)
                .placeholder("Search rooms and calls")
                .action(ImeAction::Search)
                .no_autocorrect()
                .on_input(Message::Query)
                .semantic_label("Search rooms and calls")
                .id("query"),
        )
        .height(52.0)
        .radius(26.0)
        .input_frame()
        .id("query-field"),
    )
    .fill_width()
}

/// `padding: 56px 24px 0`, a column, with the bar over its bottom 96 dp.
pub fn view(app: &BaruaTalk) -> Element<Message> {
    // `margin-top: 28px; display: flex; align-items: baseline;
    // justify-content: space-between`: the two lines share a baseline.
    let heading = row![
        inter("Live rooms", 14.0).color(MUTED).heading(),
        Container::new(inter("Free, however long", 12.0).color(MUTED)).padding_ltrb(
            0.0,
            inter_baseline_drop(14.0, 12.0),
            0.0,
            0.0
        ),
    ]
    .space_between()
    .cross_align(CrossAlign::Start)
    .fill_width()
    .padding_ltrb(MARGIN, 28.0, MARGIN, 0.0);
    // `margin-top: 14px; display: flex; gap: 16px`. The design's fifth
    // ring ends two dp before the screen does; more than that scroll
    // sideways.
    let rings = ScrollView::horizontal(
        Row::new()
            .push(new_room())
            .extend(app.live_rooms().iter().map(|room| room_ring(app, room)))
            .gap(16.0)
            .padding_xy(MARGIN, 0.0),
    )
    .id("rings");
    let shown = app.shown_items();
    let empty = shown.is_empty().then(|| {
        let line = match (app.search(), app.filter()) {
            (Some(_), _) => "Nothing found.",
            (None, Filter::Calls) => "No calls yet.",
            (None, Filter::Rooms) => "No rooms yet. Create one with New room.",
            (None, Filter::All) => "Nothing here yet. Create a room, or call someone.",
        };
        Container::new(inter(line, 13.0).color(MUTED).centered().fill_width())
            .fill_width()
            .padding_ltrb(0.0, 28.0, 0.0, 0.0)
            .id("empty")
    });
    // `margin-top: 12px; display: flex; flex-direction: column`.
    let list = Column::new()
        .extend(
            shown
                .into_iter()
                .map(|index| list_row(app, index, &app.items()[index])),
        )
        .push_maybe(empty)
        .fill_width()
        .padding_ltrb(MARGIN, 12.0, MARGIN, 0.0);
    let content = col![
        header(app),
        heading,
        Spacer::fixed(14.0),
        rings,
        // `margin-top: 24px`.
        Container::new(match app.search() {
            Some(query) => search(query),
            None => tabs(app),
        })
        .fill_width()
        .padding_ltrb(MARGIN, 24.0, MARGIN, 0.0),
        list,
    ]
    .fill_width()
    .padding_ltrb(0.0, 0.0, 0.0, 16.0);
    let body = col![
        ScrollView::new(content).id(Screen::Home.scroll_id()),
        tab_bar(Tab::Home),
    ]
    .fill();
    page(Screen::Home, body).safe_area(SafeEdges::TOP).into()
}

/// A row of the sheet: a 44 dp outlined disc with an icon, a title and a
/// line under it.
fn start_row(
    asset: &'static SvgAsset,
    title: &'static str,
    detail: &'static str,
    message: Message,
    id: &'static str,
) -> Container<Message> {
    row![
        Container::new(icon(asset, 20.0, INK))
            .center()
            .size(44.0, 44.0)
            .radius(22.0)
            .border(1.0, LINE),
        col![
            inter(title, 16.0).semibold(),
            inter(detail, 13.0).color(MUTED),
        ]
        .gap(4.0)
        .fill_width(),
    ]
    .gap(14.0)
    .cross_align(CrossAlign::Center)
    .fill_width()
    .height(68.0)
    .on_press(message)
    .id(id)
}

/// What the bar's middle button ("Start a call or room") opens: a sheet
/// with the three things there are to start. **The app's own**, from the
/// list's rows and the outlined round buttons.
pub fn start_sheet() -> Container<Message> {
    sheet(
        "start-sheet",
        col![
            grotesk("Start", 20.0).heading(),
            Spacer::fixed(8.0),
            start_row(
                &icons::PLUS,
                "New room",
                "Chat, voice and video. Free, however long",
                Message::OpenNewRoom,
                "start-room"
            ),
            start_row(
                &icons::VIDEO,
                "Call someone",
                "A person who uses Barua Talk",
                Message::OpenPeople,
                "start-person"
            ),
            start_row(
                &icons::PHONE,
                "Dial a number",
                "A call to any phone",
                Message::OpenDial,
                "start-dial"
            ),
        ]
        .fill_width(),
    )
}

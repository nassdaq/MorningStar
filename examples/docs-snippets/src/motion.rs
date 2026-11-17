//! Samples for motion and for moving a scroll view.

use std::time::Duration;

use morningstar::ui::prelude::*;

#[derive(Clone)]
pub enum Message {
    Toggle,
    Sent,
}

#[derive(Default)]
pub struct Chat {
    pub open: bool,
    pub lines: Vec<String>,
    to_end: bool,
}

impl Application for Chat {
    type Message = Message;

    // docs:begin motion
    fn view(&self) -> Element<Message> {
        let mut lines = Column::new().gap(8.0).fill_width();
        for line in &self.lines {
            lines = lines.push(text(line.clone()).fill_width());
        }
        col![
            // The view only says how tall the panel is now; `animate`
            // shows the change happening.
            Container::empty()
                .height(if self.open { 160.0 } else { 56.0 })
                .fill_width()
                .radius(16.0)
                .background(ThemeColor::Surface)
                .animate(Duration::from_millis(200), Easing::EaseOut)
                .on_press(Message::Toggle),
        ]
        // Present only while open: fades in when it enters the tree and
        // out when it leaves.
        .push_maybe(self.open.then(|| {
            Container::new(text("Details"))
                .id("details")
                .transition(Transition::fade())
        }))
        .push(ScrollView::new(lines).id("chat"))
        .gap(12.0)
        .fill()
        .into()
    }
    // docs:end motion

    fn update(&mut self, message: Message) {
        match message {
            Message::Toggle => self.open = !self.open,
            Message::Sent => {
                self.lines.push("Sawa".to_owned());
                self.to_end = true;
            }
        }
    }

    // docs:begin scroll
    fn effects(&mut self, out: &mut Effects<Message>) {
        if std::mem::take(&mut self.to_end) {
            // Carried out in the next frame, after its layout, so "the
            // end" is the end of the content that frame has.
            out.scroll(ScrollTo::end("chat").animated());
        }
    }
    // docs:end scroll
}

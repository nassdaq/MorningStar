//! [`Navigator`]: the stack of screens an application is showing.

use std::time::Duration;

use crate::motion::{Easing, Transition};

/// What the last change to a [`Navigator`] was.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Navigation {
    /// Nothing has changed since it was made.
    #[default]
    None,
    /// A screen was pushed on top.
    Push,
    /// The top screen was popped; the one under it shows again.
    Pop,
    /// The top screen, or the whole stack, was replaced.
    Replace,
}

/// The screens an application is showing, as a stack: the one on top is on
/// screen, the ones under it are where Back leads.
///
/// It is plain application state, a `Vec` that is never empty with the few
/// operations navigation needs, and nothing else: it does not know what a
/// screen looks like, holds no widget state and starts no animation. `S`
/// is whatever names a screen, usually a small `Copy` enum (with data for
/// screens that need it: `Screen::Repair(id)`).
///
/// ```
/// use morningstar_ui::prelude::*;
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// enum Screen { Home, Settings, About }
///
/// #[derive(Clone)]
/// enum Message { Open(Screen), Close }
///
/// struct App { screens: Navigator<Screen> }
///
/// impl Application for App {
///     type Message = Message;
///
///     fn view(&self) -> Element<Message> {
///         let screen = *self.screens.current();
///         let content: Element<Message> = match screen {
///             Screen::Home => Button::new("Settings")
///                 .on_press(Message::Open(Screen::Settings))
///                 .into(),
///             Screen::Settings | Screen::About => Button::new("Back")
///                 .on_press(Message::Close)
///                 .into(),
///         };
///         // The id gives each screen its own scroll positions and focus;
///         // the transition slides screens in and out in the direction of
///         // the last change. Leave it out for no animation.
///         SafeArea::new(content)
///             .id(screen)
///             .transition(self.screens.transition())
///             .into()
///     }
///
///     fn update(&mut self, message: Message) {
///         match message {
///             Message::Open(screen) => self.screens.push(screen),
///             Message::Close => { self.screens.back(); }
///         }
///     }
///
///     // The system back gesture: one screen back, or out of the app.
///     fn back(&mut self) -> bool {
///         self.screens.back()
///     }
/// }
///
/// let mut ui = Ui::new(App { screens: Navigator::new(Screen::Home) });
/// ui.send(Message::Open(Screen::Settings));
/// assert_eq!(ui.app().screens.stack(), [Screen::Home, Screen::Settings]);
/// assert!(ui.back());
/// assert!(!ui.back(), "on the first screen, Back leaves the app");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Navigator<S> {
    /// Never empty; the last is on screen.
    stack: Vec<S>,
    last: Navigation,
}

/// A stack holding only the default screen.
impl<S: Default> Default for Navigator<S> {
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S> Navigator<S> {
    /// A stack holding only `root`, the first screen.
    pub fn new(root: S) -> Self {
        Self {
            stack: vec![root],
            last: Navigation::None,
        }
    }

    /// The screen on top: the one to show.
    pub fn current(&self) -> &S {
        self.stack.last().expect("a navigator is never empty")
    }

    /// The screen on top, mutably (to change the data a screen carries).
    pub fn current_mut(&mut self) -> &mut S {
        self.stack.last_mut().expect("a navigator is never empty")
    }

    /// The first screen, at the bottom of the stack.
    pub fn root(&self) -> &S {
        &self.stack[0]
    }

    /// Every screen, the first at the bottom, the current one last.
    pub fn stack(&self) -> &[S] {
        &self.stack
    }

    /// How many screens are on the stack; at least 1.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// True if there is a screen to go back to.
    pub fn can_pop(&self) -> bool {
        self.stack.len() > 1
    }

    /// Shows `screen`, keeping the current one under it to come back to.
    pub fn push(&mut self, screen: S) {
        self.stack.push(screen);
        self.last = Navigation::Push;
    }

    /// Shows `screen` in place of the current one: Back will lead where it
    /// led before (a login screen replaced by the home screen).
    pub fn replace(&mut self, screen: S) {
        *self.current_mut() = screen;
        self.last = Navigation::Replace;
    }

    /// Goes back one screen and returns the one that was showing, or
    /// `None`, changing nothing, on the first screen.
    pub fn pop(&mut self) -> Option<S> {
        if !self.can_pop() {
            return None;
        }
        self.last = Navigation::Pop;
        self.stack.pop()
    }

    /// Goes back one screen; false, changing nothing, on the first. This
    /// is the answer [`Application::back`](crate::Application::back)
    /// wants: `fn back(&mut self) -> bool { self.screens.back() }`.
    pub fn back(&mut self) -> bool {
        self.pop().is_some()
    }

    /// Goes back to the first screen, dropping every screen above it.
    /// Returns false if it was already showing.
    pub fn pop_to_root(&mut self) -> bool {
        if !self.can_pop() {
            return false;
        }
        self.stack.truncate(1);
        self.last = Navigation::Pop;
        true
    }

    /// Goes back to the nearest screen for which `wanted` is true,
    /// dropping the ones above it. Returns false, changing nothing, if
    /// there is none below the current screen.
    pub fn pop_to(&mut self, wanted: impl Fn(&S) -> bool) -> bool {
        let below = &self.stack[..self.stack.len() - 1];
        match below.iter().rposition(wanted) {
            Some(index) => {
                self.stack.truncate(index + 1);
                self.last = Navigation::Pop;
                true
            }
            None => false,
        }
    }

    /// Starts again: the stack holds only `root` (after signing out, or
    /// when a tab bar switches section).
    pub fn reset(&mut self, root: S) {
        self.stack.clear();
        self.stack.push(root);
        self.last = Navigation::Replace;
    }

    /// Replaces the whole stack: `screens` from the bottom up, the last
    /// one showing. An empty list is ignored and returns false.
    pub fn set(&mut self, screens: impl IntoIterator<Item = S>) -> bool {
        let mut screens = screens.into_iter().peekable();
        if screens.peek().is_none() {
            return false;
        }
        self.stack.clear();
        self.stack.extend(screens);
        self.last = Navigation::Replace;
        true
    }

    /// What the last change was, for a view that wants to animate it its
    /// own way.
    pub fn last_change(&self) -> Navigation {
        self.last
    }

    /// The [`Transition`] for the root of the current screen that suits
    /// the last change: a pushed screen slides in from the right while the
    /// one it covers moves a little to the left and fades; a pop is the
    /// same backwards; a replacement cross-fades; and at start there is
    /// none. Put it on each screen's root, next to the screen's `.id(..)`
    /// (the screens must have different ids, or nothing is seen to enter
    /// or leave):
    ///
    /// ```ignore
    /// screen_view.id(screen).transition(self.screens.transition())
    /// ```
    ///
    /// How the *leaving* screen goes is taken from the screen that enters
    /// in the same frame (see
    /// [`Container::transition`](crate::Container::transition)), which is
    /// why one value, read from the navigator's state when the frame is
    /// built, is enough for both.
    pub fn transition(&self) -> Transition {
        const TIME: Duration = Duration::from_millis(300);
        match self.last {
            Navigation::None => Transition::none(),
            Navigation::Push => Transition::slide(1.0, 0.0)
                .exit_slide(-0.25, 0.0)
                .exit_fade()
                .duration(TIME)
                .easing(Easing::EaseOut),
            Navigation::Pop => Transition::slide(-0.25, 0.0)
                .enter_fade()
                .exit_slide(1.0, 0.0)
                .duration(TIME)
                .easing(Easing::EaseOut),
            Navigation::Replace => Transition::fade().duration(TIME),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_replace_and_back() {
        let mut screens = Navigator::new("home");
        assert_eq!(*screens.current(), "home");
        assert_eq!(screens.depth(), 1);
        assert!(!screens.can_pop());
        assert_eq!(screens.last_change(), Navigation::None);
        assert!(!screens.back(), "nothing under the first screen");
        assert_eq!(screens.pop(), None);
        assert_eq!(screens.last_change(), Navigation::None);

        screens.push("list");
        screens.push("detail");
        assert_eq!(screens.stack(), ["home", "list", "detail"]);
        assert_eq!(screens.last_change(), Navigation::Push);
        assert_eq!(*screens.root(), "home");

        screens.replace("edit");
        assert_eq!(screens.stack(), ["home", "list", "edit"]);
        assert_eq!(screens.last_change(), Navigation::Replace);

        assert_eq!(screens.pop(), Some("edit"));
        assert_eq!(*screens.current(), "list");
        assert_eq!(screens.last_change(), Navigation::Pop);
        assert!(screens.back());
        assert_eq!(screens.stack(), ["home"]);
        assert!(!screens.back());
        assert_eq!(screens.stack(), ["home"], "the first screen stays");
    }

    #[test]
    fn going_back_several_screens_and_starting_again() {
        let mut screens = Navigator::new(0);
        for screen in 1..=4 {
            screens.push(screen);
        }
        assert!(!screens.pop_to(|screen| *screen == 4), "the current one");
        assert!(!screens.pop_to(|screen| *screen == 9));
        assert_eq!(screens.depth(), 5);
        assert!(screens.pop_to(|screen| *screen == 2));
        assert_eq!(screens.stack(), [0, 1, 2]);
        assert!(screens.pop_to_root());
        assert!(!screens.pop_to_root());
        assert_eq!(screens.stack(), [0]);

        screens.reset(7);
        assert_eq!(screens.stack(), [7]);
        assert_eq!(screens.last_change(), Navigation::Replace);
        assert!(!screens.set([]));
        assert!(screens.set([1, 2, 3]));
        assert_eq!(*screens.current(), 3);
        *screens.current_mut() = 30;
        assert_eq!(screens.stack(), [1, 2, 30]);
    }

    #[test]
    fn the_transition_follows_the_last_change() {
        let mut screens = Navigator::new(0);
        assert_eq!(screens.transition(), Transition::none());
        screens.push(1);
        let push = screens.transition();
        assert_eq!(push.enter.shift, (1.0, 0.0));
        assert_eq!(push.exit.shift, (-0.25, 0.0));
        screens.pop();
        let pop = screens.transition();
        assert_eq!(pop.enter.shift, (-0.25, 0.0));
        assert_eq!(pop.exit.shift, (1.0, 0.0));
        assert_eq!(pop.exit.opacity, 1.0);
        screens.replace(5);
        assert_eq!(screens.transition().enter.opacity, 0.0);
    }
}

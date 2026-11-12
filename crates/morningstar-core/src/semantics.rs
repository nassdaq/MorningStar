//! What a screen means, for assistive technology (ADR 0032).
//!
//! Morningstar draws its own pixels, so to TalkBack or VoiceOver a screen
//! is one opaque picture unless it is described. A [`SemanticsTree`] is that
//! description: the nodes a screen reader can land on, in reading order,
//! each with where it is, what it is, what it says and what can be done to
//! it. The UI layer derives one from the widgets of a frame; the engine
//! compares it with the one the host was last given and encodes the
//! difference ([`SemanticsTree::encode_update`]) into one flat buffer that
//! both hosts decode.
//!
//! Everything here is plain data. Nothing is built, compared or encoded
//! unless assistive technology is on.

use std::collections::HashMap;

use crate::Rect;

/// What a node is, which decides how a screen reader names it ("button",
/// "heading", "edit box").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Role {
    /// A container that only groups its children. The default of a
    /// labelled box that can do nothing.
    #[default]
    Group = 0,
    /// Static text.
    Text = 1,
    /// A heading: screen readers can jump between headings.
    Heading = 2,
    /// Something that is activated: a button, a tappable row or card.
    Button = 3,
    /// A button that goes somewhere else (a link in a sentence).
    Link = 4,
    /// A picture that means something.
    Image = 5,
    /// An editable text field.
    TextField = 6,
    /// A box that is ticked or not.
    Checkbox = 7,
    /// An on/off switch.
    Switch = 8,
    /// One choice of several, of which one is chosen.
    Radio = 9,
    /// One tab of a tab bar.
    Tab = 10,
    /// A scrolling container: a list.
    List = 11,
    /// A progress bar or a score: read with its value.
    Progress = 12,
    /// A value that can be raised and lowered.
    Slider = 13,
}

impl Role {
    /// The role with this wire value; `Group` for one that is not known.
    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Text,
            2 => Self::Heading,
            3 => Self::Button,
            4 => Self::Link,
            5 => Self::Image,
            6 => Self::TextField,
            7 => Self::Checkbox,
            8 => Self::Switch,
            9 => Self::Radio,
            10 => Self::Tab,
            11 => Self::List,
            12 => Self::Progress,
            13 => Self::Slider,
            _ => Self::Group,
        }
    }
}

/// How urgently a change inside a node is spoken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Live {
    /// Changes are not announced: the default.
    #[default]
    Off,
    /// Announced when the screen reader has finished what it is saying.
    Polite,
    /// Announced at once, interrupting.
    Assertive,
}

macro_rules! bit_set {
    ($(#[$meta:meta])* $name:ident: $($(#[$flag_meta:meta])* $flag:ident = $bit:expr;)*) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub struct $name(pub u16);

        impl $name {
            /// None.
            pub const NONE: Self = Self(0);
            $($(#[$flag_meta])* pub const $flag: Self = Self(1 << $bit);)*

            /// True if every bit of `other` is set.
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            /// True if no bit is set.
            pub const fn is_empty(self) -> bool {
                self.0 == 0
            }

            /// Sets or clears the bits of `other`.
            pub const fn with(self, other: Self, on: bool) -> Self {
                if on {
                    Self(self.0 | other.0)
                } else {
                    Self(self.0 & !other.0)
                }
            }
        }

        impl std::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }
        }

        impl std::ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, other: Self) {
                self.0 |= other.0;
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut first = true;
                f.write_str(concat!(stringify!($name), "("))?;
                $(
                    if self.contains(Self::$flag) {
                        if !first {
                            f.write_str(" | ")?;
                        }
                        first = false;
                        f.write_str(stringify!($flag))?;
                    }
                )*
                let _ = first;
                f.write_str(")")
            }
        }
    };
}

bit_set! {
    /// The states of a node. A bit set; the wire value is the `u16`.
    States:
    /// Cannot be operated now: spoken as "disabled", and it has no
    /// actions.
    DISABLED = 0;
    /// The chosen one of its siblings: the current tab, the picked chip.
    SELECTED = 1;
    /// Can be ticked: `CHECKED` then says whether it is.
    CHECKABLE = 2;
    /// Ticked, or on.
    CHECKED = 3;
    /// Has the input focus: the text field being edited.
    FOCUSED = 4;
    /// A password field: its `value` is bullets, never its characters.
    PASSWORD = 5;
    /// A text field of more than one line.
    MULTILINE = 6;
    /// Can be opened and closed: `EXPANDED` then says which it is.
    EXPANDABLE = 7;
    /// Open.
    EXPANDED = 8;
    /// Changes inside it are announced when convenient.
    LIVE_POLITE = 9;
    /// Changes inside it are announced at once.
    LIVE_ASSERTIVE = 10;
}

bit_set! {
    /// What can be done to a node. A bit set; the wire value is the `u16`,
    /// and the bit positions are the codes of
    /// [`Action::from_wire`].
    Actions:
    /// Activate it: a tap.
    ACTIVATE = 0;
    /// Press and hold.
    LONG_PRESS = 1;
    /// Scroll towards the end.
    SCROLL_FORWARD = 2;
    /// Scroll towards the start.
    SCROLL_BACKWARD = 3;
    /// Give it the input focus.
    FOCUS = 4;
    /// Replace its text.
    SET_TEXT = 5;
    /// Move its caret or selection.
    SET_SELECTION = 6;
    /// Raise its value one step.
    INCREMENT = 7;
    /// Lower its value one step.
    DECREMENT = 8;
    /// Leave: the system's Back. Offered on the root.
    DISMISS = 9;
}

/// One thing to do to a node, as assistive technology asks for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A tap: presses a button, focuses a text field.
    Activate,
    /// Press and hold (on a text field: selects the word under it).
    LongPress,
    /// Scroll a list one page towards its end.
    ScrollForward,
    /// Scroll a list one page towards its start.
    ScrollBackward,
    /// Give a text field the input focus.
    Focus,
    /// Replace a text field's text. It passes the field's own filters
    /// (`digits_only`, `max_length`) like typed text.
    SetText(String),
    /// Put a text field's selection from `start` to `end`, counted in
    /// characters (Unicode scalar values) of its value; the caret alone
    /// when they are equal, at `end` otherwise. Characters, not bytes, so
    /// that a password field, whose node shows one bullet per character,
    /// can be addressed without its text.
    SetSelection {
        /// Where the selection starts.
        start: usize,
        /// Where it ends: where the caret is.
        end: usize,
    },
    /// Raise the value one step.
    Increment,
    /// Lower the value one step.
    Decrement,
    /// The system's Back.
    Dismiss,
}

impl Action {
    /// The [`Actions`] bit a node must offer for this to be performed.
    pub const fn bit(&self) -> Actions {
        match self {
            Self::Activate => Actions::ACTIVATE,
            Self::LongPress => Actions::LONG_PRESS,
            Self::ScrollForward => Actions::SCROLL_FORWARD,
            Self::ScrollBackward => Actions::SCROLL_BACKWARD,
            Self::Focus => Actions::FOCUS,
            Self::SetText(_) => Actions::SET_TEXT,
            Self::SetSelection { .. } => Actions::SET_SELECTION,
            Self::Increment => Actions::INCREMENT,
            Self::Decrement => Actions::DECREMENT,
            Self::Dismiss => Actions::DISMISS,
        }
    }

    /// The action a host names by `code` (the bit position of its
    /// [`Actions`] flag: 0 activate, 1 long press, 2 scroll forward, 3
    /// scroll backward, 4 focus, 5 set text, 6 set selection, 7 increment,
    /// 8 decrement, 9 dismiss). `text` is the new text of a set-text;
    /// `start` and `end` the selection of a set-selection, in characters.
    /// `None` for a code that is not known (a host newer than this
    /// library).
    pub fn from_wire(code: u32, text: Option<String>, start: usize, end: usize) -> Option<Self> {
        Some(match code {
            0 => Self::Activate,
            1 => Self::LongPress,
            2 => Self::ScrollForward,
            3 => Self::ScrollBackward,
            4 => Self::Focus,
            5 => Self::SetText(text.unwrap_or_default()),
            6 => Self::SetSelection { start, end },
            7 => Self::Increment,
            8 => Self::Decrement,
            9 => Self::Dismiss,
            _ => return None,
        })
    }
}

/// One thing a screen reader can land on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticsNode {
    /// The identity of the widget it stands for: the same from frame to
    /// frame while the widget stays where it is in the tree (or keeps its
    /// explicit id), so that the reader's focus stays on it.
    pub id: u64,
    /// Where it is on the surface, in physical pixels: the bounding box of
    /// what can be seen of it, through scroll offsets, clips and
    /// transforms.
    pub bounds: Rect,
    /// What it is.
    pub role: Role,
    /// What it is called: what is spoken first.
    pub label: String,
    /// What it holds or shows now: a field's text (bullets for a
    /// password), a progress bar's "72 percent".
    pub value: String,
    /// What it is for, spoken after a pause: a field's placeholder,
    /// "opens the report".
    pub hint: String,
    /// Its states.
    pub states: States,
    /// What can be done to it.
    pub actions: Actions,
    /// A focused text field's selection in characters of `value`, from
    /// where it was started to the caret (equal for a caret alone).
    pub selection: Option<(u32, u32)>,
    /// A list's scroll position and how far it can scroll, in dp.
    pub scroll: Option<(f32, f32)>,
    /// The index of the node it is inside; `None` for the root.
    pub parent: Option<u32>,
    /// How many nodes are inside it, at any depth: they follow it.
    pub descendants: u32,
}

impl SemanticsNode {
    /// True if the node is in `state`.
    pub fn is(&self, state: States) -> bool {
        self.states.contains(state)
    }

    /// True if `action` can be performed on the node.
    pub fn can(&self, action: Actions) -> bool {
        self.actions.contains(action)
    }

    /// How its changes are announced.
    pub fn live(&self) -> Live {
        if self.is(States::LIVE_ASSERTIVE) {
            Live::Assertive
        } else if self.is(States::LIVE_POLITE) {
            Live::Polite
        } else {
            Live::Off
        }
    }

    /// The label and the value as a screen reader would say them one
    /// after the other.
    pub fn spoken(&self) -> String {
        match (self.label.is_empty(), self.value.is_empty()) {
            (false, false) => format!("{}, {}", self.label, self.value),
            (false, true) => self.label.clone(),
            (true, _) => self.value.clone(),
        }
    }
}

/// The nodes of one frame in reading order, which is also depth-first
/// order: a node is followed by everything inside it. The first node is
/// the root, which stands for the whole surface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticsTree {
    nodes: Vec<SemanticsNode>,
}

impl SemanticsTree {
    /// An empty tree.
    pub const fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Removes every node, keeping the allocation.
    pub fn clear(&mut self) {
        self.nodes.clear();
    }

    /// Adds a node inside `parent` (an index returned earlier; `None` for
    /// the root, which must come first) and returns its index. Nodes must
    /// be added in depth-first order: everything inside a node before its
    /// next sibling.
    pub fn push(&mut self, parent: Option<usize>, mut node: SemanticsNode) -> usize {
        let index = self.nodes.len();
        node.parent = parent.map(|parent| parent as u32);
        node.descendants = 0;
        self.nodes.push(node);
        let mut above = parent;
        while let Some(up) = above {
            let Some(ancestor) = self.nodes.get_mut(up) else {
                break;
            };
            ancestor.descendants += 1;
            above = ancestor.parent.map(|parent| parent as usize);
        }
        index
    }

    /// The nodes, in reading order.
    pub fn nodes(&self) -> &[SemanticsNode] {
        &self.nodes
    }

    /// How many nodes there are, the root included.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True if there is not even a root: nothing was built.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The root.
    pub fn root(&self) -> Option<&SemanticsNode> {
        self.nodes.first()
    }

    /// The node at `index`.
    pub fn get(&self, index: usize) -> Option<&SemanticsNode> {
        self.nodes.get(index)
    }

    /// The index of the node with this identity.
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.nodes.iter().position(|node| node.id == id)
    }

    /// The node with this identity.
    pub fn node(&self, id: u64) -> Option<&SemanticsNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    /// The indices of the nodes directly inside the node at `index`, in
    /// reading order.
    pub fn children(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        let end = self
            .nodes
            .get(index)
            .map_or(0, |node| index + 1 + node.descendants as usize);
        let mut next = index + 1;
        std::iter::from_fn(move || {
            if next >= end {
                return None;
            }
            let child = next;
            next += 1 + self.nodes.get(child)?.descendants as usize;
            Some(child)
        })
    }

    /// The first node, in reading order, whose label is exactly `label`.
    pub fn find_by_label(&self, label: &str) -> Option<&SemanticsNode> {
        self.nodes.iter().find(|node| node.label == label)
    }

    /// The first node whose label contains `text`.
    pub fn find_containing(&self, text: &str) -> Option<&SemanticsNode> {
        self.nodes.iter().find(|node| node.label.contains(text))
    }

    /// The first node with this role whose label is exactly `label`.
    pub fn find(&self, role: Role, label: &str) -> Option<&SemanticsNode> {
        self.nodes
            .iter()
            .find(|node| node.role == role && node.label == label)
    }

    /// Every node with this role, in reading order.
    pub fn with_role(&self, role: Role) -> impl Iterator<Item = &SemanticsNode> + '_ {
        self.nodes.iter().filter(move |node| node.role == role)
    }

    /// What a screen reader would say swiping through the screen: each
    /// node's label and value, in order, leaving out nodes that say
    /// nothing (the root, unlabelled lists).
    pub fn spoken(&self) -> Vec<String> {
        self.nodes
            .iter()
            .map(SemanticsNode::spoken)
            .filter(|spoken| !spoken.is_empty())
            .collect()
    }

    /// The number a host knows the node at `index` by: its identity folded
    /// to a positive `i32`, because Android names virtual views with an
    /// `int`. The root is 0. Two identities that fold to the same number
    /// would make one node unreachable; with a few hundred nodes among two
    /// thousand million numbers that is a one-in-a-hundred-thousand event,
    /// and the later node then takes the next free number.
    fn host_ids(&self) -> Vec<i32> {
        let mut taken: HashMap<i32, ()> = HashMap::with_capacity(self.nodes.len());
        self.nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                if index == 0 {
                    return 0;
                }
                let mut id = fold_id(node.id);
                while taken.insert(id, ()).is_some() {
                    id = if id == i32::MAX { 1 } else { id + 1 };
                }
                id
            })
            .collect()
    }

    /// The identity of the node a host knows as `host_id`.
    pub fn id_of_host(&self, host_id: i32) -> Option<u64> {
        if host_id == 0 {
            return self.root().map(|root| root.id);
        }
        let ids = self.host_ids();
        ids.iter()
            .position(|id| *id == host_id)
            .and_then(|index| self.nodes.get(index))
            .map(|node| node.id)
    }

    /// Encodes what a host that holds `previous` must be told to hold this
    /// tree instead, appending to `out`. Returns false, having written
    /// nothing, if the two are the same, which is the usual answer.
    /// `previous` is `None` when the host holds nothing: everything is
    /// then sent and the host is told to forget what it had.
    ///
    /// # Format
    ///
    /// Little-endian. Identities are host ids (see above: positive
    /// `i32`s, the root 0). A string is a `u32` byte count and that many
    /// bytes of UTF-8.
    ///
    /// ```text
    /// u32  version (1)
    /// u32  flags: bit 0 = forget every node held before applying this
    /// i32  the node with the input focus, or -1
    /// u32  count of removed nodes, then that many i32 ids
    /// u32  count of added or changed nodes, then for each:
    ///   i32  id
    ///   i32  parent id (-1 for the root)
    ///   u8   role (the `Role` values)
    ///   u8   what changed (the `CHANGED_*` bits)
    ///   u16  states (the `States` bits)
    ///   u16  actions (the `Actions` bits)
    ///   u16  0
    ///   f32  left, top, right, bottom: bounds on the surface in pixels
    ///   f32  scroll position, scroll range in dp (both -1 if not a list)
    ///   i32  selection start, end in UTF-16 units of the value (-1, -1
    ///        if there is none)
    ///   u32  count of children, then that many i32 ids, in reading order
    ///   str  label
    ///   str  value
    ///   str  hint
    /// ```
    pub fn encode_update(&self, previous: Option<&SemanticsTree>, out: &mut Vec<u8>) -> bool {
        if previous == Some(self) {
            return false;
        }
        let ids = self.host_ids();
        let empty = SemanticsTree::new();
        let old = previous.unwrap_or(&empty);
        let old_ids = old.host_ids();
        // Old nodes by identity, each with its host id and its children's.
        let old_index: HashMap<u64, usize> = old
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id, index))
            .collect();
        let new_index: HashMap<u64, usize> = self
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| (node.id, index))
            .collect();

        put_u32(out, UPDATE_VERSION);
        put_u32(out, u32::from(previous.is_none()));
        let focused = self
            .nodes
            .iter()
            .position(|node| node.is(States::FOCUSED))
            .and_then(|index| ids.get(index).copied())
            .unwrap_or(-1);
        put_i32(out, focused);

        let removed: Vec<i32> = old
            .nodes
            .iter()
            .zip(&old_ids)
            // A node whose number changed hands is sent again under it.
            .filter(|(node, old_id)| {
                new_index
                    .get(&node.id)
                    .and_then(|index| ids.get(*index))
                    .is_none_or(|id| id != *old_id)
            })
            .map(|(_, id)| *id)
            .collect();
        put_u32(out, removed.len() as u32);
        for id in &removed {
            put_i32(out, *id);
        }

        let count_at = out.len();
        put_u32(out, 0);
        let mut count = 0u32;
        for (index, node) in self.nodes.iter().enumerate() {
            let id = ids[index];
            let before = old_index
                .get(&node.id)
                .copied()
                .filter(|old_at| old_ids[*old_at] == id);
            let changed = match before {
                None => CHANGED_ADDED,
                Some(old_at) => {
                    let was = &old.nodes[old_at];
                    let same_children = self
                        .children(index)
                        .map(|child| ids[child])
                        .eq(old.children(old_at).map(|child| old_ids[child]));
                    let mut changed = 0;
                    if was.bounds != node.bounds {
                        changed |= CHANGED_BOUNDS;
                    }
                    if was.label != node.label || was.hint != node.hint || was.role != node.role {
                        changed |= CHANGED_LABEL;
                    }
                    if was.value != node.value {
                        changed |= CHANGED_VALUE;
                    }
                    if was.states != node.states || was.actions != node.actions {
                        changed |= CHANGED_STATE;
                    }
                    if !same_children {
                        changed |= CHANGED_CHILDREN;
                    }
                    if was.scroll != node.scroll {
                        changed |= CHANGED_SCROLL;
                    }
                    if was.selection != node.selection {
                        changed |= CHANGED_SELECTION;
                    }
                    let parent_before = was.parent.map(|parent| old_ids[parent as usize]);
                    let parent_now = node.parent.map(|parent| ids[parent as usize]);
                    if parent_before != parent_now {
                        changed |= CHANGED_CHILDREN;
                    }
                    changed
                }
            };
            if changed == 0 {
                continue;
            }
            count += 1;
            put_i32(out, id);
            put_i32(out, node.parent.map_or(-1, |parent| ids[parent as usize]));
            out.push(node.role as u8);
            out.push(changed);
            out.extend_from_slice(&node.states.0.to_le_bytes());
            out.extend_from_slice(&node.actions.0.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            let bounds = node.bounds;
            for edge in [
                bounds.x,
                bounds.y,
                bounds.x + bounds.width,
                bounds.y + bounds.height,
            ] {
                put_f32(out, edge);
            }
            let (position, range) = node.scroll.unwrap_or((-1.0, -1.0));
            put_f32(out, position);
            put_f32(out, range);
            let (start, end) = node.selection.map_or((-1, -1), |(start, end)| {
                (
                    utf16_of_chars(&node.value, start as usize),
                    utf16_of_chars(&node.value, end as usize),
                )
            });
            put_i32(out, start);
            put_i32(out, end);
            put_u32(out, self.children(index).count() as u32);
            for child in self.children(index) {
                put_i32(out, ids[child]);
            }
            put_str(out, &node.label);
            put_str(out, &node.value);
            put_str(out, &node.hint);
        }
        out[count_at..count_at + 4].copy_from_slice(&count.to_le_bytes());
        true
    }
}

/// The version of the update format.
pub const UPDATE_VERSION: u32 = 1;

/// The node was not held before (or is sent again whole).
pub const CHANGED_ADDED: u8 = 1;
/// Its bounds moved.
pub const CHANGED_BOUNDS: u8 = 1 << 1;
/// Its label, hint or role changed.
pub const CHANGED_LABEL: u8 = 1 << 2;
/// Its value changed.
pub const CHANGED_VALUE: u8 = 1 << 3;
/// Its states or actions changed.
pub const CHANGED_STATE: u8 = 1 << 4;
/// Its children, or its parent, changed.
pub const CHANGED_CHILDREN: u8 = 1 << 5;
/// It scrolled.
pub const CHANGED_SCROLL: u8 = 1 << 6;
/// Its selection moved.
pub const CHANGED_SELECTION: u8 = 1 << 7;

/// A 64-bit identity as a positive, non-zero `i32`.
fn fold_id(id: u64) -> i32 {
    let folded = ((id ^ (id >> 32)) & 0x7FFF_FFFF) as i32;
    if folded == 0 { 1 } else { folded }
}

/// The length in UTF-16 units of the first `chars` characters of `text`.
fn utf16_of_chars(text: &str, chars: usize) -> i32 {
    let units: usize = text.chars().take(chars).map(char::len_utf16).sum();
    i32::try_from(units).unwrap_or(i32::MAX)
}

/// How many characters the first `units` UTF-16 units of `text` are: what
/// turns a host's offset into the one [`Action::SetSelection`] takes. An
/// offset inside a surrogate pair counts the pair.
pub fn chars_of_utf16(text: &str, units: usize) -> usize {
    let mut seen = 0;
    let mut chars = 0;
    for character in text.chars() {
        if seen >= units {
            break;
        }
        seen += character.len_utf16();
        chars += 1;
    }
    chars
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_str(out: &mut Vec<u8>, text: &str) {
    put_u32(out, u32::try_from(text.len()).unwrap_or(0));
    if u32::try_from(text.len()).is_ok() {
        out.extend_from_slice(text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, label: &str) -> SemanticsNode {
        SemanticsNode {
            id,
            label: label.to_owned(),
            bounds: Rect::new(0.0, 0.0, 10.0, 10.0),
            ..SemanticsNode::default()
        }
    }

    fn sample() -> SemanticsTree {
        let mut tree = SemanticsTree::new();
        let root = tree.push(None, node(1, ""));
        let list = tree.push(Some(root), node(2, "list"));
        tree.push(Some(list), node(3, "a"));
        tree.push(Some(list), node(4, "b"));
        tree.push(Some(root), node(5, "c"));
        tree
    }

    /// Reads an update back, as a host would.
    struct Reader<'a>(&'a [u8]);

    impl Reader<'_> {
        fn u32(&mut self) -> u32 {
            let (head, rest) = self.0.split_at(4);
            self.0 = rest;
            u32::from_le_bytes(head.try_into().unwrap())
        }
        fn i32(&mut self) -> i32 {
            self.u32() as i32
        }
        fn f32(&mut self) -> f32 {
            f32::from_bits(self.u32())
        }
        fn u16(&mut self) -> u16 {
            let (head, rest) = self.0.split_at(2);
            self.0 = rest;
            u16::from_le_bytes(head.try_into().unwrap())
        }
        fn u8(&mut self) -> u8 {
            let (head, rest) = self.0.split_at(1);
            self.0 = rest;
            head[0]
        }
        fn str(&mut self) -> String {
            let length = self.u32() as usize;
            let (head, rest) = self.0.split_at(length);
            self.0 = rest;
            String::from_utf8(head.to_vec()).unwrap()
        }
    }

    #[derive(Debug, PartialEq)]
    struct Decoded {
        id: i32,
        parent: i32,
        changed: u8,
        children: Vec<i32>,
        label: String,
        value: String,
        selection: (i32, i32),
    }

    fn decode(bytes: &[u8]) -> (u32, i32, Vec<i32>, Vec<Decoded>) {
        let mut reader = Reader(bytes);
        assert_eq!(reader.u32(), UPDATE_VERSION);
        let flags = reader.u32();
        let focused = reader.i32();
        let removed = (0..reader.u32()).map(|_| reader.i32()).collect();
        let mut nodes = Vec::new();
        for _ in 0..reader.u32() {
            let id = reader.i32();
            let parent = reader.i32();
            let _role = reader.u8();
            let changed = reader.u8();
            let (_states, _actions, _zero) = (reader.u16(), reader.u16(), reader.u16());
            for _ in 0..6 {
                reader.f32();
            }
            let selection = (reader.i32(), reader.i32());
            let children = (0..reader.u32()).map(|_| reader.i32()).collect();
            let (label, value, _hint) = (reader.str(), reader.str(), reader.str());
            nodes.push(Decoded {
                id,
                parent,
                changed,
                children,
                label,
                value,
                selection,
            });
        }
        assert!(reader.0.is_empty(), "trailing bytes");
        (flags, focused, removed, nodes)
    }

    #[test]
    fn children_and_descendants_follow_the_order_nodes_were_pushed_in() {
        let tree = sample();
        assert_eq!(tree.nodes()[0].descendants, 4);
        assert_eq!(tree.nodes()[1].descendants, 2);
        assert_eq!(tree.children(0).collect::<Vec<_>>(), [1, 4]);
        assert_eq!(tree.children(1).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(tree.children(4).count(), 0);
        assert_eq!(tree.spoken(), ["list", "a", "b", "c"]);
        assert_eq!(tree.find_by_label("b").map(|node| node.id), Some(4));
    }

    #[test]
    fn the_first_update_sends_everything_and_says_to_forget() {
        let tree = sample();
        let mut out = Vec::new();
        assert!(tree.encode_update(None, &mut out));
        let (flags, focused, removed, nodes) = decode(&out);
        assert_eq!(flags, 1);
        assert_eq!(focused, -1);
        assert!(removed.is_empty());
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[0].id, 0);
        assert_eq!(nodes[0].parent, -1);
        assert_eq!(nodes[0].children, [2, 5]);
        assert_eq!(nodes[1].children, [3, 4]);
        assert!(nodes.iter().all(|node| node.changed == CHANGED_ADDED));
    }

    #[test]
    fn an_unchanged_tree_writes_nothing() {
        let tree = sample();
        let mut out = Vec::new();
        assert!(!tree.encode_update(Some(&tree.clone()), &mut out));
        assert!(out.is_empty());
    }

    #[test]
    fn only_what_changed_is_sent() {
        let old = sample();
        let mut new = SemanticsTree::new();
        let root = new.push(None, node(1, ""));
        let list = new.push(Some(root), node(2, "list"));
        new.push(Some(list), node(3, "a"));
        // "b" (4) has gone, "c" says something else, "d" is new.
        new.push(Some(root), node(5, "see"));
        new.push(Some(root), node(6, "d"));
        let mut out = Vec::new();
        assert!(new.encode_update(Some(&old), &mut out));
        let (flags, _, removed, nodes) = decode(&out);
        assert_eq!(flags, 0);
        assert_eq!(removed, [4]);
        let summary: Vec<(i32, u8)> = nodes.iter().map(|node| (node.id, node.changed)).collect();
        assert_eq!(
            summary,
            [
                (0, CHANGED_CHILDREN),
                (2, CHANGED_CHILDREN),
                (5, CHANGED_LABEL),
                (6, CHANGED_ADDED),
            ]
        );
    }

    #[test]
    fn a_selection_is_sent_in_utf16_units_of_the_value() {
        let mut tree = SemanticsTree::new();
        let root = tree.push(None, node(1, ""));
        tree.push(
            Some(root),
            SemanticsNode {
                value: "a\u{1F600}b".to_owned(),
                selection: Some((1, 2)),
                states: States::FOCUSED,
                ..node(9, "field")
            },
        );
        let mut out = Vec::new();
        tree.encode_update(None, &mut out);
        let (_, focused, _, nodes) = decode(&out);
        assert_eq!(focused, 9);
        assert_eq!(nodes[1].selection, (1, 3));
        assert_eq!(chars_of_utf16("a\u{1F600}b", 3), 2);
        assert_eq!(chars_of_utf16("a\u{1F600}b", 2), 2);
        assert_eq!(chars_of_utf16("abc", 99), 3);
    }

    #[test]
    fn identities_that_fold_alike_still_get_different_numbers() {
        let mut tree = SemanticsTree::new();
        let root = tree.push(None, node(1, ""));
        tree.push(Some(root), node(7, "first"));
        // 7 << 32 folds to 7 as well.
        tree.push(Some(root), node(7 << 32, "second"));
        let ids = tree.host_ids();
        assert_eq!(ids, [0, 7, 8]);
        assert_eq!(tree.id_of_host(7), Some(7));
        assert_eq!(tree.id_of_host(8), Some(7 << 32));
        assert_eq!(tree.id_of_host(0), Some(1));
        assert_eq!(tree.id_of_host(99), None);
    }

    #[test]
    fn wire_actions_are_the_bit_positions() {
        for code in 0..10 {
            let action = Action::from_wire(code, Some("x".to_owned()), 1, 2).unwrap();
            assert_eq!(action.bit().0, 1 << code);
        }
        assert_eq!(Action::from_wire(10, None, 0, 0), None);
    }
}

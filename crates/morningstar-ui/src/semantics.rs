//! What a screen means to assistive technology (ADR 0032): the modifiers'
//! data, the derivation of a [`SemanticsTree`] from a painted frame, and
//! the checks behind [`Ui::accessibility_issues`](crate::Ui::accessibility_issues).
//!
//! Nothing here runs unless accessibility is on. Painting then notes, per
//! element, where it ended up on the screen and what it is (a `RawNode`);
//! `Collector::build` turns those notes into the nodes a screen reader
//! lands on, applying the defaults and the merge rules; the checks read
//! both.

use std::borrow::Cow;
use std::collections::HashSet;

use morningstar_core::{Actions, Color, Rect, Role, SemanticsNode, SemanticsTree, States};

/// The role and the flags an element was given. Inline in every element:
/// setting them allocates nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct SemFlags {
    pub role: Option<Role>,
    pub bits: u16,
}

impl SemFlags {
    pub const EXCLUDE: u16 = 1;
    pub const MERGE: u16 = 1 << 1;
    pub const GROUP: u16 = 1 << 2;
    pub const DISABLED: u16 = 1 << 3;
    pub const SELECTABLE: u16 = 1 << 4;
    pub const SELECTED: u16 = 1 << 5;
    pub const CHECKABLE: u16 = 1 << 6;
    pub const CHECKED: u16 = 1 << 7;
    pub const EXPANDABLE: u16 = 1 << 8;
    pub const EXPANDED: u16 = 1 << 9;
    pub const LIVE_POLITE: u16 = 1 << 10;
    pub const LIVE_ASSERTIVE: u16 = 1 << 11;

    pub fn set(&mut self, bit: u16, on: bool) {
        if on {
            self.bits |= bit;
        } else {
            self.bits &= !bit;
        }
    }

    pub const fn has(self, bit: u16) -> bool {
        self.bits & bit != 0
    }
}

/// The strings an element was given. Boxed in the element: only an element
/// with a label, a hint or a value pays for them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SemText {
    pub label: Option<Cow<'static, str>>,
    pub hint: Option<Cow<'static, str>>,
    pub value: Option<Cow<'static, str>>,
}

/// What an element means to assistive technology, as its builder methods
/// set it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Semantics {
    pub flags: SemFlags,
    pub text: Option<Box<SemText>>,
}

impl Semantics {
    /// Nothing said: the defaults apply.
    pub const NONE: Self = Self {
        flags: SemFlags {
            role: None,
            bits: 0,
        },
        text: None,
    };

    pub fn text_mut(&mut self) -> &mut SemText {
        self.text.get_or_insert_with(Box::default)
    }
}

/// What is painted behind a box's children.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Backdrop {
    /// One colour (with the opacity it is drawn at).
    Solid(Color),
    /// A gradient: not one colour, so nothing is concluded from it.
    Mixed,
}

/// What kind of element a note is about.
#[derive(Clone, Debug)]
pub(crate) enum RawKind {
    /// Not said yet (the element was entered and nothing more).
    Pending,
    Box {
        press: bool,
        block: bool,
        /// The input this box is the frame of.
        field: Option<u64>,
        /// Scroll position and range, in dp.
        scroll: Option<(f32, f32)>,
        fill: Option<Backdrop>,
        stack: bool,
        increment: bool,
        decrement: bool,
    },
    Text {
        content: String,
        /// The colours it is drawn in: its own, then its spans'.
        colors: Vec<Color>,
        /// Font size in dp.
        size: f32,
        bold: bool,
        /// No shadow and no outline: its contrast is its colour's.
        plain: bool,
    },
    /// A picture or an icon.
    Picture,
    Input {
        /// What it shows: the value, or bullets for a secure field.
        value: String,
        /// The placeholder.
        hint: String,
        color: Color,
        size: f32,
        bold: bool,
        secure: bool,
        multiline: bool,
        focused: bool,
        /// Anchor and caret in characters of `value`.
        selection: Option<(u32, u32)>,
    },
}

/// What painting noted about one element.
#[derive(Clone, Debug)]
pub(crate) struct RawNode {
    pub id: u64,
    parent: u32,
    /// One past the last note of its subtree.
    end: u32,
    /// Its bounding box on the screen (px), whether or not it can be seen.
    rect: Rect,
    /// What its clips leave of that.
    visible: Rect,
    /// Its laid-out size in dp, before any transform.
    size: (f32, f32),
    /// Drawn at an opacity of nothing.
    hidden: bool,
    /// Inside a group drawn through an opacity layer: its colours are not
    /// what reaches the screen.
    layered: bool,
    flags: SemFlags,
    label: Option<String>,
    hint: Option<String>,
    value: Option<String>,
    pub kind: RawKind,
}

const NONE: u32 = u32::MAX;

/// The frame of a text input (`Container::input_frame`) that is being
/// looked inside: a tap on it focuses the input, so it is the input's
/// bounds, and a label given to it is the input's label.
struct Frame {
    input: u64,
    visible: Rect,
    label: Option<String>,
}

/// A node on its way into the tree.
struct Built {
    node: SemanticsNode,
    raw: u32,
    children: Vec<Built>,
}

/// What the checks need to know about a node of the built tree.
#[derive(Clone, Copy, Debug)]
struct Meta {
    /// Its note; `NONE` for the root and the selection menu's items.
    raw: u32,
    /// Its laid-out size in dp.
    size: (f32, f32),
}

/// The notes of one frame, and the tree built from them.
pub(crate) struct Collector {
    nodes: Vec<RawNode>,
    open: Vec<u32>,
    /// Items of the selection menu: identity, label, rectangle.
    menu: Vec<(u64, &'static str, Rect)>,
    meta: Vec<Meta>,
    scale: f32,
    surface: Rect,
    background: Color,
    /// The product of the opacities of the layers being painted through.
    pub layer_opacity: f32,
    pub layers: u32,
}

/// What [`Collector::enter`] is told about an element.
pub(crate) struct Entered<'a> {
    pub id: u64,
    pub rect: Rect,
    pub visible: Rect,
    /// Laid-out size in dp.
    pub size: (f32, f32),
    pub alpha: f32,
    pub semantics: &'a Semantics,
}

impl Default for Collector {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            open: Vec::new(),
            menu: Vec::new(),
            meta: Vec::new(),
            scale: 1.0,
            surface: Rect::default(),
            background: Color::BLACK,
            layer_opacity: 1.0,
            layers: 0,
        }
    }
}

impl Collector {
    /// Starts the notes of a frame.
    pub fn begin(&mut self, scale: f32, surface: Rect, background: Color) {
        self.nodes.clear();
        self.open.clear();
        self.menu.clear();
        self.scale = scale;
        self.surface = surface;
        self.background = background;
        self.layer_opacity = 1.0;
        self.layers = 0;
    }

    /// Notes an element that is about to be painted. Everything noted
    /// until the matching [`leave`](Self::leave) is inside it.
    pub fn enter(&mut self, entered: Entered<'_>) {
        let index = self.nodes.len() as u32;
        let semantics = entered.semantics;
        let text = semantics.text.as_deref();
        let owned = |text: &Option<Cow<'static, str>>| text.as_ref().map(|t| t.to_string());
        self.nodes.push(RawNode {
            id: entered.id,
            parent: self.open.last().copied().unwrap_or(NONE),
            end: index + 1,
            rect: entered.rect,
            visible: entered.visible,
            size: entered.size,
            hidden: entered.alpha * self.layer_opacity <= 0.01,
            layered: self.layers > 0,
            flags: semantics.flags,
            label: text.and_then(|text| owned(&text.label)),
            hint: text.and_then(|text| owned(&text.hint)),
            value: text.and_then(|text| owned(&text.value)),
            kind: RawKind::Pending,
        });
        self.open.push(index);
    }

    /// Says what the element being painted is.
    pub fn describe(&mut self, kind: RawKind) {
        if let Some(node) = self
            .open
            .last()
            .and_then(|index| self.nodes.get_mut(*index as usize))
        {
            node.kind = kind;
        }
    }

    /// The element entered last has been painted, with all inside it.
    pub fn leave(&mut self) {
        let end = self.nodes.len() as u32;
        if let Some(node) = self
            .open
            .pop()
            .and_then(|index| self.nodes.get_mut(index as usize))
        {
            node.end = end;
        }
    }

    /// Notes an item of the selection menu, which is painted over
    /// everything and belongs to no element.
    pub fn menu_item(&mut self, id: u64, label: &'static str, rect: Rect) {
        self.menu.push((id, label, rect));
    }

    fn children(&self, index: u32) -> impl Iterator<Item = u32> + '_ {
        let end = self.nodes[index as usize].end;
        let mut next = index + 1;
        std::iter::from_fn(move || {
            if next >= end {
                return None;
            }
            let child = next;
            next = self.nodes[child as usize].end;
            Some(child)
        })
    }

    fn gone(&self, index: u32) -> bool {
        let node = &self.nodes[index as usize];
        node.hidden || node.flags.has(SemFlags::EXCLUDE)
    }

    /// True for a box that is a stop of its own inside a merged one.
    fn actionable(&self, index: u32) -> bool {
        match self.nodes[index as usize].kind {
            RawKind::Box {
                press,
                increment,
                decrement,
                ..
            } => press || increment || decrement,
            RawKind::Input { .. } => true,
            _ => false,
        }
    }

    /// The texts inside `index`, in order: what a merged node says when it
    /// was given no label.
    fn gather(&self, index: u32, parts: &mut Vec<String>) {
        for child in self.children(index) {
            if self.gone(child) {
                continue;
            }
            let node = &self.nodes[child as usize];
            let mut said = |text: &str| {
                let text = text.trim();
                if !text.is_empty() {
                    parts.push(text.to_owned());
                }
            };
            match &node.kind {
                RawKind::Text { content, .. } => {
                    said(node.label.as_deref().unwrap_or(content));
                    said(node.value.as_deref().unwrap_or(""));
                }
                RawKind::Picture => {
                    said(node.label.as_deref().unwrap_or(""));
                    said(node.value.as_deref().unwrap_or(""));
                }
                RawKind::Box { .. } if !self.actionable(child) => {
                    if node.label.is_some() || node.value.is_some() {
                        said(node.label.as_deref().unwrap_or(""));
                        said(node.value.as_deref().unwrap_or(""));
                    } else {
                        self.gather(child, parts);
                    }
                }
                _ => {}
            }
        }
    }

    fn states(flags: SemFlags) -> States {
        let mut states = States::NONE;
        let map = [
            (SemFlags::DISABLED, States::DISABLED),
            (SemFlags::SELECTED, States::SELECTED),
            (SemFlags::CHECKABLE, States::CHECKABLE),
            (SemFlags::CHECKED, States::CHECKED),
            (SemFlags::EXPANDABLE, States::EXPANDABLE),
            (SemFlags::EXPANDED, States::EXPANDED),
            (SemFlags::LIVE_POLITE, States::LIVE_POLITE),
            (SemFlags::LIVE_ASSERTIVE, States::LIVE_ASSERTIVE),
        ];
        for (flag, state) in map {
            if flags.has(flag) {
                states |= state;
            }
        }
        states
    }

    /// Turns the element `index` and what is inside it into nodes,
    /// appended to `out`. `frames` are the input frames around it.
    fn emit(&self, index: u32, out: &mut Vec<Built>, frames: &mut Vec<Frame>) {
        if self.gone(index) {
            return;
        }
        let raw = &self.nodes[index as usize];
        let flags = raw.flags;
        let leaf = |role: Role, label: String| Built {
            node: SemanticsNode {
                id: raw.id,
                bounds: raw.visible,
                role: flags.role.unwrap_or(role),
                label,
                value: raw.value.clone().unwrap_or_default(),
                hint: raw.hint.clone().unwrap_or_default(),
                states: Self::states(flags),
                ..SemanticsNode::default()
            },
            raw: index,
            children: Vec::new(),
        };
        match &raw.kind {
            RawKind::Pending => {}
            RawKind::Text { content, .. } => {
                let label = raw.label.clone().unwrap_or_else(|| content.clone());
                let says = !label.trim().is_empty() || raw.value.is_some();
                if says && !raw.visible.is_empty() {
                    out.push(leaf(Role::Text, label));
                }
            }
            RawKind::Picture => {
                // Decoration unless it was given something to say.
                if !raw.visible.is_empty() && (raw.label.is_some() || raw.value.is_some()) {
                    out.push(leaf(Role::Image, raw.label.clone().unwrap_or_default()));
                }
            }
            RawKind::Input {
                value,
                hint,
                secure,
                multiline,
                focused,
                selection,
                ..
            } => {
                // A field in a frame is as large as its frame: that is
                // what a tap focuses.
                let framed = frames.iter().rev().find(|frame| frame.input == raw.id);
                let bounds = framed.map_or(raw.visible, |frame| frame.visible);
                if bounds.is_empty() {
                    return;
                }
                let label = raw
                    .label
                    .clone()
                    .or_else(|| framed.and_then(|frame| frame.label.clone()))
                    .unwrap_or_default();
                let mut built = leaf(Role::TextField, label);
                let node = &mut built.node;
                node.bounds = bounds;
                node.value.clone_from(value);
                node.hint = match (&raw.hint, hint.is_empty()) {
                    (Some(given), true) => given.clone(),
                    (Some(given), false) => format!("{hint}, {given}"),
                    (None, _) => hint.clone(),
                };
                node.states = node
                    .states
                    .with(States::PASSWORD, *secure)
                    .with(States::MULTILINE, *multiline)
                    .with(States::FOCUSED, *focused);
                node.selection = *selection;
                if !flags.has(SemFlags::DISABLED) {
                    node.actions = Actions::ACTIVATE
                        | Actions::FOCUS
                        | Actions::LONG_PRESS
                        | Actions::SET_TEXT
                        | Actions::SET_SELECTION;
                }
                out.push(built);
            }
            RawKind::Box {
                press,
                field,
                scroll,
                stack,
                increment,
                decrement,
                ..
            } => {
                let acts = *press || *increment || *decrement;
                if let Some(input) = field.filter(|_| !acts) {
                    // The frame of a field says nothing itself: it lends
                    // the field its size and its label.
                    frames.push(Frame {
                        input,
                        visible: raw.visible,
                        label: raw.label.clone(),
                    });
                    self.emit_children(index, *stack, out, frames);
                    frames.pop();
                    return;
                }
                let said = raw.label.is_some() || raw.value.is_some();
                let explicit = said
                    || flags.role.is_some()
                    || flags.has(
                        SemFlags::MERGE
                            | SemFlags::SELECTABLE
                            | SemFlags::CHECKABLE
                            | SemFlags::EXPANDABLE
                            | SemFlags::LIVE_POLITE
                            | SemFlags::LIVE_ASSERTIVE,
                    );
                if !acts && (flags.has(SemFlags::GROUP) || scroll.is_some()) {
                    // A container: what is inside stays separate stops.
                    let mut children = Vec::new();
                    self.emit_children(index, *stack, &mut children, frames);
                    let label = raw.label.clone().unwrap_or_default();
                    if raw.visible.is_empty() || (children.is_empty() && label.is_empty()) {
                        return;
                    }
                    let mut built = leaf(
                        if scroll.is_some() {
                            Role::List
                        } else {
                            Role::Group
                        },
                        label,
                    );
                    if let Some((position, range)) = *scroll {
                        built.node.scroll = Some((position, range));
                        if position < range - 0.5 {
                            built.node.actions |= Actions::SCROLL_FORWARD;
                        }
                        if position > 0.5 {
                            built.node.actions |= Actions::SCROLL_BACKWARD;
                        }
                    }
                    built.children = children;
                    out.push(built);
                } else if acts || explicit {
                    // One stop: what is inside is read as its label, and
                    // only the controls inside stay stops of their own.
                    let label = raw.label.clone().unwrap_or_else(|| {
                        let mut parts = Vec::new();
                        self.gather(index, &mut parts);
                        parts.join(", ")
                    });
                    let mut nested = Vec::new();
                    self.nested(index, &mut nested, frames);
                    if raw.visible.is_empty() || (!acts && label.is_empty() && !said) {
                        // Nothing of it to land on, or nothing to say.
                        if !acts && !raw.visible.is_empty() {
                            self.emit_children(index, *stack, out, frames);
                        } else {
                            out.append(&mut nested);
                        }
                        return;
                    }
                    let role = if *press {
                        if flags.has(SemFlags::CHECKABLE) {
                            Role::Checkbox
                        } else {
                            Role::Button
                        }
                    } else if *increment || *decrement {
                        Role::Slider
                    } else if flags.has(SemFlags::CHECKABLE) {
                        Role::Checkbox
                    } else {
                        Role::Text
                    };
                    let mut built = leaf(role, label);
                    if !flags.has(SemFlags::DISABLED) {
                        built.node.actions = Actions::NONE
                            .with(Actions::ACTIVATE, *press)
                            .with(Actions::INCREMENT, *increment)
                            .with(Actions::DECREMENT, *decrement);
                    }
                    built.children = nested;
                    out.push(built);
                } else {
                    // Says nothing itself: what is inside it stands where
                    // it stands.
                    self.emit_children(index, *stack, out, frames);
                }
            }
        }
    }

    /// Emits what is inside the box `index`. The children of a row or a
    /// column are read in the order they are laid out in; those of a
    /// stack lie on one another, in the order they are painted, and are
    /// read top to bottom, then start to end: each child with all inside
    /// it, placed by where its first stop is.
    fn emit_children(
        &self,
        index: u32,
        stack: bool,
        out: &mut Vec<Built>,
        frames: &mut Vec<Frame>,
    ) {
        if !stack {
            for child in self.children(index) {
                self.emit(child, out, frames);
            }
            return;
        }
        let mut layers: Vec<Vec<Built>> = Vec::new();
        for child in self.children(index) {
            let mut layer = Vec::new();
            self.emit(child, &mut layer, frames);
            if !layer.is_empty() {
                layers.push(layer);
            }
        }
        // Stable: layers that start at the same place keep paint order.
        layers.sort_by(|a, b| {
            let (a, b) = (a[0].node.bounds, b[0].node.bounds);
            a.y.round()
                .total_cmp(&b.y.round())
                .then(a.x.round().total_cmp(&b.x.round()))
        });
        out.extend(layers.into_iter().flatten());
    }

    /// The controls inside the merged node `index`, which stay stops of
    /// their own.
    fn nested(&self, index: u32, out: &mut Vec<Built>, frames: &mut Vec<Frame>) {
        for child in self.children(index) {
            if self.gone(child) {
                continue;
            }
            if self.actionable(child) {
                self.emit(child, out, frames);
            } else {
                let framed = match self.nodes[child as usize].kind {
                    RawKind::Box { field, .. } => field,
                    _ => None,
                };
                if let Some(input) = framed {
                    let node = &self.nodes[child as usize];
                    frames.push(Frame {
                        input,
                        visible: node.visible,
                        label: node.label.clone(),
                    });
                }
                self.nested(child, out, frames);
                if framed.is_some() {
                    frames.pop();
                }
            }
        }
    }

    /// Builds the tree of the frame just painted into `tree`.
    pub fn build(&mut self, tree: &mut SemanticsTree, root: u64) {
        tree.clear();
        self.meta.clear();
        let mut top = Vec::new();
        let mut frames = Vec::new();
        if !self.nodes.is_empty() {
            self.emit(0, &mut top, &mut frames);
        }
        // What lies wholly under a box that swallows touches (the scrim
        // of a sheet or a dialog) cannot be reached by a finger and is
        // not offered to a screen reader either.
        let blockers: Vec<(u32, Rect)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                matches!(node.kind, RawKind::Box { block: true, .. })
                    && !node.hidden
                    && !node.visible.is_empty()
            })
            .map(|(index, node)| (index as u32, node.visible))
            .collect();
        if !blockers.is_empty() {
            self.uncover(&mut top, &blockers);
        }
        for (id, label, rect) in &self.menu {
            top.push(Built {
                node: SemanticsNode {
                    id: *id,
                    bounds: *rect,
                    role: Role::Button,
                    label: (*label).to_owned(),
                    actions: Actions::ACTIVATE,
                    ..SemanticsNode::default()
                },
                raw: NONE,
                children: Vec::new(),
            });
        }

        let at = tree.push(
            None,
            SemanticsNode {
                // Never a widget's identity: those of paths have the low
                // bit clear and explicit ones are hashes.
                id: root ^ 0xA11C_E550_0000_0001,
                bounds: self.surface,
                role: Role::Group,
                actions: Actions::DISMISS,
                ..SemanticsNode::default()
            },
        );
        self.meta.push(Meta {
            raw: NONE,
            size: (0.0, 0.0),
        });
        let mut seen = HashSet::new();
        self.flatten(tree, at, top, &mut seen);
    }

    fn uncover(&self, nodes: &mut Vec<Built>, blockers: &[(u32, Rect)]) {
        nodes.retain(|built| {
            built.raw == NONE
                || !blockers.iter().any(|(blocker, rect)| {
                    // Painted after the node and all inside it.
                    *blocker >= self.nodes[built.raw as usize].end
                        && contains(*rect, built.node.bounds, 0.5)
                })
        });
        for built in nodes {
            self.uncover(&mut built.children, blockers);
        }
    }

    fn flatten(
        &mut self,
        tree: &mut SemanticsTree,
        parent: usize,
        nodes: Vec<Built>,
        seen: &mut HashSet<u64>,
    ) {
        for (position, mut built) in nodes.into_iter().enumerate() {
            // Two widgets given the same explicit id would be one node to
            // the host: the later one is told apart.
            let mut salt = position as u64;
            while !seen.insert(built.node.id) {
                salt += 1;
                built.node.id ^= salt.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 2;
            }
            let size = if built.raw == NONE {
                (
                    built.node.bounds.width / self.scale,
                    built.node.bounds.height / self.scale,
                )
            } else {
                self.target_size(built.raw)
            };
            self.meta.push(Meta {
                raw: built.raw,
                size,
            });
            let at = tree.push(Some(parent), built.node);
            self.flatten(tree, at, built.children, seen);
        }
    }

    /// The size (dp) of what a tap on the element `index` can land on: its
    /// own box, or for an input its frame.
    fn target_size(&self, index: u32) -> (f32, f32) {
        let node = &self.nodes[index as usize];
        if matches!(node.kind, RawKind::Input { .. }) {
            let mut above = node.parent;
            while above != NONE {
                let outer = &self.nodes[above as usize];
                if matches!(outer.kind, RawKind::Box { field: Some(input), press: false, .. } if input == node.id)
                {
                    return outer.size;
                }
                above = outer.parent;
            }
        }
        node.size
    }

    // ------------------------------------------------------------ checks

    /// What is wrong with the tree last built, for a screen reader's user
    /// or one who sees or aims poorly.
    pub fn issues(&self, tree: &SemanticsTree) -> Vec<AccessibilityIssue> {
        let mut issues = Vec::new();
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        let in_dp = |rect: Rect| {
            Rect::new(
                rect.x / scale,
                rect.y / scale,
                rect.width / scale,
                rect.height / scale,
            )
        };
        let operable =
            Actions::ACTIVATE.0 | Actions::FOCUS.0 | Actions::INCREMENT.0 | Actions::DECREMENT.0;
        for (index, node) in tree.nodes().iter().enumerate() {
            let Some(meta) = self.meta.get(index) else {
                break;
            };
            let acts = node.actions.0 & operable != 0
                || (node.is(States::DISABLED)
                    && matches!(
                        node.role,
                        Role::Button
                            | Role::Link
                            | Role::Tab
                            | Role::Checkbox
                            | Role::Switch
                            | Role::Radio
                            | Role::TextField
                            | Role::Slider
                    ));
            if !acts {
                continue;
            }
            let named = !node.label.trim().is_empty()
                || (node.role == Role::TextField && !node.hint.trim().is_empty());
            if !named {
                issues.push(AccessibilityIssue {
                    kind: IssueKind::MissingLabel,
                    id: node.id,
                    label: String::new(),
                    bounds: in_dp(node.bounds),
                    detail: format!(
                        "a {:?} with nothing to call it: give it .label(\"...\")",
                        node.role
                    ),
                });
            }
            let (width, height) = meta.size;
            if meta.raw != NONE
                && (width < MIN_TARGET - 0.01 || height < MIN_TARGET - 0.01)
                && !node.is(States::DISABLED)
            {
                issues.push(AccessibilityIssue {
                    kind: IssueKind::SmallTarget,
                    id: node.id,
                    label: node.label.clone(),
                    bounds: in_dp(node.bounds),
                    detail: format!(
                        "touch target {} x {} dp, smaller than {MIN_TARGET} x {MIN_TARGET}",
                        trim(width),
                        trim(height)
                    ),
                });
            }
        }
        // Two controls side by side that are called the same cannot be
        // told apart by ear.
        for (index, _) in tree.nodes().iter().enumerate() {
            let mut heard: Vec<(Role, &str, &str)> = Vec::new();
            for child in tree.children(index) {
                let node = &tree.nodes()[child];
                if node.actions.0 & operable == 0 || node.label.trim().is_empty() {
                    continue;
                }
                let key = (node.role, node.label.as_str(), node.value.as_str());
                if heard.contains(&key) {
                    issues.push(AccessibilityIssue {
                        kind: IssueKind::DuplicateLabel,
                        id: node.id,
                        label: node.label.clone(),
                        bounds: in_dp(node.bounds),
                        detail: "another control beside it has the same label".to_owned(),
                    });
                } else {
                    heard.push(key);
                }
            }
        }
        self.contrast(&mut issues, scale);
        issues
    }

    /// Text whose colour is too close to the one solid colour known to be
    /// behind it.
    fn contrast(&self, issues: &mut Vec<AccessibilityIssue>, scale: f32) {
        // Elements out of a screen reader's sight, or disabled, with all
        // inside them, are skipped: `skip_until` is where that ends.
        let mut skip_until = 0u32;
        for (index, node) in self.nodes.iter().enumerate() {
            let index = index as u32;
            if index < skip_until {
                continue;
            }
            if node.hidden || node.flags.has(SemFlags::EXCLUDE | SemFlags::DISABLED) {
                skip_until = node.end;
                continue;
            }
            if node.visible.is_empty() || node.layered {
                continue;
            }
            let (colors, size, bold, label): (&[Color], f32, bool, &str) = match &node.kind {
                RawKind::Text {
                    content,
                    colors,
                    size,
                    bold,
                    plain: true,
                } if !content.trim().is_empty() => (colors, *size, *bold, content),
                RawKind::Input {
                    value,
                    hint,
                    color,
                    size,
                    bold,
                    ..
                } if !(value.is_empty() && hint.is_empty()) => (
                    std::slice::from_ref(color),
                    *size,
                    *bold,
                    if value.is_empty() { hint } else { value },
                ),
                _ => continue,
            };
            let Some(behind) = self.behind(index) else {
                continue;
            };
            let large = size >= LARGE_TEXT || (bold && size >= LARGE_BOLD_TEXT);
            let needed = if large { 3.0 } else { 4.5 };
            let worst = colors
                .iter()
                .filter(|color| color.a > 0.01)
                .map(|color| (contrast_ratio(over(*color, behind), behind), *color))
                .min_by(|a, b| a.0.total_cmp(&b.0));
            let Some((ratio, color)) = worst else {
                continue;
            };
            // Compared as it is reported, to two decimals.
            if (ratio * 100.0).round() / 100.0 >= needed {
                continue;
            }
            let secure = matches!(node.kind, RawKind::Input { secure: true, .. });
            issues.push(AccessibilityIssue {
                kind: IssueKind::LowContrast,
                id: node.id,
                label: if secure {
                    String::new()
                } else {
                    label.to_owned()
                },
                bounds: Rect::new(
                    node.rect.x / scale,
                    node.rect.y / scale,
                    node.rect.width / scale,
                    node.rect.height / scale,
                ),
                detail: format!(
                    "contrast {ratio:.2}:1 of {} on {}, below {needed}:1 for {} dp{} text",
                    hex(over(color, behind)),
                    hex(behind),
                    trim(size),
                    if bold { " bold" } else { "" },
                ),
            });
        }
    }

    /// The one solid, opaque colour behind the element `index`, if that is
    /// what is behind it: its nearest filled ancestor, or the screen's
    /// background, with nothing else painted in between. `None` when
    /// something else is (a gradient, a picture, a sibling drawn under
    /// it), or when it is not known.
    fn behind(&self, index: u32) -> Option<Color> {
        let target = self.nodes[index as usize].rect;
        let mut child = index;
        let mut above = self.nodes[index as usize].parent;
        while above != NONE {
            // What the parent painted before this branch, under it.
            let mut under = None;
            for sibling in self.children(above) {
                if sibling >= child {
                    break;
                }
                let node = &self.nodes[sibling as usize];
                if !node.hidden && overlaps(node.rect, target) && self.paints(sibling) {
                    under = Some(sibling);
                }
            }
            if let Some(sibling) = under {
                let node = &self.nodes[sibling as usize];
                return match node.kind {
                    RawKind::Box {
                        fill: Some(Backdrop::Solid(color)),
                        ..
                    } if color.a >= 0.999
                        && contains(node.rect, target, 1.0)
                        && !self.children(sibling).any(|inner| self.paints(inner)) =>
                    {
                        Some(color)
                    }
                    _ => None,
                };
            }
            let parent = &self.nodes[above as usize];
            if parent.layered && !self.nodes[index as usize].layered {
                return None;
            }
            if let RawKind::Box {
                fill: Some(fill), ..
            } = parent.kind
            {
                let Backdrop::Solid(color) = fill else {
                    return None;
                };
                if !contains(parent.rect, target, 1.0) {
                    return None;
                }
                if color.a >= 0.999 {
                    return Some(color);
                }
                if color.a > 0.01 {
                    // See-through: over whatever is behind the box.
                    return self.behind(above).map(|below| over(color, below));
                }
            }
            child = above;
            above = parent.parent;
        }
        Some(self.background)
    }

    /// True if the element `index`, or anything inside it, puts paint on
    /// the screen.
    fn paints(&self, index: u32) -> bool {
        let node = &self.nodes[index as usize];
        if node.hidden {
            return false;
        }
        match node.kind {
            RawKind::Box { fill, .. } => {
                fill.is_some() || self.children(index).any(|child| self.paints(child))
            }
            RawKind::Pending => false,
            _ => true,
        }
    }
}

/// The smallest side, in dp, of something meant to be tapped.
const MIN_TARGET: f32 = 48.0;
/// Text this large (dp) needs a contrast of 3:1 rather than 4.5:1: WCAG's
/// 18 pt, and 14 pt in bold, at 96 dpi.
const LARGE_TEXT: f32 = 24.0;
const LARGE_BOLD_TEXT: f32 = 18.66;

fn contains(outer: Rect, inner: Rect, slack: f32) -> bool {
    inner.x >= outer.x - slack
        && inner.y >= outer.y - slack
        && inner.x + inner.width <= outer.x + outer.width + slack
        && inner.y + inner.height <= outer.y + outer.height + slack
}

fn overlaps(a: Rect, b: Rect) -> bool {
    let width = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let height = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    width > 0.5 && height > 0.5
}

/// `top` over the opaque `bottom`, composited as the renderer does it: in
/// the encoded space (ADR 0023).
fn over(top: Color, bottom: Color) -> Color {
    if top.a >= 0.999 {
        return Color::rgb(top.r, top.g, top.b);
    }
    let (top_srgb, bottom_srgb) = (top.srgb(), bottom.srgb());
    let mix = |index: usize| {
        let encoded = top_srgb[index] * top.a + bottom_srgb[index] * (1.0 - top.a);
        if encoded <= 0.04045 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    };
    Color::rgb(mix(0), mix(1), mix(2))
}

/// WCAG's relative luminance; the components are already linear.
fn luminance(color: Color) -> f32 {
    0.2126 * color.r.clamp(0.0, 1.0)
        + 0.7152 * color.g.clamp(0.0, 1.0)
        + 0.0722 * color.b.clamp(0.0, 1.0)
}

/// WCAG's contrast ratio of two opaque colours: 1 to 21.
pub(crate) fn contrast_ratio(a: Color, b: Color) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn hex(color: Color) -> String {
    let [r, g, b, _] = color.srgb();
    let byte = |value: f32| (value * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b))
}

/// A length without a trailing ".0".
fn trim(value: f32) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

/// What kind of problem an [`AccessibilityIssue`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IssueKind {
    /// A control with nothing to call it: an icon-only button without a
    /// `.label(..)`, a text field with neither label nor placeholder. A
    /// screen reader says only "button".
    MissingLabel,
    /// A control smaller than 48 x 48 dp: hard to hit, and hard to find by
    /// touch.
    SmallTarget,
    /// Two controls beside each other with the same label: they cannot be
    /// told apart by ear.
    DuplicateLabel,
    /// Text whose contrast with the solid colour behind it is under 4.5:1
    /// (3:1 for text of 24 dp, or 18.66 dp bold, and larger). Only
    /// reported where both colours are known.
    LowContrast,
}

/// Something [`Ui::accessibility_issues`](crate::Ui::accessibility_issues)
/// found.
#[derive(Clone, Debug, PartialEq)]
pub struct AccessibilityIssue {
    /// What is wrong.
    pub kind: IssueKind,
    /// The identity of the widget.
    pub id: u64,
    /// Its label, or the text itself for a contrast finding; empty when it
    /// has none (and for a password field).
    pub label: String,
    /// Where it is, in dp.
    pub bounds: Rect,
    /// The finding in words, with its numbers.
    pub detail: String,
}

impl std::fmt::Display for AccessibilityIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            IssueKind::MissingLabel => "missing label",
            IssueKind::SmallTarget => "small target",
            IssueKind::DuplicateLabel => "duplicate label",
            IssueKind::LowContrast => "low contrast",
        };
        write!(
            f,
            "{kind}: {:?} at ({}, {}) {} x {} dp: {}",
            self.label,
            trim(self.bounds.x),
            trim(self.bounds.y),
            trim(self.bounds.width),
            trim(self.bounds.height),
            self.detail
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_is_wcags() {
        let white = Color::WHITE;
        let black = Color::BLACK;
        assert!((contrast_ratio(white, black) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(white, white) - 1.0).abs() < 0.001);
        // #767676 on white is the well-known 4.54:1.
        let grey = crate::hex(0x767676);
        assert!((contrast_ratio(grey, white) - 4.54).abs() < 0.02);
    }

    #[test]
    fn a_see_through_colour_is_judged_as_it_lands() {
        // Half-transparent white over black is the grey #808080.
        let landed = over(Color::rgba(1.0, 1.0, 1.0, 0.5), Color::BLACK);
        assert_eq!(hex(landed), "#808080");
    }
}

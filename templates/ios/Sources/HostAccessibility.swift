import Foundation
import UIKit

// NEVER COMPILED. This file was written on a machine without Xcode, against
// the C interface in morningstar_ios.h and from the documentation of
// UIAccessibility. It has been parsed (swiftc -parse) and nothing more: no
// type has been checked, no line has run, and VoiceOver has never read a
// Morningstar screen. Expect to fix it the first time it is built. See
// docs/architecture/0032-accessibility.md, "Not verified".

/// One node of the application's semantics tree, as last sent.
final class SemanticsNode {
    var id: Int32 = 0
    var parent: Int32 = -1
    var role: Int = 0
    var states: Int = 0
    var actions: Int = 0
    /// In pixels of the layer.
    var bounds: CGRect = .zero
    /// In points; both negative if the node does not scroll.
    var scrollPosition: CGFloat = -1
    var scrollRange: CGFloat = -1
    var children: [Int32] = []
    var label = ""
    var value = ""
    var hint = ""

    func has(_ state: Int) -> Bool { states & state != 0 }
    /// True if the action with this code (MORNINGSTAR_A11Y_*) is offered.
    func can(_ action: Int) -> Bool { actions & (1 << action) != 0 }
}

/// What VoiceOver sees of a Morningstar screen (ADR 0032).
///
/// The application draws its own pixels; this class keeps the tree of
/// nodes Rust derives from its widgets (`morningstar_ios_take_semantics`)
/// and vends one `UIAccessibilityElement` per node that says something,
/// in reading order, as the `accessibilityElements` of the Metal view.
/// Actions (a double tap, a three-finger scroll, a swipe up on an
/// adjustable) go back through `perform`.
///
/// A password field's node carries bullets, never its characters: Rust
/// does not send them.
final class HostAccessibility {
    weak var view: UIView?
    /// Sends an action to the engine: (node, MORNINGSTAR_A11Y_* code,
    /// text). Returns whether the application carried it out.
    var perform: ((Int32, Int, String?) -> Bool)?

    private var nodes: [Int32: SemanticsNode] = [:]
    /// Elements are kept by node id so that VoiceOver's focus, which is on
    /// an element object, survives an update.
    private var elements: [Int32: NodeElement] = [:]
    /// The elements in reading order; nil when it must be worked out again.
    private var ordered: [NodeElement]?

    /// Forgets the tree: accessibility went off.
    func reset() {
        nodes.removeAll()
        elements.removeAll()
        ordered = nil
    }

    func node(_ id: Int32) -> SemanticsNode? { nodes[id] }

    /// Applies one update (the format is described in morningstar_ios.h)
    /// and tells VoiceOver what changed.
    func apply(_ bytes: UnsafeBufferPointer<UInt8>) {
        var reader = Reader(bytes: bytes)
        guard let version = reader.uint32(), Int(version) == MORNINGSTAR_A11Y_VERSION,
            let flags = reader.uint32(),
            reader.int32() != nil,  // The input focus: the keyboard follows it by itself.
            let removedCount = reader.uint32()
        else { return }
        var structure = false
        var screen = false
        if flags & 1 != 0 {
            reset()
            screen = true
        }
        for _ in 0..<removedCount {
            guard let id = reader.int32() else { return }
            nodes[id] = nil
            elements[id] = nil
            structure = true
        }
        guard let count = reader.uint32() else { return }
        var announcements: [(String, Bool)] = []
        var scrolled = false
        for _ in 0..<count {
            let node = SemanticsNode()
            guard let id = reader.int32(), let parent = reader.int32(),
                let role = reader.uint8(), let changed = reader.uint8(),
                let states = reader.uint16(), let actions = reader.uint16(),
                reader.uint16() != nil,
                let left = reader.float(), let top = reader.float(),
                let right = reader.float(), let bottom = reader.float(),
                let position = reader.float(), let range = reader.float(),
                reader.int32() != nil, reader.int32() != nil,  // The selection: not used.
                let childCount = reader.uint32()
            else { return }
            node.id = id
            node.parent = parent
            node.role = Int(role)
            node.states = Int(states)
            node.actions = Int(actions)
            node.bounds = CGRect(
                x: CGFloat(left), y: CGFloat(top),
                width: CGFloat(right - left), height: CGFloat(bottom - top))
            node.scrollPosition = CGFloat(position)
            node.scrollRange = CGFloat(range)
            for _ in 0..<childCount {
                guard let child = reader.int32() else { return }
                node.children.append(child)
            }
            guard let label = reader.string(), let value = reader.string(),
                let hint = reader.string()
            else { return }
            node.label = label
            node.value = value
            node.hint = hint
            nodes[id] = node

            let change = Int(changed)
            if change & (MORNINGSTAR_CHANGED_ADDED | MORNINGSTAR_CHANGED_CHILDREN) != 0 {
                structure = true
            }
            if change & MORNINGSTAR_CHANGED_SCROLL != 0 {
                scrolled = true
            }
            // A live region: iOS has no such thing on an element, so what
            // it now says is announced.
            let live = node.has(MORNINGSTAR_STATE_LIVE_POLITE | MORNINGSTAR_STATE_LIVE_ASSERTIVE)
            // Changed, or just appeared (a toast) on a screen that was
            // already there.
            let news =
                change & MORNINGSTAR_CHANGED_ADDED != 0
                ? !screen
                : change & (MORNINGSTAR_CHANGED_LABEL | MORNINGSTAR_CHANGED_VALUE) != 0
            if live, news {
                announcements.append(
                    (HostAccessibility.spoken(node), node.has(MORNINGSTAR_STATE_LIVE_ASSERTIVE)))
            }
        }
        ordered = nil
        if screen {
            UIAccessibility.post(notification: .screenChanged, argument: nil)
        } else if scrolled {
            UIAccessibility.post(notification: .pageScrolled, argument: nil)
        } else if structure {
            UIAccessibility.post(notification: .layoutChanged, argument: nil)
        }
        for (text, assertive) in announcements {
            HostAccessibility.announce(text, assertive: assertive)
        }
    }

    /// Has VoiceOver say something: at once, or when it has finished.
    static func announce(_ text: String, assertive: Bool) {
        guard UIAccessibility.isVoiceOverRunning, !text.isEmpty else { return }
        let argument = NSAttributedString(
            string: text,
            attributes: [.accessibilitySpeechQueueAnnouncement: !assertive])
        UIAccessibility.post(notification: .announcement, argument: argument)
    }

    static func spoken(_ node: SemanticsNode) -> String {
        node.value.isEmpty ? node.label : (node.label.isEmpty ? node.value : "\(node.label), \(node.value)")
    }

    /// The elements VoiceOver steps through: every node that says or does
    /// something, in the tree's order, which is reading order. Containers
    /// that say nothing (an unlabelled list) are not stops; their scrolling
    /// is reached from the elements inside them.
    func accessibilityElements() -> [Any] {
        if let ordered = ordered { return ordered }
        var list: [NodeElement] = []
        func visit(_ id: Int32, _ depth: Int) {
            guard depth < 64, let node = nodes[id] else { return }
            let container =
                node.role == MORNINGSTAR_ROLE_LIST || node.role == MORNINGSTAR_ROLE_GROUP
            let says = !node.label.isEmpty || !node.value.isEmpty
            let acts = node.actions & ~((1 << MORNINGSTAR_A11Y_SCROLL_FORWARD)
                | (1 << MORNINGSTAR_A11Y_SCROLL_BACKWARD) | (1 << MORNINGSTAR_A11Y_DISMISS)) != 0
            if id != 0, (says && !(container && node.label.isEmpty)) || acts {
                let element: NodeElement
                if let existing = elements[id] {
                    element = existing
                } else if let view = view {
                    element = NodeElement(owner: self, id: id, container: view)
                    elements[id] = element
                } else {
                    return
                }
                list.append(element)
            }
            for child in node.children { visit(child, depth + 1) }
        }
        visit(0, 0)
        ordered = list
        return list
    }

    /// The nearest node at or above `id` that offers the action `code`.
    func nearest(_ id: Int32, offering code: Int) -> Int32? {
        var at = id
        var steps = 0
        while let node = nodes[at], steps < 64 {
            if node.can(code) { return at }
            if node.parent < 0 { return nil }
            at = node.parent
            steps += 1
        }
        return nil
    }

    /// Reads the little-endian update.
    private struct Reader {
        let bytes: UnsafeBufferPointer<UInt8>
        var at = 0

        init(bytes: UnsafeBufferPointer<UInt8>) { self.bytes = bytes }

        mutating func uint8() -> UInt8? {
            guard at + 1 <= bytes.count else { return nil }
            defer { at += 1 }
            return bytes[at]
        }

        mutating func uint16() -> UInt16? {
            guard at + 2 <= bytes.count else { return nil }
            defer { at += 2 }
            return UInt16(bytes[at]) | UInt16(bytes[at + 1]) << 8
        }

        mutating func uint32() -> UInt32? {
            guard at + 4 <= bytes.count else { return nil }
            defer { at += 4 }
            return UInt32(bytes[at]) | UInt32(bytes[at + 1]) << 8 | UInt32(bytes[at + 2]) << 16
                | UInt32(bytes[at + 3]) << 24
        }

        mutating func int32() -> Int32? {
            uint32().map { Int32(bitPattern: $0) }
        }

        mutating func float() -> Float? {
            uint32().map { Float(bitPattern: $0) }
        }

        mutating func string() -> String? {
            guard let length = uint32().map({ Int($0) }), at + length <= bytes.count else {
                return nil
            }
            defer { at += length }
            return String(decoding: UnsafeBufferPointer(rebasing: bytes[at..<(at + length)]), as: UTF8.self)
        }
    }
}

/// One node, as VoiceOver meets it. Everything is read from the tree when
/// VoiceOver asks, so an element is never out of date.
final class NodeElement: UIAccessibilityElement {
    private weak var owner: HostAccessibility?
    private let id: Int32

    init(owner: HostAccessibility, id: Int32, container: UIView) {
        self.owner = owner
        self.id = id
        super.init(accessibilityContainer: container)
    }

    private var node: SemanticsNode? { owner?.node(id) }

    override var isAccessibilityElement: Bool {
        get { true }
        set {}
    }

    override var accessibilityLabel: String? {
        get {
            guard let node = node else { return nil }
            // A field is called by its label; with none, by its placeholder.
            if node.role == MORNINGSTAR_ROLE_TEXT_FIELD, node.label.isEmpty { return node.hint }
            return node.label
        }
        set {}
    }

    override var accessibilityValue: String? {
        get {
            guard let node = node else { return nil }
            if !node.value.isEmpty { return node.value }
            if node.has(MORNINGSTAR_STATE_CHECKABLE) {
                return node.has(MORNINGSTAR_STATE_CHECKED) ? "1" : "0"
            }
            if node.has(MORNINGSTAR_STATE_EXPANDABLE) {
                return node.has(MORNINGSTAR_STATE_EXPANDED) ? "expanded" : "collapsed"
            }
            return nil
        }
        set {}
    }

    override var accessibilityHint: String? {
        get {
            guard let node = node else { return nil }
            if node.role == MORNINGSTAR_ROLE_TEXT_FIELD, node.label.isEmpty { return nil }
            return node.hint.isEmpty ? nil : node.hint
        }
        set {}
    }

    override var accessibilityTraits: UIAccessibilityTraits {
        get {
            guard let node = node else { return [] }
            var traits: UIAccessibilityTraits = []
            switch node.role {
            case MORNINGSTAR_ROLE_TEXT: traits.insert(.staticText)
            case MORNINGSTAR_ROLE_HEADING: traits.insert(.header)
            case MORNINGSTAR_ROLE_BUTTON, MORNINGSTAR_ROLE_TAB, MORNINGSTAR_ROLE_RADIO:
                traits.insert(.button)
            case MORNINGSTAR_ROLE_LINK: traits.insert(.link)
            case MORNINGSTAR_ROLE_IMAGE: traits.insert(.image)
            case MORNINGSTAR_ROLE_CHECKBOX, MORNINGSTAR_ROLE_SWITCH:
                traits.insert(.button)
                if #available(iOS 17.0, *) { traits.insert(.toggleButton) }
            case MORNINGSTAR_ROLE_PROGRESS: traits.insert(.updatesFrequently)
            case MORNINGSTAR_ROLE_SLIDER: traits.insert(.adjustable)
            default: break
            }
            if node.has(MORNINGSTAR_STATE_DISABLED) { traits.insert(.notEnabled) }
            if node.has(MORNINGSTAR_STATE_SELECTED) { traits.insert(.selected) }
            return traits
        }
        set {}
    }

    /// Rust's bounds are pixels of the layer; VoiceOver wants points of
    /// the screen.
    override var accessibilityFrame: CGRect {
        get {
            guard let node = node, let view = owner?.view else { return .zero }
            let scale = view.contentScaleFactor
            let inView = CGRect(
                x: node.bounds.minX / scale, y: node.bounds.minY / scale,
                width: node.bounds.width / scale, height: node.bounds.height / scale)
            return UIAccessibility.convertToScreenCoordinates(inView, in: view)
        }
        set {}
    }

    /// A double tap. Returning false has VoiceOver send a tap to the
    /// middle of the frame instead, which reaches the same widget.
    override func accessibilityActivate() -> Bool {
        guard let node = node, node.can(MORNINGSTAR_A11Y_ACTIVATE) else { return false }
        return owner?.perform?(id, MORNINGSTAR_A11Y_ACTIVATE, nil) ?? false
    }

    /// A three-finger swipe: scrolls the list this node is in. Swiping up
    /// moves towards the end, which UIKit calls scrolling down.
    override func accessibilityScroll(_ direction: UIAccessibilityScrollDirection) -> Bool {
        let code: Int
        switch direction {
        case .down, .next: code = MORNINGSTAR_A11Y_SCROLL_FORWARD
        case .up, .previous: code = MORNINGSTAR_A11Y_SCROLL_BACKWARD
        default: return false
        }
        guard let owner = owner, let list = owner.nearest(id, offering: code) else { return false }
        return owner.perform?(list, code, nil) ?? false
    }

    override func accessibilityIncrement() {
        _ = owner?.perform?(id, MORNINGSTAR_A11Y_INCREMENT, nil)
    }

    override func accessibilityDecrement() {
        _ = owner?.perform?(id, MORNINGSTAR_A11Y_DECREMENT, nil)
    }

    /// The two-finger scrub: Back.
    override func accessibilityPerformEscape() -> Bool {
        owner?.perform?(0, MORNINGSTAR_A11Y_DISMISS, nil) ?? false
    }
}

import QuartzCore
import UIKit

/// A view whose backing layer is the CAMetalLayer Morningstar renders into.
///
/// It is also what the keyboard types into. While a text field of the
/// application has focus the view is first responder and, through
/// `UIKeyInput`, receives inserted text and the delete key, which it hands
/// on. `UITextInputTraits` (the properties below) tells the keyboard what
/// kind to be. Only `UIKeyInput` is adopted, not `UITextInput`: there is no
/// marked text, so input methods that compose (Chinese, Japanese, Korean)
/// and autocorrection's replacement of earlier words do not work.
final class MetalView: UIView, UIKeyInput {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    /// Receives (phase, pointer id, x, y) with coordinates in physical pixels.
    var onPointer: ((UInt32, UInt64, Float, Float) -> Void)?

    /// Text typed on the keyboard ("\n" for the Return key).
    var onInsertText: ((String) -> Void)?
    /// The delete key.
    var onDeleteBackward: (() -> Void)?
    /// The view stopped being first responder without `endEditing()`.
    var onFocusLost: (() -> Void)?

    /// True while a field of the application has focus. Only then may the
    /// view become first responder, so a stray tap never raises a keyboard.
    private var editing = false
    /// True while `endEditing()` is resigning on the application's behalf.
    private var resigningOnRequest = false
    /// Whether the focused field has text: UIKit asks through `hasText`.
    var fieldHasText = false

    /// What VoiceOver sees of the view (ADR 0032): one element per node
    /// of the application's semantics tree. NOT COMPILED.
    var accessibility: HostAccessibility?

    // The view itself is a container, not an element: with VoiceOver off
    // the tree is empty and there is nothing to step through.
    override var isAccessibilityElement: Bool {
        get { false }
        set {}
    }

    override var accessibilityElements: [Any]? {
        get { accessibility?.accessibilityElements() }
        set {}
    }

    // UITextInputTraits. UIKit reads these when the view becomes first
    // responder and on reloadInputViews().
    var keyboardType: UIKeyboardType = .default
    var autocapitalizationType: UITextAutocapitalizationType = .none
    var autocorrectionType: UITextAutocorrectionType = .default
    var spellCheckingType: UITextSpellCheckingType = .default
    var returnKeyType: UIReturnKeyType = .default
    var isSecureTextEntry: Bool = false

    // UIKeyInput.
    var hasText: Bool { fieldHasText }

    func insertText(_ text: String) {
        onInsertText?(text)
    }

    func deleteBackward() {
        onDeleteBackward?()
    }

    // The standard edit actions (ADR 0025): a hardware keyboard's Cmd+A,
    // C, X, V, and anything else that sends them up the responder chain.
    // The application's field does them; the selection, its handles and
    // its menu are drawn by the application, not by UIKit. NOT COMPILED.
    var onEditCommand: ((UInt32) -> Void)?
    var onCaretMove: ((UInt32, Bool) -> Void)?

    override func selectAll(_ sender: Any?) {
        onEditCommand?(UInt32(MORNINGSTAR_EDIT_SELECT_ALL))
    }

    override func copy(_ sender: Any?) {
        onEditCommand?(UInt32(MORNINGSTAR_EDIT_COPY))
    }

    override func cut(_ sender: Any?) {
        onEditCommand?(UInt32(MORNINGSTAR_EDIT_CUT))
    }

    override func paste(_ sender: Any?) {
        onEditCommand?(UInt32(MORNINGSTAR_EDIT_PASTE))
    }

    @objc private func undoEdit() {
        onEditCommand?(UInt32(MORNINGSTAR_EDIT_UNDO))
    }

    @objc private func redoEdit() {
        onEditCommand?(UInt32(MORNINGSTAR_EDIT_REDO))
    }

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        switch action {
        case #selector(selectAll(_:)), #selector(copy(_:)), #selector(cut(_:)),
            #selector(paste(_:)):
            return editing
        default:
            return super.canPerformAction(action, withSender: sender)
        }
    }

    // Arrow keys of a hardware keyboard: alone by characters and lines,
    // with Option by words, with Command to the ends; Shift selects.
    override var keyCommands: [UIKeyCommand]? {
        guard editing else { return nil }
        var commands: [UIKeyCommand] = []
        let arrows = [
            UIKeyCommand.inputLeftArrow, UIKeyCommand.inputRightArrow,
            UIKeyCommand.inputUpArrow, UIKeyCommand.inputDownArrow,
        ]
        for input in arrows {
            for flags: UIKeyModifierFlags in [
                [], .shift, .alternate, [.alternate, .shift], .command, [.command, .shift],
            ] {
                commands.append(
                    UIKeyCommand(
                        input: input, modifierFlags: flags, action: #selector(arrowKey(_:))))
            }
        }
        commands.append(
            UIKeyCommand(input: "z", modifierFlags: .command, action: #selector(undoEdit)))
        commands.append(
            UIKeyCommand(
                input: "z", modifierFlags: [.command, .shift], action: #selector(redoEdit)))
        return commands
    }

    @objc private func arrowKey(_ command: UIKeyCommand) {
        let flags = command.modifierFlags
        let words = flags.contains(.alternate)
        let ends = flags.contains(.command)
        let move: Int
        switch command.input {
        case UIKeyCommand.inputLeftArrow:
            move =
                ends
                ? MORNINGSTAR_MOVE_LINE_START
                : (words ? MORNINGSTAR_MOVE_WORD_LEFT : MORNINGSTAR_MOVE_LEFT)
        case UIKeyCommand.inputRightArrow:
            move =
                ends
                ? MORNINGSTAR_MOVE_LINE_END
                : (words ? MORNINGSTAR_MOVE_WORD_RIGHT : MORNINGSTAR_MOVE_RIGHT)
        case UIKeyCommand.inputUpArrow:
            move = ends ? MORNINGSTAR_MOVE_TEXT_START : MORNINGSTAR_MOVE_UP
        default:
            move = ends ? MORNINGSTAR_MOVE_TEXT_END : MORNINGSTAR_MOVE_DOWN
        }
        onCaretMove?(UInt32(move), flags.contains(.shift))
    }

    override var canBecomeFirstResponder: Bool { editing }

    /// Shows the keyboard for a field with these traits, or reconfigures it
    /// if it is already up for another field.
    func beginEditing(_ traits: MorningstarTextInput) {
        switch Int(traits.keyboard) {
        case MORNINGSTAR_KEYBOARD_NUMBER: keyboardType = .numberPad
        case MORNINGSTAR_KEYBOARD_DECIMAL: keyboardType = .decimalPad
        case MORNINGSTAR_KEYBOARD_PHONE: keyboardType = .phonePad
        case MORNINGSTAR_KEYBOARD_EMAIL: keyboardType = .emailAddress
        default: keyboardType = .default
        }
        switch Int(traits.capitalization) {
        case MORNINGSTAR_CAPITALIZE_SENTENCES: autocapitalizationType = .sentences
        case MORNINGSTAR_CAPITALIZE_WORDS: autocapitalizationType = .words
        case MORNINGSTAR_CAPITALIZE_CHARACTERS: autocapitalizationType = .allCharacters
        default: autocapitalizationType = .none
        }
        switch Int(traits.action) {
        case MORNINGSTAR_ACTION_NEXT: returnKeyType = .next
        case MORNINGSTAR_ACTION_GO: returnKeyType = .go
        case MORNINGSTAR_ACTION_SEARCH: returnKeyType = .search
        case MORNINGSTAR_ACTION_SEND: returnKeyType = .send
        default: returnKeyType = traits.multiline ? .default : .done
        }
        isSecureTextEntry = traits.secure
        // Without marked text, autocorrection could only append: it is off.
        autocorrectionType = .no
        spellCheckingType = .no
        fieldHasText = traits.has_text

        editing = true
        if isFirstResponder {
            // Another field, or other traits: same responder, new keyboard.
            reloadInputViews()
        } else {
            becomeFirstResponder()
        }
    }

    /// Hides the keyboard because the application's field lost focus.
    func endEditing() {
        editing = false
        if isFirstResponder {
            resigningOnRequest = true
            resignFirstResponder()
            resigningOnRequest = false
        }
    }

    // UIKit also resigns the view by itself: the dismiss key of an iPad's
    // keyboard, another responder taking over. The application must hear of
    // it, or its field would stay focused with no keyboard.
    @discardableResult
    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned && !resigningOnRequest && editing {
            editing = false
            onFocusLost?()
        }
        return resigned
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isMultipleTouchEnabled = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("MetalView is created in code")
    }

    // A different window can be on a screen with a different scale; the view
    // controller recomputes scale, size and insets when the view lays out.
    override func didMoveToWindow() {
        super.didMoveToWindow()
        setNeedsLayout()
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        forward(touches, phase: UInt32(MORNINGSTAR_PHASE_DOWN))
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        forward(touches, phase: UInt32(MORNINGSTAR_PHASE_MOVED))
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        forward(touches, phase: UInt32(MORNINGSTAR_PHASE_UP))
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        forward(touches, phase: UInt32(MORNINGSTAR_PHASE_CANCELLED))
    }

    private func forward(_ touches: Set<UITouch>, phase: UInt32) {
        for touch in touches {
            let point = touch.location(in: self)
            // UIKit keeps one UITouch object per finger for the whole
            // gesture, so its identity is a stable pointer id.
            let id = UInt64(UInt(bitPattern: ObjectIdentifier(touch).hashValue))
            onPointer?(
                phase, id,
                Float(point.x * contentScaleFactor),
                Float(point.y * contentScaleFactor))
        }
    }
}

/// Forwards layout, lifecycle, display metrics and display refresh to the
/// Rust engine through the C interface in morningstar_ios.h. All calls happen
/// on the main thread.
///
/// Rendering is on demand. The display link runs only while the engine says
/// it needs a frame, and the engine can only start to need one inside a call
/// made from this class (or when a delay it asked for has passed, for which
/// a timer is armed). The same goes for the keyboard: whether it should be
/// up changes only inside those calls. So every method that calls into the
/// engine ends with `syncWithEngine()`; that is the one rule to keep when
/// editing.
///
/// Platform services (HTTP, recording, playback, stored values; ADR 0019)
/// follow the same rule: the application's requests appear only inside
/// those calls, `syncWithEngine()` hands them to `HostServices`, and each
/// answer comes back through `deliverServiceResult`. That part of the host
/// has never been compiled; see the note at the top of HostServices.swift.
final class HostViewController: UIViewController {
    private var engine: OpaquePointer?
    private let services = HostServices()
    private var displayLink: CADisplayLink?
    private var attached = false
    private var active = false
    /// Fires when the engine wants a frame at a later time (a caret blink).
    private var redrawTimer: Timer?
    /// Guards `syncWithEngine()` against re-entry: showing the keyboard
    /// posts notifications that call back into this class.
    private var syncing = false
    private var syncAgain = false
    /// Dark status-bar content (over a light screen), as the application
    /// last asked (ADR 0026). NOT COMPILED.
    private var darkStatusContent = false
    /// The application's semantics tree, for VoiceOver (ADR 0032). NOT
    /// COMPILED; see HostAccessibility.swift.
    private let accessibility = HostAccessibility()

    private var metalView: MetalView? { view as? MetalView }

    override var preferredStatusBarStyle: UIStatusBarStyle {
        darkStatusContent ? .darkContent : .lightContent
    }

    override func loadView() {
        let metalView = MetalView()
        metalView.onPointer = { [weak self] phase, id, x, y in
            guard let self = self else { return }
            morningstar_ios_pointer(self.engine, phase, id, x, y)
            self.syncWithEngine()
        }
        metalView.onInsertText = { [weak self] text in
            guard let self = self else { return }
            text.withCString { morningstar_ios_text_insert(self.engine, $0) }
            self.syncWithEngine()
        }
        metalView.onDeleteBackward = { [weak self] in
            guard let self = self else { return }
            morningstar_ios_text_delete_backward(self.engine)
            self.syncWithEngine()
        }
        metalView.onFocusLost = { [weak self] in
            guard let self = self else { return }
            morningstar_ios_text_focus_lost(self.engine)
            self.syncWithEngine()
        }
        metalView.onEditCommand = { [weak self] command in
            guard let self = self else { return }
            morningstar_ios_text_command(self.engine, command)
            self.syncWithEngine()
        }
        metalView.onCaretMove = { [weak self] move, extend in
            guard let self = self else { return }
            morningstar_ios_text_move(self.engine, move, extend)
            self.syncWithEngine()
        }
        metalView.accessibility = accessibility
        accessibility.view = metalView
        accessibility.perform = { [weak self] node, action, text in
            guard let self = self else { return false }
            let done: Bool
            if let text = text {
                done = text.withCString {
                    morningstar_ios_accessibility_action(
                        self.engine, node, UInt32(action), $0, 0, 0)
                }
            } else {
                done = morningstar_ios_accessibility_action(
                    self.engine, node, UInt32(action), nil, 0, 0)
            }
            self.syncWithEngine()
            return done
        }
        view = metalView
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        // The application is handed its installation id and the values it
        // stored on earlier runs when it is set up, so it can know them
        // before its first frame. The C strings only need to live for the
        // call: Rust copies them.
        let stored = HostServices.storedSnapshot().map { strdup($0) }
        defer { stored.forEach { free($0) } }
        let storedPointers = stored.map { UnsafePointer<CChar>($0) }
        engine = HostServices.installID().withCString { installID in
            storedPointers.withUnsafeBufferPointer { pointers in
                morningstar_ios_create_with_state(installID, pointers.baseAddress, pointers.count)
            }
        }
        services.owner = self
        // Whatever the application asked for while it was being set up.
        syncWithEngine()
        // Benchmarks pass this launch argument to get a steady frame stream.
        if ProcessInfo.processInfo.arguments.contains("-morningstar-continuous") {
            morningstar_ios_set_continuous(engine, true)
        }

        let link = CADisplayLink(target: self, selector: #selector(onDisplayLink))
        link.isPaused = true
        link.add(to: .main, forMode: .common)
        displayLink = link

        // The keyboard's frame, for the keyboard inset. One notification
        // covers showing, hiding, resizing and rotation.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(keyboardFrameWillChange(_:)),
            name: UIResponder.keyboardWillChangeFrameNotification,
            object: nil)

        // "Reduce Motion" in the accessibility settings (ADR 0026): now,
        // and whenever the user changes it. NOT COMPILED.
        morningstar_ios_set_reduce_motion(engine, UIAccessibility.isReduceMotionEnabled)
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(reduceMotionChanged),
            name: UIAccessibility.reduceMotionStatusDidChangeNotification,
            object: nil)

        // VoiceOver and Switch Control (ADR 0032): now, and whenever
        // either comes or goes. NOT COMPILED.
        for name in [
            UIAccessibility.voiceOverStatusDidChangeNotification,
            UIAccessibility.switchControlStatusDidChangeNotification,
        ] {
            NotificationCenter.default.addObserver(
                self,
                selector: #selector(assistiveTechnologyChanged),
                name: name,
                object: nil)
        }
        assistiveTechnologyChanged()
    }

    /// Whether anything reads the screen: VoiceOver, or Switch Control.
    /// While nothing does, the engine describes nothing. NOT COMPILED.
    @objc private func assistiveTechnologyChanged() {
        let enabled = UIAccessibility.isVoiceOverRunning || UIAccessibility.isSwitchControlRunning
        if !enabled {
            accessibility.reset()
        }
        morningstar_ios_set_accessibility(engine, enabled)
        syncWithEngine()
    }

    /// Takes what changed in the meaning of the screen, if the frame last
    /// built changed it, and tells VoiceOver. One call that answers false
    /// while accessibility is off.
    private func updateAccessibility() {
        var data: UnsafePointer<UInt8>?
        var length = 0
        while morningstar_ios_take_semantics(engine, &data, &length) {
            // The bytes are only valid until the next take: decoded here.
            accessibility.apply(UnsafeBufferPointer(start: data, count: length))
        }
    }

    @objc private func reduceMotionChanged() {
        morningstar_ios_set_reduce_motion(engine, UIAccessibility.isReduceMotionEnabled)
        syncWithEngine()
    }

    /// Light or dark status-bar content, when the frame just built wants
    /// it changed. The home indicator takes its colour from what is drawn
    /// under it and needs nothing.
    private func updateSystemBars() {
        var bars = MorningstarSystemBars()
        if morningstar_ios_take_system_bars(engine, &bars),
            bars.dark_status_content != darkStatusContent
        {
            darkStatusContent = bars.dark_status_content
            setNeedsStatusBarAppearanceUpdate()
        }
    }

    @objc private func keyboardFrameWillChange(_ notification: Notification) {
        guard
            let value = notification.userInfo?[UIResponder.keyboardFrameEndUserInfoKey]
                as? NSValue
        else { return }
        // The frame is in screen coordinates; how far it reaches up into
        // the view is what the application has to keep clear.
        let frame = view.convert(value.cgRectValue, from: nil)
        let overlap = max(0, view.bounds.maxY - frame.minY)
        morningstar_ios_set_keyboard_inset(
            engine, Float(overlap * view.contentScaleFactor))
        syncWithEngine()
    }

    // Runs when the view's size changes (including rotation) and when it is
    // asked to by the two overrides below, so everything that depends on the
    // screen is recomputed in one place. The engine ignores repeated values.
    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        // Render at the screen's native pixel density.
        let scale = view.window?.screen.nativeScale ?? UIScreen.main.nativeScale
        view.contentScaleFactor = scale
        morningstar_ios_set_scale(engine, Float(scale))
        sendSafeAreaInsets()

        let width = UInt32((view.bounds.width * scale).rounded())
        let height = UInt32((view.bounds.height * scale).rounded())
        if attached {
            morningstar_ios_resize(engine, width, height)
        } else {
            let layer = Unmanaged.passUnretained(view.layer).toOpaque()
            attached = morningstar_ios_attach_layer(engine, layer, width, height)
        }
        syncWithEngine()
    }

    // The safe area can change without the view's size changing (the status
    // bar appears, the in-call bar grows), which does not trigger a layout.
    override func viewSafeAreaInsetsDidChange() {
        super.viewSafeAreaInsetsDidChange()
        sendSafeAreaInsets()
        syncWithEngine()
    }

    // The scale changes when the window moves to a screen of another density.
    override func traitCollectionDidChange(_ previousTraitCollection: UITraitCollection?) {
        super.traitCollectionDidChange(previousTraitCollection)
        if previousTraitCollection?.displayScale != traitCollection.displayScale {
            view.setNeedsLayout()
        }
    }

    private func sendSafeAreaInsets() {
        let scale = view.contentScaleFactor
        let insets = view.safeAreaInsets
        morningstar_ios_set_insets(
            engine,
            Float(insets.left * scale),
            Float(insets.top * scale),
            Float(insets.right * scale),
            Float(insets.bottom * scale))
    }

    func setActive(_ active: Bool) {
        self.active = active
        morningstar_ios_set_active(engine, active)
        syncWithEngine()
    }

    // TODO(ADR 0018): nothing calls morningstar_ios_back yet. The intended
    // caller is a UIScreenEdgePanGestureRecognizer on the leading edge,
    // added in viewDidLoad, whose action on `.ended` is
    //
    //     _ = morningstar_ios_back(engine)
    //     syncWithEngine()
    //
    // It is left out until this host has been compiled and run: an edge
    // recogniser delays or cancels the touches MetalView forwards near that
    // edge, and how that feels can only be judged on a device.

    @objc private func onDisplayLink() {
        morningstar_ios_frame(engine)
        syncWithEngine()
    }

    /// Brings the host into line with the engine after a call into it: the
    /// display link, the redraw timer and the keyboard.
    private func syncWithEngine() {
        if syncing {
            syncAgain = true
            return
        }
        syncing = true
        repeat {
            syncAgain = false
            if let engine = engine {
                // Starting work never calls back into the engine: answers
                // arrive on a later turn of the run loop.
                services.drain(engine)
            }
            updateKeyboard()
            updateSystemBars()
            updateAccessibility()
            updateDisplayLink()
        } while syncAgain
        syncing = false
    }

    /// An answer from `HostServices`, on the main thread. `call` makes the
    /// one `morningstar_ios_*_result` call. It is not run once the engine
    /// is gone, which is how an answer that arrives too late is dropped.
    func deliverServiceResult(_ call: (OpaquePointer) -> Void) {
        guard let engine = engine else { return }
        call(engine)
        // The answer may want a frame, and the application may have asked
        // for more work in response.
        syncWithEngine()
    }

    /// Runs the display link exactly while there is a frame to produce. A
    /// paused display link wakes nothing, so an unchanged screen costs no CPU
    /// or GPU time. If the engine wants a frame later (a caret that blinks
    /// twice a second), a one-shot timer wakes this method up then, and the
    /// display link runs for that one frame.
    private func updateDisplayLink() {
        guard let displayLink = displayLink else { return }
        let paused = !(active && morningstar_ios_needs_frame(engine))
        if displayLink.isPaused != paused {
            displayLink.isPaused = paused
        }

        redrawTimer?.invalidate()
        redrawTimer = nil
        let delay = morningstar_ios_redraw_delay(engine)
        if paused && active && delay >= 0 {
            let timer = Timer(timeInterval: max(delay, 0.001), repeats: false) {
                [weak self] _ in
                self?.syncWithEngine()
            }
            timer.tolerance = 0.02
            RunLoop.main.add(timer, forMode: .common)
            redrawTimer = timer
        }
    }

    /// Shows, reconfigures or hides the keyboard if the frame the engine
    /// last built gained, changed or lost a focused text field.
    private func updateKeyboard() {
        guard let metalView = metalView else { return }
        var command = MorningstarTextInput()
        while morningstar_ios_take_text_input(engine, &command) {
            switch Int(command.command) {
            case MORNINGSTAR_TEXT_SHOW:
                metalView.beginEditing(command)
            case MORNINGSTAR_TEXT_UPDATE:
                metalView.fieldHasText = command.has_text
            default:
                metalView.endEditing()
            }
        }
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
        redrawTimer?.invalidate()
        displayLink?.invalidate()
        // Before the engine goes: nothing may answer it afterwards.
        services.shutDown()
        // Destroying the engine releases its reference to the layer before
        // the view (and the layer) is deallocated.
        morningstar_ios_destroy(engine)
        engine = nil
    }
}

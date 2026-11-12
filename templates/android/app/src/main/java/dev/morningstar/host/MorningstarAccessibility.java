package dev.morningstar.host;

import android.content.Context;
import android.graphics.Rect;
import android.os.Build;
import android.os.Bundle;
import android.util.Log;
import android.util.SparseArray;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewParent;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityManager;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.text.BreakIterator;
import java.util.ArrayList;

/**
 * What TalkBack and the other accessibility services see of a Morningstar
 * screen: a virtual view hierarchy under the one SurfaceView, built from
 * the semantics tree Rust derives from its widgets. See
 * docs/architecture/0032-accessibility.md.
 *
 * Rust sends what changed in the tree after each frame, as one byte array
 * (MorningstarNative.onSemanticsUpdate; the format is that of
 * SemanticsTree::encode_update in crates/morningstar-core/src/semantics.rs,
 * and the two must change together). This class keeps the tree and answers
 * every query of the accessibility framework from its copy, on the main
 * thread, without ever waiting for the render thread. Actions (a double
 * tap, a scroll, new text) are posted to Rust like touches
 * (nativeAccessibilityAction); what they change comes back as the next
 * update.
 *
 * The tree is static, like the Rust runtime it mirrors: it outlives an
 * activity, and a recreated activity's view attaches to it. Everything here
 * is main-thread only.
 *
 * A password field's node carries bullets, never its characters: Rust does
 * not send them.
 */
final class MorningstarAccessibility extends AccessibilityNodeProvider {
    private static final String TAG = "Morningstar";

    // The format's version and the bits of a node's "what changed" byte.
    // Must match crates/morningstar-core/src/semantics.rs.
    private static final int VERSION = 1;
    private static final int CHANGED_ADDED = 1;
    private static final int CHANGED_BOUNDS = 1 << 1;
    private static final int CHANGED_LABEL = 1 << 2;
    private static final int CHANGED_VALUE = 1 << 3;
    private static final int CHANGED_STATE = 1 << 4;
    private static final int CHANGED_CHILDREN = 1 << 5;
    private static final int CHANGED_SCROLL = 1 << 6;
    private static final int CHANGED_SELECTION = 1 << 7;

    // Roles. Must match `Role` in semantics.rs.
    private static final int ROLE_GROUP = 0;
    private static final int ROLE_TEXT = 1;
    private static final int ROLE_HEADING = 2;
    private static final int ROLE_BUTTON = 3;
    private static final int ROLE_LINK = 4;
    private static final int ROLE_IMAGE = 5;
    private static final int ROLE_TEXT_FIELD = 6;
    private static final int ROLE_CHECKBOX = 7;
    private static final int ROLE_SWITCH = 8;
    private static final int ROLE_RADIO = 9;
    private static final int ROLE_TAB = 10;
    private static final int ROLE_LIST = 11;
    private static final int ROLE_PROGRESS = 12;
    private static final int ROLE_SLIDER = 13;

    // States. Must match `States` in semantics.rs.
    private static final int STATE_DISABLED = 1;
    private static final int STATE_SELECTED = 1 << 1;
    private static final int STATE_CHECKABLE = 1 << 2;
    private static final int STATE_CHECKED = 1 << 3;
    private static final int STATE_FOCUSED = 1 << 4;
    private static final int STATE_PASSWORD = 1 << 5;
    private static final int STATE_MULTILINE = 1 << 6;
    private static final int STATE_EXPANDABLE = 1 << 7;
    private static final int STATE_EXPANDED = 1 << 8;
    private static final int STATE_LIVE_POLITE = 1 << 9;
    private static final int STATE_LIVE_ASSERTIVE = 1 << 10;

    // Actions: the bit positions are the codes nativeAccessibilityAction
    // takes (MorningstarNative.A11Y_ACTION_*). Must match `Actions`.
    private static final int CAN_ACTIVATE = 1;
    private static final int CAN_LONG_PRESS = 1 << 1;
    private static final int CAN_SCROLL_FORWARD = 1 << 2;
    private static final int CAN_SCROLL_BACKWARD = 1 << 3;
    private static final int CAN_FOCUS = 1 << 4;
    private static final int CAN_SET_TEXT = 1 << 5;
    private static final int CAN_SET_SELECTION = 1 << 6;
    private static final int CAN_INCREMENT = 1 << 7;
    private static final int CAN_DECREMENT = 1 << 8;

    /** The root node's id; it stands for the host view itself. */
    private static final int ROOT = 0;

    /**
     * Above this many changed nodes in one update (a scroll moves every
     * row), the services are told once that the subtree changed instead of
     * once per node.
     */
    private static final int MAX_NODE_EVENTS = 8;

    /** One node of the tree, as last sent. */
    private static final class Node {
        int id;
        int parent;
        int role;
        int states;
        int actions;
        float left;
        float top;
        float right;
        float bottom;
        /** In dp; both negative if the node does not scroll. */
        float scrollPosition;
        float scrollRange;
        /** In UTF-16 units of value; -1 if there is no selection. */
        int selectionStart;
        int selectionEnd;
        int[] children;
        String label;
        String value;
        String hint;

        boolean is(int state) {
            return (states & state) != 0;
        }

        boolean can(int action) {
            return (actions & action) != 0;
        }
    }

    private static final SparseArray<Node> NODES = new SparseArray<>();
    /** The provider of the activity on screen, or null. */
    private static MorningstarAccessibility attached;
    /** The node with accessibility focus, or INVALID. */
    private static int accessibilityFocus = Integer.MIN_VALUE;
    /** The node the finger is over while exploring by touch, or INVALID. */
    private static int hovered = Integer.MIN_VALUE;
    /** The node with the input focus (a text field), or INVALID. */
    private static int inputFocus = Integer.MIN_VALUE;

    private static final int INVALID = Integer.MIN_VALUE;

    private final View host;
    private final AccessibilityManager manager;
    private final int[] location = new int[2];

    private final AccessibilityManager.AccessibilityStateChangeListener stateListener =
            enabled -> sendEnabled();
    private final AccessibilityManager.TouchExplorationStateChangeListener touchListener =
            enabled -> sendEnabled();

    MorningstarAccessibility(View host) {
        this.host = host;
        this.manager =
                (AccessibilityManager)
                        host.getContext().getSystemService(Context.ACCESSIBILITY_SERVICE);
    }

    // ------------------------------------------------------------ lifecycle

    /**
     * The view's activity is on screen: this provider answers for the tree
     * from now on, and Rust is told whether to describe its frames and
     * again whenever that changes.
     */
    void attach() {
        attached = this;
        manager.addAccessibilityStateChangeListener(stateListener);
        manager.addTouchExplorationStateChangeListener(touchListener);
        sendEnabled();
    }

    /** The view's activity is going away. */
    void detach() {
        manager.removeAccessibilityStateChangeListener(stateListener);
        manager.removeTouchExplorationStateChangeListener(touchListener);
        if (attached == this) {
            attached = null;
        }
    }

    /**
     * Tells Rust whether anything reads the screen through the
     * accessibility framework: TalkBack, but also Switch Access, Voice
     * Access and UI tests, which need the tree without touch exploration.
     * While nothing does, Rust builds no tree and sends nothing.
     */
    private void sendEnabled() {
        boolean enabled = manager.isEnabled();
        if (!enabled) {
            NODES.clear();
            accessibilityFocus = INVALID;
            hovered = INVALID;
            inputFocus = INVALID;
        }
        MorningstarNative.nativeSetAccessibility(enabled);
    }

    // -------------------------------------------------------------- updates

    /**
     * Applies one update from Rust and tells the accessibility services
     * what changed. Main thread.
     */
    static void applyUpdate(byte[] data) {
        ArrayList<Node> changed = new ArrayList<>();
        ArrayList<Integer> changes = new ArrayList<>();
        ArrayList<String> oldValues = new ArrayList<>();
        boolean structure;
        boolean forget;
        int focused;
        try {
            ByteBuffer in = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN);
            int version = in.getInt();
            if (version != VERSION) {
                Log.w(TAG, "accessibility: update of unknown version " + version);
                return;
            }
            int flags = in.getInt();
            focused = in.getInt();
            forget = (flags & 1) != 0;
            structure = forget;
            if (structure) {
                NODES.clear();
            }
            int removed = in.getInt();
            for (int i = 0; i < removed; i++) {
                int id = in.getInt();
                NODES.remove(id);
                if (id == accessibilityFocus) {
                    accessibilityFocus = INVALID;
                }
                if (id == hovered) {
                    hovered = INVALID;
                }
                structure = true;
            }
            int count = in.getInt();
            for (int i = 0; i < count; i++) {
                Node node = new Node();
                node.id = in.getInt();
                node.parent = in.getInt();
                node.role = in.get() & 0xFF;
                int change = in.get() & 0xFF;
                node.states = in.getShort() & 0xFFFF;
                node.actions = in.getShort() & 0xFFFF;
                in.getShort();
                node.left = in.getFloat();
                node.top = in.getFloat();
                node.right = in.getFloat();
                node.bottom = in.getFloat();
                node.scrollPosition = in.getFloat();
                node.scrollRange = in.getFloat();
                node.selectionStart = in.getInt();
                node.selectionEnd = in.getInt();
                node.children = new int[in.getInt()];
                for (int c = 0; c < node.children.length; c++) {
                    node.children[c] = in.getInt();
                }
                node.label = readString(in);
                node.value = readString(in);
                node.hint = readString(in);
                Node before = NODES.get(node.id);
                oldValues.add(before == null ? "" : before.value);
                NODES.put(node.id, node);
                changed.add(node);
                changes.add(change);
                if ((change & (CHANGED_ADDED | CHANGED_CHILDREN)) != 0) {
                    structure = true;
                }
            }
        } catch (RuntimeException e) {
            // A truncated or malformed update: Rust and this file disagree
            // about the format. Nothing sensible is left of the tree.
            Log.e(TAG, "accessibility: malformed update, tree dropped", e);
            NODES.clear();
            return;
        }
        int previousFocus = inputFocus;
        inputFocus = focused < 0 ? INVALID : focused;

        MorningstarAccessibility provider = attached;
        if (provider == null || !provider.manager.isEnabled()) {
            return;
        }
        if (structure || changed.size() > MAX_NODE_EVENTS) {
            // Children came, went or moved: have the services read the
            // subtree again.
            provider.sendContentChanged(
                    View.NO_ID, AccessibilityEvent.CONTENT_CHANGE_TYPE_SUBTREE);
        }
        boolean perNode = !structure && changed.size() <= MAX_NODE_EVENTS;
        for (int i = 0; i < changed.size(); i++) {
            Node node = changed.get(i);
            int change = changes.get(i);
            if ((change & CHANGED_ADDED) != 0) {
                // A live region that has just appeared (a toast) is news
                // too, unless the whole screen is new.
                if (!forget && node.is(STATE_LIVE_POLITE | STATE_LIVE_ASSERTIVE)) {
                    provider.sendContentChanged(
                            node.id, AccessibilityEvent.CONTENT_CHANGE_TYPE_TEXT);
                }
                continue;
            }
            if (node.role == ROLE_TEXT_FIELD && (change & CHANGED_VALUE) != 0) {
                provider.sendTextChanged(node, oldValues.get(i));
            }
            if (node.role == ROLE_TEXT_FIELD
                    && (change & CHANGED_SELECTION) != 0
                    && node.selectionStart >= 0) {
                provider.sendSelectionChanged(node);
            }
            if ((change & CHANGED_SCROLL) != 0) {
                provider.sendScrolled(node);
            }
            boolean said = (change & (CHANGED_LABEL | CHANGED_VALUE)) != 0;
            boolean live = said && node.is(STATE_LIVE_POLITE | STATE_LIVE_ASSERTIVE);
            if (perNode || live) {
                // CONTENT_CHANGE_TYPE_UNDEFINED is 0: "something about it".
                int types = AccessibilityEvent.CONTENT_CHANGE_TYPE_UNDEFINED;
                if (said) {
                    types |= AccessibilityEvent.CONTENT_CHANGE_TYPE_TEXT;
                    if (Build.VERSION.SDK_INT >= 30 && (change & CHANGED_VALUE) != 0) {
                        types |= AccessibilityEvent.CONTENT_CHANGE_TYPE_STATE_DESCRIPTION;
                    }
                }
                // A live region's event is what makes TalkBack read its
                // new text out: it asks for the node, finds it live, and
                // speaks it.
                provider.sendContentChanged(node.id, types);
            }
        }
        if (inputFocus != previousFocus && inputFocus != INVALID) {
            provider.sendEvent(inputFocus, AccessibilityEvent.TYPE_VIEW_FOCUSED);
        }
        if ((accessibilityFocus != INVALID) && NODES.get(accessibilityFocus) == null) {
            accessibilityFocus = INVALID;
        }
        // The focus rectangle TalkBack draws follows the node's bounds;
        // have the view report them again.
        provider.host.invalidate();
    }

    private static String readString(ByteBuffer in) {
        int length = in.getInt();
        if (length == 0) {
            return "";
        }
        byte[] bytes = new byte[length];
        in.get(bytes);
        return new String(bytes, StandardCharsets.UTF_8);
    }

    /** Has the screen reader say something that is not on the screen. */
    @SuppressWarnings("deprecation") // announceForAccessibility: there is no node to make live.
    static void announce(String message, boolean assertive) {
        MorningstarAccessibility provider = attached;
        if (provider == null || !provider.manager.isEnabled() || message.isEmpty()) {
            return;
        }
        if (assertive) {
            provider.manager.interrupt();
        }
        provider.host.announceForAccessibility(message);
    }

    // -------------------------------------------------------------- queries

    @Override
    public AccessibilityNodeInfo createAccessibilityNodeInfo(int virtualViewId) {
        if (virtualViewId == View.NO_ID) {
            return hostInfo();
        }
        Node node = NODES.get(virtualViewId);
        if (node == null || virtualViewId == ROOT) {
            return null;
        }
        return nodeInfo(node);
    }

    /** The host view, with the root's children as its own. */
    @SuppressWarnings("deprecation") // obtain(View): the constructor is API 30.
    private AccessibilityNodeInfo hostInfo() {
        AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain(host);
        host.onInitializeAccessibilityNodeInfo(info);
        Node root = NODES.get(ROOT);
        if (root != null) {
            for (int child : root.children) {
                info.addChild(host, child);
            }
        }
        return info;
    }

    @SuppressWarnings("deprecation") // obtain(View, int), setBoundsInParent.
    private AccessibilityNodeInfo nodeInfo(Node node) {
        AccessibilityNodeInfo info = AccessibilityNodeInfo.obtain(host, node.id);
        info.setPackageName(host.getContext().getPackageName());
        info.setClassName(className(node.role));
        if (node.parent <= ROOT) {
            info.setParent(host);
        } else {
            info.setParent(host, node.parent);
        }
        for (int child : node.children) {
            info.addChild(host, child);
        }

        // Rust's bounds are pixels of the surface, which is the view; the
        // services want the screen's.
        host.getLocationOnScreen(location);
        Rect bounds =
                new Rect(
                        (int) Math.floor(node.left),
                        (int) Math.floor(node.top),
                        (int) Math.ceil(node.right),
                        (int) Math.ceil(node.bottom));
        Node parent = NODES.get(node.parent);
        Rect inParent = new Rect(bounds);
        if (parent != null) {
            inParent.offset(-(int) Math.floor(parent.left), -(int) Math.floor(parent.top));
        }
        info.setBoundsInParent(inParent);
        bounds.offset(location[0], location[1]);
        info.setBoundsInScreen(bounds);
        info.setVisibleToUser(true);

        boolean enabled = !node.is(STATE_DISABLED);
        info.setEnabled(enabled);
        info.setSelected(node.is(STATE_SELECTED));
        if (node.is(STATE_CHECKABLE)) {
            info.setCheckable(true);
            info.setChecked(node.is(STATE_CHECKED));
        }
        if (node.role == ROLE_HEADING && Build.VERSION.SDK_INT >= 28) {
            info.setHeading(true);
        }
        if (node.is(STATE_LIVE_ASSERTIVE)) {
            info.setLiveRegion(View.ACCESSIBILITY_LIVE_REGION_ASSERTIVE);
        } else if (node.is(STATE_LIVE_POLITE)) {
            info.setLiveRegion(View.ACCESSIBILITY_LIVE_REGION_POLITE);
        }
        String role = roleDescription(node.role);
        if (role != null) {
            // What AccessibilityNodeInfoCompat.setRoleDescription writes,
            // and TalkBack reads.
            info.getExtras().putCharSequence("AccessibilityNodeInfo.roleDescription", role);
        }

        if (node.role == ROLE_TEXT_FIELD) {
            describeField(info, node);
        } else {
            describe(info, node);
        }

        // Accessibility focus is this class's to keep: the nodes are not
        // views.
        if (accessibilityFocus == node.id) {
            info.setAccessibilityFocused(true);
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS);
        } else {
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
        }
        if (node.can(CAN_ACTIVATE)) {
            info.setClickable(true);
            info.setFocusable(true);
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK);
        }
        if (node.can(CAN_LONG_PRESS)) {
            info.setLongClickable(true);
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_LONG_CLICK);
        }
        // A slider is raised and lowered with the scroll actions: that is
        // what TalkBack's volume keys send to a SeekBar.
        if (node.can(CAN_SCROLL_FORWARD) || node.can(CAN_INCREMENT)) {
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
        }
        if (node.can(CAN_SCROLL_BACKWARD) || node.can(CAN_DECREMENT)) {
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
        }
        if (node.scrollRange >= 0) {
            info.setScrollable(true);
        }
        return info;
    }

    /** Text, description and state of anything that is not a text field. */
    private static void describe(AccessibilityNodeInfo info, Node node) {
        StringBuilder spoken = new StringBuilder(node.label);
        boolean stateDescription = Build.VERSION.SDK_INT >= 30;
        if (!node.value.isEmpty()) {
            if (stateDescription) {
                info.setStateDescription(node.value);
            } else {
                append(spoken, node.value);
            }
        }
        if (node.is(STATE_EXPANDABLE) && !stateDescription) {
            append(spoken, node.is(STATE_EXPANDED) ? "expanded" : "collapsed");
        }
        if (node.is(STATE_EXPANDABLE) && node.can(CAN_ACTIVATE)) {
            info.addAction(
                    node.is(STATE_EXPANDED)
                            ? AccessibilityNodeInfo.AccessibilityAction.ACTION_COLLAPSE
                            : AccessibilityNodeInfo.AccessibilityAction.ACTION_EXPAND);
        }
        // A hint is part of what is said: there is no field of a node for
        // "what this is for" that TalkBack reads on every kind of node.
        append(spoken, node.hint);
        String text = spoken.toString();
        if (node.role == ROLE_IMAGE || !node.hint.isEmpty()) {
            info.setContentDescription(text);
        } else {
            info.setText(text);
        }
    }

    /** An editable field: its text, hint, selection and editing actions. */
    private static void describeField(AccessibilityNodeInfo info, Node node) {
        info.setEditable(true);
        info.setFocusable(true);
        info.setFocused(node.is(STATE_FOCUSED));
        info.setPassword(node.is(STATE_PASSWORD));
        info.setMultiLine(node.is(STATE_MULTILINE));
        // What the field is called and what it asks for, together: an
        // EditText has only its hint to carry both.
        StringBuilder hint = new StringBuilder(node.label);
        append(hint, node.hint);
        if (node.value.isEmpty()) {
            if (Build.VERSION.SDK_INT >= 26) {
                info.setHintText(hint);
                info.setShowingHintText(true);
            }
            info.setText(hint);
        } else {
            // Bullets for a password: Rust sends nothing else.
            info.setText(node.value);
            if (Build.VERSION.SDK_INT >= 26) {
                info.setHintText(hint);
            } else {
                info.setContentDescription(hint + ", " + node.value);
            }
        }
        if (node.selectionStart >= 0) {
            info.setTextSelection(node.selectionStart, node.selectionEnd);
        }
        if (node.can(CAN_FOCUS) && !node.is(STATE_FOCUSED)) {
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_FOCUS);
        }
        if (node.can(CAN_SET_TEXT)) {
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_TEXT);
        }
        if (node.can(CAN_SET_SELECTION) && !node.value.isEmpty()) {
            info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SET_SELECTION);
            info.setMovementGranularities(
                    AccessibilityNodeInfo.MOVEMENT_GRANULARITY_CHARACTER
                            | AccessibilityNodeInfo.MOVEMENT_GRANULARITY_WORD);
            info.addAction(
                    AccessibilityNodeInfo.AccessibilityAction.ACTION_NEXT_AT_MOVEMENT_GRANULARITY);
            info.addAction(
                    AccessibilityNodeInfo.AccessibilityAction
                            .ACTION_PREVIOUS_AT_MOVEMENT_GRANULARITY);
        }
    }

    private static void append(StringBuilder to, String part) {
        if (part.isEmpty()) {
            return;
        }
        if (to.length() > 0) {
            to.append(", ");
        }
        to.append(part);
    }

    /** The class TalkBack takes a node's role from. */
    private static String className(int role) {
        switch (role) {
            case ROLE_TEXT:
            case ROLE_HEADING:
                return "android.widget.TextView";
            case ROLE_BUTTON:
            case ROLE_LINK:
            case ROLE_TAB:
                return "android.widget.Button";
            case ROLE_IMAGE:
                return "android.widget.ImageView";
            case ROLE_TEXT_FIELD:
                return "android.widget.EditText";
            case ROLE_CHECKBOX:
                return "android.widget.CheckBox";
            case ROLE_SWITCH:
                return "android.widget.Switch";
            case ROLE_RADIO:
                return "android.widget.RadioButton";
            case ROLE_LIST:
                return "android.widget.ScrollView";
            case ROLE_PROGRESS:
                return "android.widget.ProgressBar";
            case ROLE_SLIDER:
                return "android.widget.SeekBar";
            case ROLE_GROUP:
            default:
                return "android.view.ViewGroup";
        }
    }

    /** What a role is called where no widget class says it. English. */
    private static String roleDescription(int role) {
        switch (role) {
            case ROLE_TAB:
                return "Tab";
            case ROLE_LINK:
                return "Link";
            default:
                return null;
        }
    }

    @Override
    public AccessibilityNodeInfo findFocus(int focus) {
        int id = focus == AccessibilityNodeInfo.FOCUS_ACCESSIBILITY ? accessibilityFocus : inputFocus;
        if (id == INVALID || NODES.get(id) == null) {
            return null;
        }
        return createAccessibilityNodeInfo(id);
    }

    // -------------------------------------------------------------- actions

    @Override
    public boolean performAction(int virtualViewId, int action, Bundle arguments) {
        if (virtualViewId == View.NO_ID) {
            return host.performAccessibilityAction(action, arguments);
        }
        Node node = NODES.get(virtualViewId);
        if (node == null) {
            return false;
        }
        int id = node.id;
        switch (action) {
            case AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS:
                if (accessibilityFocus == id) {
                    return false;
                }
                if (accessibilityFocus != INVALID) {
                    sendEvent(
                            accessibilityFocus,
                            AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED);
                }
                accessibilityFocus = id;
                host.invalidate();
                sendEvent(id, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED);
                return true;
            case AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS:
                if (accessibilityFocus != id) {
                    return false;
                }
                accessibilityFocus = INVALID;
                host.invalidate();
                sendEvent(id, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED);
                return true;
            case AccessibilityNodeInfo.ACTION_CLICK:
                if (!node.can(CAN_ACTIVATE)) {
                    return false;
                }
                act(id, MorningstarNative.A11Y_ACTION_ACTIVATE);
                sendEvent(id, AccessibilityEvent.TYPE_VIEW_CLICKED);
                return true;
            case AccessibilityNodeInfo.ACTION_EXPAND:
            case AccessibilityNodeInfo.ACTION_COLLAPSE:
                // Opening and closing is what a tap on it does.
                boolean open = action == AccessibilityNodeInfo.ACTION_EXPAND;
                if (!node.can(CAN_ACTIVATE)
                        || !node.is(STATE_EXPANDABLE)
                        || node.is(STATE_EXPANDED) == open) {
                    return false;
                }
                act(id, MorningstarNative.A11Y_ACTION_ACTIVATE);
                return true;
            case AccessibilityNodeInfo.ACTION_LONG_CLICK:
                if (!node.can(CAN_LONG_PRESS)) {
                    return false;
                }
                act(id, MorningstarNative.A11Y_ACTION_LONG_PRESS);
                sendEvent(id, AccessibilityEvent.TYPE_VIEW_LONG_CLICKED);
                return true;
            case AccessibilityNodeInfo.ACTION_SCROLL_FORWARD:
                if (node.can(CAN_INCREMENT)) {
                    act(id, MorningstarNative.A11Y_ACTION_INCREMENT);
                    return true;
                }
                if (!node.can(CAN_SCROLL_FORWARD)) {
                    return false;
                }
                act(id, MorningstarNative.A11Y_ACTION_SCROLL_FORWARD);
                return true;
            case AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD:
                if (node.can(CAN_DECREMENT)) {
                    act(id, MorningstarNative.A11Y_ACTION_DECREMENT);
                    return true;
                }
                if (!node.can(CAN_SCROLL_BACKWARD)) {
                    return false;
                }
                act(id, MorningstarNative.A11Y_ACTION_SCROLL_BACKWARD);
                return true;
            case AccessibilityNodeInfo.ACTION_FOCUS:
                if (!node.can(CAN_FOCUS) || node.is(STATE_FOCUSED)) {
                    return false;
                }
                act(id, MorningstarNative.A11Y_ACTION_FOCUS);
                return true;
            case AccessibilityNodeInfo.ACTION_SET_TEXT:
                if (!node.can(CAN_SET_TEXT)) {
                    return false;
                }
                CharSequence text =
                        arguments == null
                                ? null
                                : arguments.getCharSequence(
                                        AccessibilityNodeInfo
                                                .ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE);
                MorningstarNative.nativeAccessibilityAction(
                        id,
                        MorningstarNative.A11Y_ACTION_SET_TEXT,
                        text == null ? "" : text.toString(),
                        0,
                        0);
                return true;
            case AccessibilityNodeInfo.ACTION_SET_SELECTION:
                if (!node.can(CAN_SET_SELECTION)) {
                    return false;
                }
                int length = node.value.length();
                int start =
                        arguments == null
                                ? length
                                : arguments.getInt(
                                        AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT,
                                        length);
                int end =
                        arguments == null
                                ? start
                                : arguments.getInt(
                                        AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT,
                                        start);
                select(node, clamp(start, length), clamp(end, length));
                return true;
            case AccessibilityNodeInfo.ACTION_NEXT_AT_MOVEMENT_GRANULARITY:
            case AccessibilityNodeInfo.ACTION_PREVIOUS_AT_MOVEMENT_GRANULARITY:
                return node.can(CAN_SET_SELECTION) && traverse(node, action, arguments);
            default:
                return false;
        }
    }

    private static int clamp(int value, int max) {
        return Math.max(0, Math.min(value, max));
    }

    /** Posts an action without arguments to Rust. */
    private static void act(int id, int action) {
        MorningstarNative.nativeAccessibilityAction(id, action, null, 0, 0);
    }

    /**
     * Asks Rust to put the field's selection at start..end (UTF-16 units of
     * its value) and remembers it, so that the next step of a traversal
     * starts from there even if Rust's next update has not arrived.
     */
    private static void select(Node node, int start, int end) {
        node.selectionStart = start;
        node.selectionEnd = end;
        MorningstarNative.nativeAccessibilityAction(
                node.id, MorningstarNative.A11Y_ACTION_SET_SELECTION, null, start, end);
    }

    /**
     * Moves a text field's caret by a character or a word, as TalkBack's
     * "read by character / word" does on an EditText, and tells it what
     * was passed over so that it reads it out.
     */
    private boolean traverse(Node node, int action, Bundle arguments) {
        if (arguments == null || node.value.isEmpty()) {
            return false;
        }
        int granularity =
                arguments.getInt(AccessibilityNodeInfo.ACTION_ARGUMENT_MOVEMENT_GRANULARITY_INT);
        boolean extend =
                arguments.getBoolean(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_EXTEND_SELECTION_BOOLEAN, false);
        BreakIterator iterator;
        if (granularity == AccessibilityNodeInfo.MOVEMENT_GRANULARITY_CHARACTER) {
            iterator = BreakIterator.getCharacterInstance();
        } else if (granularity == AccessibilityNodeInfo.MOVEMENT_GRANULARITY_WORD) {
            iterator = BreakIterator.getWordInstance();
        } else {
            return false;
        }
        String value = node.value;
        iterator.setText(value);
        boolean forward = action == AccessibilityNodeInfo.ACTION_NEXT_AT_MOVEMENT_GRANULARITY;
        int from = node.selectionEnd < 0 ? (forward ? 0 : value.length()) : node.selectionEnd;
        from = clamp(from, value.length());
        int to = from;
        // For words, pass over the spaces between them.
        do {
            int next = forward ? iterator.following(to) : iterator.preceding(to);
            if (next == BreakIterator.DONE) {
                break;
            }
            to = next;
        } while (granularity == AccessibilityNodeInfo.MOVEMENT_GRANULARITY_WORD
                && value.substring(Math.min(from, to), Math.max(from, to)).trim().isEmpty()
                && to > 0
                && to < value.length());
        if (to == from) {
            return false;
        }
        int anchor = extend && node.selectionStart >= 0 ? node.selectionStart : to;
        select(node, anchor, to);
        AccessibilityEvent event =
                obtain(AccessibilityEvent.TYPE_VIEW_TEXT_TRAVERSED_AT_MOVEMENT_GRANULARITY);
        event.setFromIndex(Math.min(from, to));
        event.setToIndex(Math.max(from, to));
        event.setAction(action);
        event.setMovementGranularity(granularity);
        event.getText().add(value);
        send(node.id, event);
        return true;
    }

    // ---------------------------------------------------- explore by touch

    /**
     * A hover event of the host view: with touch exploration on, a finger
     * moving over the screen. Tells the services which node it is over;
     * TalkBack then focuses and reads it. Returns true if the event was
     * used.
     */
    boolean dispatchHoverEvent(MotionEvent event) {
        if (!manager.isEnabled() || !manager.isTouchExplorationEnabled()) {
            return false;
        }
        switch (event.getActionMasked()) {
            case MotionEvent.ACTION_HOVER_ENTER:
            case MotionEvent.ACTION_HOVER_MOVE:
                int at = nodeAt(event.getX(), event.getY());
                setHovered(at);
                return at != INVALID;
            case MotionEvent.ACTION_HOVER_EXIT:
                if (hovered != INVALID) {
                    setHovered(INVALID);
                    return true;
                }
                return false;
            default:
                return false;
        }
    }

    private void setHovered(int id) {
        if (hovered == id) {
            return;
        }
        int previous = hovered;
        hovered = id;
        // Enter before exit, as ExploreByTouchHelper sends them.
        if (id != INVALID) {
            sendEvent(id, AccessibilityEvent.TYPE_VIEW_HOVER_ENTER);
        }
        if (previous != INVALID && NODES.get(previous) != null) {
            sendEvent(previous, AccessibilityEvent.TYPE_VIEW_HOVER_EXIT);
        }
    }

    /**
     * The node under a point of the view, in pixels: the deepest one, and
     * among siblings the one painted last. A container that says nothing
     * (an unlabelled list) is not a place to land.
     */
    private static int nodeAt(float x, float y) {
        Node root = NODES.get(ROOT);
        return root == null ? INVALID : nodeAt(root, x, y, 0);
    }

    private static int nodeAt(Node node, float x, float y, int depth) {
        if (depth > 64) {
            return INVALID;
        }
        for (int i = node.children.length - 1; i >= 0; i--) {
            Node child = NODES.get(node.children[i]);
            if (child == null
                    || x < child.left
                    || x >= child.right
                    || y < child.top
                    || y >= child.bottom) {
                continue;
            }
            int inside = nodeAt(child, x, y, depth + 1);
            if (inside != INVALID) {
                return inside;
            }
            boolean says =
                    !child.label.isEmpty() || !child.value.isEmpty() || child.actions != 0;
            boolean container = child.role == ROLE_LIST || child.role == ROLE_GROUP;
            if (says && !(container && child.label.isEmpty())) {
                return child.id;
            }
        }
        return INVALID;
    }

    // --------------------------------------------------------------- events

    @SuppressWarnings("deprecation") // obtain(int): the constructor is API 30.
    private static AccessibilityEvent obtain(int type) {
        return AccessibilityEvent.obtain(type);
    }

    /** Sends an event of `type` about a node (View.NO_ID: the host). */
    private void sendEvent(int id, int type) {
        if (!manager.isEnabled()) {
            return;
        }
        send(id, obtain(type));
    }

    private void send(int id, AccessibilityEvent event) {
        if (!manager.isEnabled()) {
            return;
        }
        ViewParent parent = host.getParent();
        if (parent == null) {
            return;
        }
        event.setPackageName(host.getContext().getPackageName());
        Node node = id == View.NO_ID ? null : NODES.get(id);
        if (node == null) {
            if (id != View.NO_ID) {
                // The node has gone since the event was decided on.
                return;
            }
            host.onInitializeAccessibilityEvent(event);
        } else {
            event.setClassName(className(node.role));
            event.setSource(host, id);
            event.setEnabled(!node.is(STATE_DISABLED));
            event.setPassword(node.is(STATE_PASSWORD));
            event.setChecked(node.is(STATE_CHECKED));
            if (event.getText().isEmpty() && !node.label.isEmpty()) {
                event.getText().add(node.label);
            }
            if (!node.value.isEmpty() && node.role != ROLE_TEXT_FIELD) {
                event.setContentDescription(node.label + ", " + node.value);
            }
        }
        parent.requestSendAccessibilityEvent(host, event);
    }

    private void sendContentChanged(int id, int types) {
        if (!manager.isEnabled()) {
            return;
        }
        AccessibilityEvent event = obtain(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
        event.setContentChangeTypes(types);
        send(id, event);
    }

    private void sendTextChanged(Node node, String before) {
        AccessibilityEvent event = obtain(AccessibilityEvent.TYPE_VIEW_TEXT_CHANGED);
        String now = node.value;
        // The run that differs: what TalkBack reads as typed or deleted.
        int prefix = 0;
        int shortest = Math.min(before.length(), now.length());
        while (prefix < shortest && before.charAt(prefix) == now.charAt(prefix)) {
            prefix++;
        }
        int suffix = 0;
        while (suffix < shortest - prefix
                && before.charAt(before.length() - 1 - suffix)
                        == now.charAt(now.length() - 1 - suffix)) {
            suffix++;
        }
        event.setFromIndex(prefix);
        event.setRemovedCount(before.length() - prefix - suffix);
        event.setAddedCount(now.length() - prefix - suffix);
        event.setBeforeText(before);
        event.getText().add(now);
        send(node.id, event);
    }

    private void sendSelectionChanged(Node node) {
        AccessibilityEvent event = obtain(AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED);
        event.setFromIndex(node.selectionStart);
        event.setToIndex(node.selectionEnd);
        event.setItemCount(node.value.length());
        event.getText().add(node.value);
        send(node.id, event);
    }

    private void sendScrolled(Node node) {
        if (node.scrollRange < 0) {
            return;
        }
        float density = host.getResources().getDisplayMetrics().density;
        AccessibilityEvent event = obtain(AccessibilityEvent.TYPE_VIEW_SCROLLED);
        event.setScrollY(Math.round(node.scrollPosition * density));
        event.setMaxScrollY(Math.round(node.scrollRange * density));
        send(node.id, event);
    }
}

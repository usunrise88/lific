// LIF-245 — single source of truth for every keyboard shortcut in the app.
//
// Two jobs:
//   1. `shortcuts()` is the data the Shortcut Help overlay (ShortcutHelp.svelte)
//      renders from, grouped by `scope`. Keeping the list here — rather than
//      hand-rolled markup duplicated inside a popover — means the overlay
//      can't silently drift from what the handlers actually bind. When you
//      add or change a binding in IssueList.svelte / Layout.svelte /
//      CommandPalette.svelte, update the matching entry here too.
//   2. `shortcutsSuppressed()` centralizes "should a shortcut fire right
//      now" — focus is in a text field, or a modal/overlay that owns its
//      own keyboard input (peek panel, command palette) is open. Handlers
//      that used to roll their own ad hoc guard (IssueList's
//      `isInputFocused`, DocumentDetail's `inField`, the peek-open
//      early-return) call this instead so the rule can't drift between
//      call sites either.

import { peekState } from "./issues/peek.svelte";
import { commandPaletteState } from "./commandPaletteState.svelte";
import { shortcutHelpState } from "./shortcutHelpState.svelte";
import { contextMenuState } from "./contextMenuState.svelte"; // LIF-248
import { mobileNavState } from "./mobileNavState.svelte";
import { t } from "./i18n";

export type ShortcutScope = "global" | "list" | "board" | "peek" | "palette" | "editor";

export interface ShortcutEntry {
  /** Display form. Space-separated tokens render as separate <kbd> chips,
   *  e.g. "J ↓" → two chips, "⌘ K" → two chips. */
  keys: string;
  label: string;
  scope: ShortcutScope;
}

export const SCOPE_ORDER: ShortcutScope[] = ["global", "list", "board", "peek", "palette", "editor"];

/** Display name of a shortcut group. A function, not a constant, so the
 *  active locale applies at render time. */
export function scopeLabel(scope: ShortcutScope): string {
  return t(`shortcuts.scope.${scope}`);
}

// [keys, label key under `shortcuts.`, scope]. Labels are resolved by
// `shortcuts()` at call time so the active locale applies.
const SHORTCUT_DEFS: [string, string, ShortcutScope][] = [
  // ── Global (Layout.svelte) ───────────────────────────────
  ["⌘ K", "openPalette", "global"],
  ["⌘ P", "openPalette", "global"],
  ["⌘ \\", "toggleSidebar", "global"],
  ["?", "showHelp", "global"],
  ["Esc", "closeDialog", "global"],

  // ── Issue list (IssueList.svelte handleKeydown) ──────────
  ["J ↓", "focusDown", "list"],
  ["K ↑", "focusUp", "list"],
  ["Home", "focusFirst", "list"],
  ["End", "focusLast", "list"],
  ["Enter", "openFocused", "list"],
  ["Space", "peekFocused", "list"],
  ["X", "toggleSelection", "list"],
  ["Ctrl/⌘ A", "selectAll", "list"],
  ["⇧ J/K", "extendSelection", "list"],
  ["S", "statusPicker", "list"],
  ["⇧ S", "cycleStatus", "list"],
  ["P", "priorityPicker", "list"],
  ["⇧ P", "cyclePriority", "list"],
  ["M", "modulePicker", "list"],
  ["C", "newIssue", "list"],
  ["/", "focusSearch", "list"],
  ["Esc", "clearFocus", "list"],

  // ── Board (IssueList.svelte, layout="board") ─────────────
  ["{click}", "openCard", "board"],
  ["⌘/Ctrl {click}", "peekCard", "board"],
  ["{drag}", "moveCard", "board"],
  ["C", "newIssue", "board"],
  ["/", "focusSearch", "board"],

  // ── Peek panel (PeekPanel.svelte) ─────────────────────────
  ["Esc", "closePreview", "peek"],

  // ── Command palette (CommandPalette.svelte) ──────────────
  ["↓ ↑", "moveSelection", "palette"],
  ["Enter", "openRun", "palette"],
  ["⌘ Enter", "openNewTab", "palette"],
  ["⌫", "stepBack", "palette"],
  ["Esc", "close", "palette"],

  // ── Markdown editor (EditableMarkdown.svelte, edit mode) ──
  ["⌘ B", "bold", "editor"],
  ["⌘ I", "italic", "editor"],
  ["⌘ ⇧ K", "insertLink", "editor"],
  ["⌘ S", "save", "editor"],
];

/** Every shortcut with its label in the active locale. Mouse gestures
 *  (Click, Drag) are words, not key names, so they are translated too. */
export function shortcuts(): ShortcutEntry[] {
  return SHORTCUT_DEFS.map(([keys, label, scope]) => ({
    keys: keys
      .replace("{click}", t("shortcuts.gesture.click"))
      .replace("{drag}", t("shortcuts.gesture.drag")),
    label: t(`shortcuts.${label}`),
    scope,
  }));
}

/** True when focus is in something that owns its own text input — a
 *  shortcut must not hijack a keystroke meant for the field. Shared by
 *  every keydown handler in the app (previously duplicated per-route). */
export function isTypingContext(el: Element | null = document.activeElement): boolean {
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    (el as HTMLElement).isContentEditable
  );
}

/** True when no list/board/global shortcut should fire: typing in a field,
 *  or a modal that owns its own keyboard handling is open (the peek panel,
 *  the command palette, the shortcut help overlay, or — LIF-248 — the
 *  right-click context menu, whose own arrow/Enter/Esc handling would
 *  otherwise double-fire alongside a list's j/k row navigation). Deliberately
 *  NOT used by the "?" toggle handler in Layout.svelte — that one needs to
 *  keep firing while the help overlay is already open (so a second "?"
 *  press can close it) and checks the typing/peek/palette/context-menu
 *  conditions directly instead. */
export function shortcutsSuppressed(): boolean {
  return (
    isTypingContext() ||
    peekState.open ||
    commandPaletteState.open ||
    shortcutHelpState.open ||
    contextMenuState.open ||
    mobileNavState.open
  );
}

/**
 * グローバルホットキーの割り当て（`control+alt+KeyP`の形の文字列。ADR-122）
 *
 * 文字列の形は、Rust側（`global_shortcuts.rs`。global-hotkeyの`HotKey`）が読み書きする形にそろえる:
 * 修飾キーを`shift`・`control`・`alt`・`super`の順に`+`でつなぎ、最後に`KeyboardEvent.code`と同じ
 * 名前のキーを置く。
 */

/** 修飾キー（文字列での順） */
const MODIFIERS = [
  { name: 'shift', label: 'Shift', pressed: (event: KeyboardEventLike) => event.shiftKey },
  { name: 'control', label: 'Control', pressed: (event: KeyboardEventLike) => event.ctrlKey },
  { name: 'alt', label: 'Option', pressed: (event: KeyboardEventLike) => event.altKey },
  { name: 'super', label: 'Command', pressed: (event: KeyboardEventLike) => event.metaKey }
] as const;

/** 表示の順（macOSの⌃⌥⇧⌘の順） */
const DISPLAY_ORDER = ['control', 'alt', 'shift', 'super'] as const;

/** 割り当てに使えるキー（Rustのglobal-hotkeyが読めるキーのうち、よく使うもの）と表示名 */
const KEY_LABELS: Record<string, string> = {
  Period: '.',
  Comma: ',',
  Slash: '/',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Backslash: '\\',
  Minus: '-',
  Equal: '=',
  Backquote: '`',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Space: 'Space',
  Enter: 'Enter',
  Tab: 'Tab',
  Backspace: 'Backspace',
  Delete: 'Delete',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown'
};

/** キーの表示名（使えないキーはnull） */
function keyLabel(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit([0-9])$/.exec(code);
  if (digit) return digit[1];
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  if (/^Numpad[0-9]$/.test(code)) return code;
  return KEY_LABELS[code] ?? null;
}

/** 割り当ての記録に使う、キーボードの入力（テストで作れるように、必要な所だけ） */
export interface KeyboardEventLike {
  code: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** 押したキーの記録の結果 */
export type RecordedShortcut =
  | { kind: 'shortcut'; shortcut: string }
  /** 修飾キーだけを押している（続けてほかのキーを待つ） */
  | { kind: 'modifierOnly' }
  /** Control・Option・Commandのどれも押していない */
  | { kind: 'needsModifier' }
  /** 割り当てに使えないキー */
  | { kind: 'unsupported' };

/** 押したキーを、割り当ての文字列にする */
export function recordShortcut(event: KeyboardEventLike): RecordedShortcut {
  if (/^(Shift|Control|Alt|Meta|OS)(Left|Right)?$/.test(event.code))
    return { kind: 'modifierOnly' };
  if (keyLabel(event.code) === null) return { kind: 'unsupported' };
  if (!event.ctrlKey && !event.altKey && !event.metaKey) return { kind: 'needsModifier' };
  const modifiers = MODIFIERS.filter((modifier) => modifier.pressed(event)).map((m) => m.name);
  return { kind: 'shortcut', shortcut: [...modifiers, event.code].join('+') };
}

/** 割り当ての文字列を、表示するキーの名前にする（押す順。読めなければ空） */
export function shortcutKeyNames(shortcut: string): string[] {
  const parts = shortcut.split('+');
  const code = parts.pop() ?? '';
  const label = keyLabel(code);
  if (label === null) return [];
  const modifiers = new Set(parts.map((part) => part.toLowerCase()));
  const names = DISPLAY_ORDER.filter((name) => modifiers.has(name)).map(
    (name) => MODIFIERS.find((modifier) => modifier.name === name)?.label ?? name
  );
  return [...names, label];
}

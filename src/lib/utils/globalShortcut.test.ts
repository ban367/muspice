import { describe, expect, it } from 'vitest';
import { recordShortcut, shortcutKeyNames, type KeyboardEventLike } from './globalShortcut.js';

function key(code: string, modifiers: Partial<KeyboardEventLike> = {}): KeyboardEventLike {
  return { code, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, ...modifiers };
}

describe('recordShortcut', () => {
  it('押したキーを、Rust側と同じ形（shift・control・alt・superの順）の文字列にする', () => {
    expect(recordShortcut(key('KeyP', { ctrlKey: true, altKey: true }))).toEqual({
      kind: 'shortcut',
      shortcut: 'control+alt+KeyP'
    });
    expect(recordShortcut(key('ArrowUp', { metaKey: true, shiftKey: true }))).toEqual({
      kind: 'shortcut',
      shortcut: 'shift+super+ArrowUp'
    });
  });

  it('修飾キーだけ・修飾キーのないキー・使えないキーは、割り当てにしない', () => {
    expect(recordShortcut(key('ControlLeft', { ctrlKey: true }))).toEqual({
      kind: 'modifierOnly'
    });
    expect(recordShortcut(key('KeyP'))).toEqual({ kind: 'needsModifier' });
    expect(recordShortcut(key('KeyP', { shiftKey: true }))).toEqual({ kind: 'needsModifier' });
    expect(recordShortcut(key('IntlYen', { ctrlKey: true }))).toEqual({ kind: 'unsupported' });
  });
});

describe('shortcutKeyNames', () => {
  it('表示するキーの名前を、⌃⌥⇧⌘の順にする', () => {
    expect(shortcutKeyNames('control+alt+KeyP')).toEqual(['Control', 'Option', 'P']);
    expect(shortcutKeyNames('shift+super+ArrowUp')).toEqual(['Shift', 'Command', '↑']);
    expect(shortcutKeyNames('control+alt+Period')).toEqual(['Control', 'Option', '.']);
    expect(shortcutKeyNames('control+F5')).toEqual(['Control', 'F5']);
    expect(shortcutKeyNames('control+Nope')).toEqual([]);
  });
});

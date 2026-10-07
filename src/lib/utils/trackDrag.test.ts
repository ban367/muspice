import { describe, expect, it } from 'vitest';
import { TRACK_DRAG_TYPE, isTrackDrag, readDraggedTrackIds } from './trackDrag.js';

/** `dataTransfer`に指定したデータだけを持つドラッグイベント */
function dragEvent(data: Record<string, string>): DragEvent {
  return {
    dataTransfer: {
      types: Object.keys(data),
      getData: (type: string) => data[type] ?? ''
    }
  } as unknown as DragEvent;
}

describe('readDraggedTrackIds', () => {
  it('ドラッグしたトラックIDを順番どおりに返す', () => {
    const event = dragEvent({ [TRACK_DRAG_TYPE]: JSON.stringify(['t2', 't1', 't3']) });
    expect(readDraggedTrackIds(event)).toEqual(['t2', 't1', 't3']);
  });

  it('トラックのドラッグでない場合は空を返す', () => {
    expect(readDraggedTrackIds(dragEvent({ 'text/plain': 't1' }))).toEqual([]);
    expect(readDraggedTrackIds({ dataTransfer: null } as DragEvent)).toEqual([]);
  });

  it('壊れたデータ・文字列でない要素は捨てる', () => {
    expect(readDraggedTrackIds(dragEvent({ [TRACK_DRAG_TYPE]: '{' }))).toEqual([]);
    expect(readDraggedTrackIds(dragEvent({ [TRACK_DRAG_TYPE]: '"t1"' }))).toEqual([]);
    expect(readDraggedTrackIds(dragEvent({ [TRACK_DRAG_TYPE]: '["t1", 2, null]' }))).toEqual([
      't1'
    ]);
  });
});

describe('isTrackDrag', () => {
  it('トラックのドラッグだけをtrueにする', () => {
    expect(isTrackDrag(dragEvent({ [TRACK_DRAG_TYPE]: '[]' }))).toBe(true);
    expect(isTrackDrag(dragEvent({ Files: '' }))).toBe(false);
    expect(isTrackDrag({ dataTransfer: null } as DragEvent)).toBe(false);
  });
});

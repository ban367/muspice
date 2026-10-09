import { describe, expect, it } from 'vitest';
import type { Metadata } from '#lib/types/models.js';
import {
  NUMBER_FIELDS,
  TEXT_FIELDS,
  emptyForm,
  formFromTags,
  metadataFromForm,
  suggestionsFor,
  validateForm,
  type MetadataForm,
  type NumberField,
  type TextField
} from './metadataForm.js';

const labels = Object.fromEntries(
  [...TEXT_FIELDS, ...NUMBER_FIELDS].map((field) => [field, field])
) as Record<TextField | NumberField, string>;

const tags: Metadata = {
  title: 'タイトル',
  artist: 'アーティスト',
  album: 'アルバム',
  albumArtist: 'アルバムアーティスト',
  composer: '作曲者',
  genre: 'Jazz',
  grouping: 'グループ',
  comment: 'コメント',
  lyrics: '  字下げした1行目\n2行目\n',
  year: 2021,
  trackNumber: 3,
  trackTotal: 12,
  discNumber: 1,
  discTotal: 2,
  bpm: 128,
  compilation: true
};

describe('formFromTags', () => {
  it('タグの値を入力欄の値にし、タグにない項目は空にする', () => {
    const form = formFromTags({ title: '曲', trackNumber: 5 });

    expect(form.title).toBe('曲');
    expect(form.trackNumber).toBe('5');
    expect(form.artist).toBe('');
    expect(form.year).toBe('');
    expect(form.compilation).toBe('off');
    expect(formFromTags(tags).compilation).toBe('on');
  });
});

describe('metadataFromForm', () => {
  it('1曲の編集: 読んだタグをそのまま保存すると、同じ値になる', () => {
    expect(metadataFromForm(formFromTags(tags), 'single')).toEqual(tags);
  });

  it('1曲の編集: 空にした項目は値なしにする（タグから取り除かれる）', () => {
    const form: MetadataForm = {
      ...formFromTags(tags),
      artist: '  ',
      year: '',
      compilation: 'off'
    };

    const metadata = metadataFromForm(form, 'single');

    expect(metadata.artist).toBeUndefined();
    expect(metadata.year).toBeUndefined();
    expect(metadata.compilation).toBeUndefined();
    expect(metadata.title).toBe('タイトル');
  });

  it('前後の空白を除く（歌詞・コメントは、そのまま残す）', () => {
    const form: MetadataForm = {
      ...emptyForm(),
      title: '  曲  ',
      lyrics: '  1行目\n',
      comment: ' メモ '
    };

    expect(metadataFromForm(form, 'single')).toEqual({
      title: '曲',
      lyrics: '  1行目\n',
      comment: ' メモ '
    });
  });

  it('一括編集: 入力した項目だけを入れ、1曲ごとの項目は入れない', () => {
    const form: MetadataForm = {
      ...emptyForm(),
      title: '同じタイトル',
      trackNumber: '1',
      lyrics: '歌詞',
      albumArtist: 'Various Artists',
      discNumber: '2'
    };

    expect(metadataFromForm(form, 'bulk')).toEqual({
      albumArtist: 'Various Artists',
      discNumber: 2
    });
    expect(metadataFromForm(emptyForm(), 'bulk')).toEqual({});
  });

  it('一括編集: コンピレーションの印は、付ける・外す・変えないを選べる', () => {
    const form = emptyForm();

    expect(metadataFromForm(form, 'bulk').compilation).toBeUndefined();
    expect(metadataFromForm({ ...form, compilation: 'on' }, 'bulk').compilation).toBe(true);
    expect(metadataFromForm({ ...form, compilation: 'off' }, 'bulk').compilation).toBe(false);
  });
});

describe('validateForm', () => {
  it('範囲の中の入力は、誤りなし', () => {
    expect(validateForm(formFromTags(tags), labels)).toEqual([]);
    expect(validateForm(emptyForm(), labels)).toEqual([]);
  });

  it('数値の範囲の外・整数でない入力は、誤りにする', () => {
    const form: MetadataForm = {
      ...emptyForm(),
      year: '999',
      trackNumber: '0',
      discTotal: '1000',
      bpm: '12.5',
      trackTotal: 'abc'
    };

    expect(validateForm(form, labels)).toHaveLength(5);
  });

  it('長すぎる文字列は、誤りにする（文字数で数える）', () => {
    const form: MetadataForm = { ...emptyForm(), genre: 'あ'.repeat(100), title: '🎵'.repeat(255) };
    expect(validateForm(form, labels)).toEqual([]);

    expect(validateForm({ ...form, genre: 'あ'.repeat(101) }, labels)).toHaveLength(1);
  });
});

describe('suggestionsFor', () => {
  const values = ['Aoi Sora', 'The Voltage', 'ネオン通り', 'aoi', 'Blue Aoi'];

  it('入力を含む値を、大文字と小文字を区別せずに返す', () => {
    expect(suggestionsFor(values, 'AOI')).toEqual(['Aoi Sora', 'Blue Aoi']);
    expect(suggestionsFor(values, 'ネオン')).toEqual(['ネオン通り']);
  });

  it('入力が空・候補がない・一覧がない場合は、何も返さない', () => {
    expect(suggestionsFor(values, '  ')).toEqual([]);
    expect(suggestionsFor(values, 'zzz')).toEqual([]);
    expect(suggestionsFor(undefined, 'a')).toEqual([]);
  });

  it('候補の数を制限する', () => {
    const many = Array.from({ length: 100 }, (_, index) => `Artist ${index}`);
    expect(suggestionsFor(many, 'artist', 5)).toHaveLength(5);
  });
});

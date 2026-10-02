/**
 * ブラウザモック用のメディア生成（アルバムアート・音声）
 *
 * 実ファイルを用意せずに済むよう、文字列から決定的に生成する。
 */
/** 文字列から決定的なハッシュ値を得る（FNV-1a） */
export function hashString(value: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i++) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/**
 * アルバム名ごとに色の異なるグラデーション画像をdata URLで生成する
 *
 * SVGはASCIIのみで構成し、`btoa`でそのままBase64化できるようにする。
 */
export function createAlbumArt(albumName: string): string {
  const hash = hashString(albumName);
  const startHue = hash % 360;
  const endHue = (startHue + 40 + ((hash >>> 9) % 80)) % 360;
  const svg =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120">' +
    '<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">' +
    `<stop offset="0" stop-color="hsl(${startHue},65%,55%)"/>` +
    `<stop offset="1" stop-color="hsl(${endHue},70%,28%)"/>` +
    '</linearGradient></defs>' +
    '<rect width="120" height="120" fill="url(#g)"/>' +
    '<circle cx="60" cy="60" r="34" fill="none" stroke="rgba(255,255,255,0.35)" stroke-width="2"/>' +
    '<circle cx="60" cy="60" r="6" fill="rgba(255,255,255,0.6)"/>' +
    '</svg>';
  return `data:image/svg+xml;base64,${btoa(svg)}`;
}

const SAMPLE_RATE = 8000;
/** モック音声の長さ（秒）。トラックの`duration`とは一致しない */
export const MOCK_AUDIO_SECONDS = 20;
const NOTE_SECONDS = 0.5;
const BASE_FREQUENCIES_HZ = [220, 246.94, 277.18, 329.63, 369.99, 440];
/** 根音・長3度・完全5度・オクターブのアルペジオ */
const ARPEGGIO_RATIOS = [1, 1.25, 1.5, 2];

/**
 * シード文字列ごとに音程の異なるアルペジオのWAV（16bit PCM・モノラル）を生成する
 *
 * 各音の始まりと終わりで振幅を0にし、音の切り替わりでノイズが出ないようにする。
 */
export function createToneWav(seed: string): ArrayBuffer {
  const baseFrequency = BASE_FREQUENCIES_HZ[hashString(seed) % BASE_FREQUENCIES_HZ.length];
  const sampleCount = SAMPLE_RATE * MOCK_AUDIO_SECONDS;
  const dataSize = sampleCount * 2;
  const buffer = new ArrayBuffer(44 + dataSize);
  const view = new DataView(buffer);
  const writeAscii = (offset: number, text: string) => {
    for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
  };

  writeAscii(0, 'RIFF');
  view.setUint32(4, 36 + dataSize, true);
  writeAscii(8, 'WAVE');
  writeAscii(12, 'fmt ');
  view.setUint32(16, 16, true); // fmtチャンクのサイズ
  view.setUint16(20, 1, true); // リニアPCM
  view.setUint16(22, 1, true); // モノラル
  view.setUint32(24, SAMPLE_RATE, true);
  view.setUint32(28, SAMPLE_RATE * 2, true); // バイトレート
  view.setUint16(32, 2, true); // ブロックサイズ
  view.setUint16(34, 16, true); // ビット深度
  writeAscii(36, 'data');
  view.setUint32(40, dataSize, true);

  for (let i = 0; i < sampleCount; i++) {
    const time = i / SAMPLE_RATE;
    const note = Math.floor(time / NOTE_SECONDS);
    const envelope = Math.sin((Math.PI * (time % NOTE_SECONDS)) / NOTE_SECONDS);
    const frequency = baseFrequency * ARPEGGIO_RATIOS[note % ARPEGGIO_RATIOS.length];
    const sample = Math.sin(2 * Math.PI * frequency * time) * envelope * 0.15;
    view.setInt16(44 + i * 2, Math.round(sample * 0x7fff), true);
  }

  return buffer;
}

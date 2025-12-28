import { writable } from 'svelte/store';

export interface ErrorNotification {
	id: string;
	message: string;
	type: 'error' | 'warning' | 'info';
	timestamp: Date;
}

function createErrorStore() {
	const { subscribe, update } = writable<ErrorNotification[]>([]);

	return {
		subscribe,
		addError: (message: string, type: 'error' | 'warning' | 'info' = 'error') => {
			const notification: ErrorNotification = {
				id: crypto.randomUUID(),
				message,
				type,
				timestamp: new Date()
			};

			update((errors) => [...errors, notification]);

			// 5秒後に自動削除
			setTimeout(() => {
				update((errors) => errors.filter((e) => e.id !== notification.id));
			}, 5000);
		},
		removeError: (id: string) => {
			update((errors) => errors.filter((e) => e.id !== id));
		},
		clear: () => {
			update(() => []);
		}
	};
}

export const errorStore = createErrorStore();

/**
 * グローバルエラーハンドラー
 * Tauriコマンドのエラーをユーザーフレンドリーなメッセージに変換
 */
export function handleError(error: unknown, context?: string): void {
	let message = 'エラーが発生しました';

	if (typeof error === 'string') {
		message = error;
	} else if (error instanceof Error) {
		message = error.message;
	}

	// コンテキストがある場合は追加
	if (context) {
		message = `${context}: ${message}`;
	}

	// エラーメッセージをユーザーフレンドリーに変換
	message = convertToUserFriendlyMessage(message);

	// エラーストアに追加
	errorStore.addError(message, 'error');

	// コンソールにも出力（開発用）
	console.error('[Error]', context || '', error);
}

/**
 * エラーメッセージをユーザーフレンドリーに変換
 */
function convertToUserFriendlyMessage(message: string): string {
	// データベース関連のエラー
	if (message.includes('データベース')) {
		return 'データベースの操作中にエラーが発生しました。もう一度お試しください。';
	}

	// ファイル関連のエラー
	if (message.includes('ファイルが見つかりません')) {
		return '指定されたファイルが見つかりません。ファイルが移動または削除された可能性があります。';
	}

	if (message.includes('サポートされていないファイル形式')) {
		return 'このファイル形式はサポートされていません。MP3、FLAC、WAV、M4Aファイルのみ対応しています。';
	}

	// メタデータ関連のエラー
	if (message.includes('メタデータ')) {
		return 'メタデータの処理中にエラーが発生しました。ファイルが破損している可能性があります。';
	}

	// プレイリスト関連のエラー
	if (message.includes('プレイリスト')) {
		return 'プレイリストの操作中にエラーが発生しました。もう一度お試しください。';
	}

	// トラック関連のエラー
	if (message.includes('トラックが見つかりません')) {
		return '指定された楽曲が見つかりません。';
	}

	// バリデーション関連のエラー
	if (message.includes('バリデーション')) {
		return '入力内容に問題があります。入力内容を確認してください。';
	}

	// ロック関連のエラー
	if (message.includes('ロック')) {
		return '処理が競合しています。しばらく待ってからもう一度お試しください。';
	}

	// デフォルトメッセージ
	return message;
}

/**
 * 成功メッセージを表示
 */
export function showSuccess(message: string): void {
	errorStore.addError(message, 'info');
}

/**
 * 警告メッセージを表示
 */
export function showWarning(message: string): void {
	errorStore.addError(message, 'warning');
}

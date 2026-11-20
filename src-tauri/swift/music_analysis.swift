// 曲の解析（テンポ・キー・ペース）。macOS 27のMusicUnderstanding（端末内で動くAppleの楽曲解析）を、
// Rust（src/music_analysis.rs）からC言語の関数として呼べるようにする。
//
// ビルドはbuild.rsが行う（SDKにMusicUnderstandingがある時だけ）。アプリの対象のmacOSの版は27より古いため、
// MusicUnderstandingを使う処理はすべて`@available(macOS 27, *)`の中に置き、フレームワークは弱く
// リンクする（古いmacOSでも起動でき、解析だけが使えない）。

import AVFoundation
import Foundation
import MusicUnderstanding

/// 解析の結果の種類（Rustの`music_analysis::ffi`の定数と同じ値）
private enum Status: Int32 {
    /// 解析できた
    case analyzed = 0
    /// このmacOSでは使えない（macOS 27より古い）
    case unavailable = 1
    /// ファイルを音声として開けない（AVFoundationが対応していない形式・音声のないファイル）
    case unsupported = 2
    /// 解析に失敗した
    case failed = 3
}

/// 1曲の解析の結果（値のない項目は、BPM・ペースがNaN、キーが-1）
private struct Features: Sendable {
    var bpm: Double = .nan
    /// 主音のピッチクラス（C = 0, C# = 1, ..., B = 11）
    var keyPitchClass: Int32 = -1
    /// 0 = 長調、1 = 短調
    var keyMode: Int32 = -1
    /// 曲の速さの感じ（区間ごとの値を、長さで重みを付けて平均した値）
    var pace: Double = .nan
}

/// 非同期の処理の結果を、待っているスレッドへ渡す入れ物（書くのは1回だけで、セマフォの後に読む）
private final class ResultBox: @unchecked Sendable {
    var status: Status = .failed
    var features = Features()
}

/// このmacOSで曲の解析を使えるか
@_cdecl("muspice_music_analysis_available")
public func musicAnalysisAvailable() -> Bool {
    if #available(macOS 27, *) {
        return true
    }
    return false
}

/// 1曲を解析する（終わるまで返らない。Rustのバックグラウンドのスレッドから呼ぶ）
///
/// - Parameters:
///   - path: ファイルのパス（UTF-8）
///   - bpm / keyPitchClass / keyMode / pace: 結果を書き込む先（解析できた時だけ書く）
/// - Returns: `Status`の値
@_cdecl("muspice_music_analysis_analyze")
public func musicAnalysisAnalyze(
    _ path: UnsafePointer<CChar>,
    _ bpm: UnsafeMutablePointer<Double>,
    _ keyPitchClass: UnsafeMutablePointer<Int32>,
    _ keyMode: UnsafeMutablePointer<Int32>,
    _ pace: UnsafeMutablePointer<Double>
) -> Int32 {
    guard #available(macOS 27, *) else {
        return Status.unavailable.rawValue
    }
    let url = URL(fileURLWithPath: String(cString: path))
    let box = ResultBox()
    let semaphore = DispatchSemaphore(value: 0)
    // 再生（リアルタイムのスレッド）を妨げないよう、優先度を下げて実行する
    Task.detached(priority: .utility) {
        let (status, features) = await analyzeFile(url)
        box.status = status
        box.features = features
        semaphore.signal()
    }
    semaphore.wait()

    if box.status == .analyzed {
        bpm.pointee = box.features.bpm
        keyPitchClass.pointee = box.features.keyPitchClass
        keyMode.pointee = box.features.keyMode
        pace.pointee = box.features.pace
    }
    return box.status.rawValue
}

@available(macOS 27, *)
private func analyzeFile(_ url: URL) async -> (Status, Features) {
    let asset = AVURLAsset(url: url)
    do {
        // 音声のトラックがない（AVFoundationが読めない形式）なら、解析しない
        let tracks = try await asset.loadTracks(withMediaType: .audio)
        if tracks.isEmpty {
            return (.unsupported, Features())
        }
    } catch {
        return (.unsupported, Features())
    }

    do {
        let session = try await MusicUnderstandingSession(asset: asset)
        let result = try await session.analyze(for: [.key, .rhythm, .pace])
        var features = Features()
        if let value = result.rhythm?.beatsPerMinute, value.isFinite, value > 0 {
            features.bpm = Double(value)
        }
        if let key = dominantKey(result.key) {
            features.keyPitchClass = Int32(key.pitchClass)
            features.keyMode = key.isMinor ? 1 : 0
        }
        if let value = weightedPace(result.pace) {
            features.pace = value
        }
        return (.analyzed, features)
    } catch let error as MusicUnderstandingError where error == .invalidAsset {
        return (.unsupported, Features())
    } catch {
        return (.failed, Features())
    }
}

/// 区間の長さの合計が最も長いキー（同じ長さなら、先に出てきたキー）
@available(macOS 27, *)
private func dominantKey(_ result: KeyResult?) -> (pitchClass: Int, isMinor: Bool)? {
    guard let ranges = result?.ranges else { return nil }
    var totals: [Int: Double] = [:]
    var order: [Int] = []
    for range in ranges {
        let duration = range.range.duration.seconds
        guard duration.isFinite, duration > 0 else { continue }
        // 主音（0〜11）と長調・短調を、1つの値にまとめる（短調は12を足す）
        let code = pitchClass(of: range.value.tonic) + (range.value.mode == .minor ? 12 : 0)
        if totals[code] == nil {
            order.append(code)
        }
        totals[code, default: 0] += duration
    }
    var best: (code: Int, total: Double)?
    for code in order {
        let total = totals[code, default: 0]
        if let current = best, total <= current.total {
            continue
        }
        best = (code, total)
    }
    guard let code = best?.code else { return nil }
    return (code % 12, code >= 12)
}

/// 区間ごとのペースを、区間の長さで重みを付けた平均（値がなければnil）
@available(macOS 27, *)
private func weightedPace(_ result: PaceResult?) -> Double? {
    guard let ranges = result?.ranges else { return nil }
    var total = 0.0
    var weighted = 0.0
    for range in ranges {
        let weight = range.range.duration.seconds
        guard range.value.isFinite, weight.isFinite, weight > 0 else { continue }
        total += weight
        weighted += range.value * weight
    }
    return total > 0 ? weighted / total : nil
}

@available(macOS 27, *)
private func pitchClass(of tonic: KeyResult.Tonic) -> Int {
    switch tonic {
    case .c: 0
    case .cSharp, .dFlat: 1
    case .d: 2
    case .dSharp, .eFlat: 3
    case .e: 4
    case .f: 5
    case .fSharp, .gFlat: 6
    case .g: 7
    case .gSharp, .aFlat: 8
    case .a: 9
    case .aSharp, .bFlat: 10
    case .b: 11
    }
}

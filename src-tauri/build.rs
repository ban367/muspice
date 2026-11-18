use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    // 曲の解析（MusicUnderstanding）を組み込んだかを、Rustのコードへ伝える印（`music_analysis.rs`）
    println!("cargo::rustc-check-cfg=cfg(music_understanding)");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        match build_music_analysis() {
            Ok(None) => println!("cargo::rustc-cfg=music_understanding"),
            // 組み込めない環境（MusicUnderstandingのないSDK）でもビルドは続ける（解析だけが使えない）
            Ok(Some(reason)) => println!("cargo::warning=Music analysis is not built in: {reason}"),
            Err(e) => panic!("Failed to build the music analysis: {e}"),
        }
    }

    tauri_build::build()
}

/// 曲の解析のSwiftのコード（`swift/music_analysis.swift`）を静的ライブラリにビルドし、リンクする
///
/// MusicUnderstandingはmacOS 27のSDK（Xcode 27）にしかない。SDKにない時は、何もせずに理由を返す
/// （`Ok(Some(理由))`）。SDKにあるのにビルドできない時は、エラーにする。
/// アプリの対象のmacOSの版はそれより古いため、MusicUnderstandingと、Swiftの並行処理の実行環境は
/// 弱くリンクする（古いmacOSでも起動できる）。
fn build_music_analysis() -> Result<Option<String>, String> {
    let source = Path::new("swift/music_analysis.swift");
    println!("cargo::rerun-if-changed={}", source.display());
    println!("cargo::rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo::rerun-if-env-changed=MACOSX_DEPLOYMENT_TARGET");
    println!("cargo::rerun-if-env-changed=MUSPICE_SKIP_MUSIC_ANALYSIS");
    // 組み込まない側（CIのmacOS・Linuxと同じ）を、手元で確かめる時に使う
    if env::var_os("MUSPICE_SKIP_MUSIC_ANALYSIS").is_some() {
        return Ok(Some("MUSPICE_SKIP_MUSIC_ANALYSIS is set".to_string()));
    }

    let sdk = run("xcrun", &["--sdk", "macosx", "--show-sdk-path"])?;
    let sdk = sdk.trim();
    if !Path::new(sdk)
        .join("System/Library/Frameworks/MusicUnderstanding.framework")
        .exists()
    {
        return Ok(Some(format!(
            "the SDK at {sdk} has no MusicUnderstanding framework (Xcode 27 or later is required)"
        )));
    }

    let (arch, default_deployment) = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => ("arm64", "11.0"),
        Ok("x86_64") => ("x86_64", "10.13"),
        Ok(other) => return Err(format!("unsupported architecture: {other}")),
        Err(e) => return Err(e.to_string()),
    };
    // Rustと同じ対象の版でビルドする（Tauriのビルドは、設定の版を環境変数で渡す）
    let deployment =
        env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| default_deployment.to_string());
    let target = format!("{arch}-apple-macos{deployment}");

    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);
    let library = out_dir.join("libmuspice_music_analysis.a");
    run(
        "xcrun",
        &[
            "--sdk",
            "macosx",
            "swiftc",
            "-parse-as-library",
            "-emit-library",
            "-static",
            "-O",
            "-module-name",
            "MuspiceMusicAnalysis",
            "-target",
            &target,
            "-sdk",
            sdk,
            "-o",
            &library.display().to_string(),
            &source.display().to_string(),
        ],
    )?;

    // Swiftの実行環境（OSのライブラリと、古い版向けの互換用のライブラリ）の場所
    let info = run(
        "xcrun",
        &[
            "--sdk",
            "macosx",
            "swiftc",
            "-target",
            &target,
            "-print-target-info",
        ],
    )?;
    let info: serde_json::Value = serde_json::from_str(&info).map_err(|e| e.to_string())?;
    let runtime_paths = info["paths"]["runtimeLibraryPaths"]
        .as_array()
        .ok_or("the Swift target info has no runtime library paths")?;
    for path in runtime_paths.iter().filter_map(|path| path.as_str()) {
        println!("cargo::rustc-link-search=native={path}");
    }
    // `#available`の判定（`__isPlatformVersionAtLeast`）は、clangの実行時ライブラリにある
    let resource_dir = run("xcrun", &["clang", "--print-resource-dir"])?;
    println!(
        "cargo::rustc-link-search=native={}/lib/darwin",
        resource_dir.trim()
    );
    println!("cargo::rustc-link-lib=static=clang_rt.osx");

    println!("cargo::rustc-link-search=native={}", out_dir.display());
    println!("cargo::rustc-link-lib=static=muspice_music_analysis");
    println!("cargo::rustc-link-arg=-Wl,-weak_framework,MusicUnderstanding");
    println!("cargo::rustc-link-arg=-Wl,-weak-lswift_Concurrency");
    // 古い版の対象でビルドしたSwiftのコードは、並行処理の実行環境を`@rpath`で探す（OSのものを使う）
    println!("cargo::rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    Ok(None)
}

/// コマンドを実行して、標準出力を返す（失敗した時は、標準エラー出力を理由にする）
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run {program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} failed: {}",
            args.first().copied().unwrap_or_default(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}

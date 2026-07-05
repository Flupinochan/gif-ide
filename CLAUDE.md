# CLAUDE.md

## 重要

- `cargo build` の実行は不要です。時間がかかるため代わりに `cargo check` を実行してください
- `Cargo.toml` に新しい依存関係を勝手に追加しないでください。追加が必要な場合は、追加理由を説明したうえでユーザに許可を取ってください。また追加する場合は最新バージョンを確認して追加してください

## Commands

| コマンド      | 用途                   |
| ------------- | ---------------------- |
| `cargo check` | コンパイルエラーの確認 |
| `cargo test`  | 単体テスト実行         |

## Architecture

このプロジェクトは **Slint + Rust** による GIF再生・編集アプリケーションです

GIFの構造・画像処理・ffmpeg などのドメイン知識は `GUIDE.md` にまとまっています

GIF export、差分矩形、Disposal Method、並行処理まわりを触る前に該当箇所を読むこと

### ファイル構成

| ファイル          | 説明                                                                                        |
| ----------------- | ------------------------------------------------------------------------------------------- |
| lang/             | 翻訳・多言語対応                                                                            |
| ui/               | slintを使用したUI                                                                           |
| ui/preview        | 動作確認用                                                                                  |
| src/              | Rustロジック (main.rs がエントリポイント、1ウィンドウ = 1モジュール)                        |
| tests/fixtures    | テスト用GIF (sample.gif)                                                                    |
| docs/             | GitHub Pages (landing、privacy policy) とSlint学習メモ (development/)                       |
| GUIDE.md          | GIF・画像・ffmpegのドメイン知識                                                             |
| .github/workflows | 配布用ビルドとRelease作成 (release.yml、actでローカル実行可)、カバレッジ計測 (coverage.yml) |
| resources/ffmpeg  | 同梱するffmpegバイナリ (git管理外、workflowで配置)                                          |

## Coding Rule

- UI側で有効な値のみRustに渡す
  - Rust callback は **有効なインデックスを受け取って画像を表示する** という単一責任になる
    - "Make illegal states unrepresentable" の原則
    - Rust callback に無効なインデックスを渡せない構造にすると、**防御的な else ブランチが不要になる**
- UI操作はcallbackとしてのみRustへ通知し、Slint側で業務データ (frames等) を直接書き換えない
  - View → callback → Rustが正規データを更新 → set_xxxでUIに反映の一方通行
- 業務データを持つpropertyは `in-out` ではなく `in` にする (Rustのみ書き込み可、Slint側は読み取り専用にして直接書き換えを構文的に防ぐ)
- Rust側 (`GifFile`) を唯一の正規データとして保持し、UIの表示用モデル (`frames`等) はそこから都度生成する

## 命名規則

- `as_weak()` で取得した変数は `<元の変数名>_weak` の接尾辞で命名する (例: `ui.as_weak()` → `ui_weak`)
- 同じコンポーネントから複数のcallback向けにweak参照を複製する場合は、対応するcallback名をさらに付与する (例: `on_play` 用 → `ui_weak_play`)

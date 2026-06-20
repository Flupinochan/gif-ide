# CLAUDE.md

コード生成する場合に、簡単な一部のコードはTODOとしてユーザに作成させるようにしてください
ユーザに求められない限りコード修正は不要です
コード例を標準出力するようにしてください
コードを修正・復元する前に必ず Read ツールで最新のファイル内容を確認してから行ってください
cargo buildも不要です。無意味のためです。コンパイルエラーはvscode画面上で確認できます
Cargo.toml の dependencies / build-dependencies に新しい依存関係を勝手に追加しないでください。追加が必要な場合は、追加理由を説明したうえで必ずユーザに確認を取ってください


## Architecture

このプロジェクトは **Slint + Rust** による GIF再生・編集アプリケーションです

### ファイル構成

| ファイル | 説明              |
| -------- | ----------------- |
| ui/      | slintを使用したUI |
| src/     | Rustロジック      |

### データフロー方針

- UI操作はcallbackとしてのみRustへ通知し、Slint側で業務データ (frames等) を直接書き換えない (View → callback → Rustが正規データを更新 → set_xxxでUIに反映、の一方通行)
- 業務データを持つpropertyは `in-out` ではなく `in` にする (Rustのみ書き込み可、Slint側は読み取り専用にして直接書き換えを構文的に防ぐ)
- Rust側 (`GifFile`) を唯一の正規データとして保持し、UIの表示用モデル (`frames`等) はそこから都度生成する

### 命名規則

- `as_weak()` で取得した変数は `<元の変数名>_weak` の接尾辞で命名する (例: `ui.as_weak()` → `ui_weak`)
- 同じコンポーネントから複数のcallback向けにweak参照を複製する場合は、対応するcallback名をさらに付与する (例: `on_play` 用 → `ui_weak_play`)

[![codecov](https://codecov.io/gh/Flupinochan/gif-ide/branch/main/graph/badge.svg?token=2v7YK8XmlH)](https://codecov.io/gh/Flupinochan/gif-ide)

# GIF IDE

GIFを再生・編集・出力するGUIツール

## モチベーション

ffmpeg CLIで行っていた **GIFの軽量化** をGUIツールにして誰でも簡単に行えるようにしたかった

使用例としては、動画を読み込み、サイズを縮小化し、フレーム数を落とし、アニメーションのdelay間隔を増やすことで、元の動画よりも軽いGIFを出力することができます

私が利用しているZennブログでは、GIFのアップロードサイズが5MB以下という制限があるのですが、これを容易に満たせるようになりました

## 機能詳細

- 入力
  - GIF
  - 動画
- 再生
  - 1画像ずつコマ送りで表示するプレビュー
  - リピート再生のON/OFF
  - フレームタイムライン
- 編集
  - フレームの間引き
  - フレームのdelay間隔
  - 幅・高さのリサイズ
    - Nearest
    - Triangle
    - CatmullRom
    - Gaussian
    - Lanczos3
- 出力
  - フォーマット
    - GIF
    - PNG
    - JPEG
    - WEBP
    - BMP
    - ICO
    - AVIF
  - GIF出力オプション
    - ループ再生のON・OFF
    - 差分矩形出力によるファイルサイズ軽量化
  - 静止画出力オプション
    - 出力範囲の選択 (1フレーム・全フレーム)

## 今後実装したい機能

あったらいい機能ですが自分が利用していないため、実装しない可能性も高いです...

- tracingでエラーログを出力してトラブル対応を可能にする
- 動画のストリーミング読み込みへの変更
- 背景透過機能
- 他OS (Mac・Linux) への対応


  - ffmpeg binary
    - [windows 8.1 lgpl](https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-win64-lgpl-8.1.zip)

## ロジックについて

[GUIDE.md](./GUIDE.md) を参照

## ローカル開発環境セットアップ

### 初回作業

`resources/ffmpeg/*.exe` の `ffmpeg` を利用しますが、リポジトリにはpushしていないため、ダウンロードする必要があります

GitHub Actionsをローカルで実行すればダウンロードされます

```powershell
winget install nektos.act

act workflow_dispatch -W .github/workflows/release.yml -P windows-latest=-self-hosted --env HOST_DIST_DIR="$PWD\dist"
```

### ローカルでのアプリ起動

```bash
cargo run
```

### ローカルでのリリースビルド

GitHub Actionsをローカルで実行するための `act` を利用

コマンドは [初回作業](#初回作業) 記載

## リリース手順

GitHubのActionsタブから `Release` workflowを手動実行

Release Noteが自動生成される

★必ずtomlファイルのバージョンを更新してから実行すること

ffmpegは [BtbN FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) の **LGPL版** を使用

以下が生成される。zipを展開してそのまま利用可能

```
dist/
├── gif-ide/
│   ├── gif-ide.exe
│   └── ffmpeg/
│       ├── ffmpeg.exe
│       ├── ffprobe.exe
│       └── LICENSE.txt
└── gif-ide-v<version>-win64.zip
```

## テスト手順

### カバレッジ計測

GitHub Actionsでcodecovを利用

ローカルでは以下で実行可能

```bash
# 初回のみ
cargo install cargo-llvm-cov

# ターミナルにサマリー表示
cargo llvm-cov
# target/llvm-cov/html/ にHTMLレポート生成
cargo llvm-cov --html
```

### 画像を「すべて」で出力する際の `並行処理` のパフォーマンス計測

無制限平行はリスクがあるため、goroutineに相当するストリーミング平行で実装

1. 1枚ずつ逐次出力: 並行化なし
2. 全フレームを一度に並行出力: 同時実行数の制限なし
3. CPUコア数ごとチャンク分割して並行出力 (旧実装): 「Nコア分処理 → 完了待ち (バリア) → 次のNコア分」を繰り返す
4. 現在の実装: ストリーミング並行出力。1タスク完了ごとに次の1タスクを即座に投入し、常にCPUコア数分を並行実行 (バリアなし)

実行方法

```powershell
# GIFファイルのパスは環境変数 `TEST_GIF_PATH` で指定
$env:TEST_GIF_PATH = "C:\path\to\test.gif"
cargo test --release compare_export_strategies -- --ignored --nocapture
```

#### 結果例 (31フレーム、CPU論理コア数 = 8)

| 戦略                                  | 所要時間 |
| ------------------------------------- | -------- |
| 1. 1枚ずつ逐次出力                    | 48.49ms  |
| 2. 全フレーム並行出力 (無制限)        | 32.60ms  |
| 3. CPUコア数ごとチャンク並行 (旧実装) | 24.89ms  |
| 4. ストリーミング並行 (現在の実装)    | 24.53ms  |

#### 結果例 (221フレーム、CPU論理コア数 = 8)

| 戦略                                  | 所要時間 |
| ------------------------------------- | -------- |
| 1. 1枚ずつ逐次出力                    | 434.75ms |
| 2. 全フレーム並行出力 (無制限)        | 151.02ms |
| 3. CPUコア数ごとチャンク並行 (旧実装) | 312.99ms |
| 4. ストリーミング並行 (現在の実装)    | 231.03ms |

#### 出力が表示されない場合 (PowerShell)

ファイルにリダイレクトして確認

```powershell
cargo test --release compare_export_strategies -- --ignored --nocapture *> bench_output.txt
Get-Content bench_output.txt
```

## Coding Rule

- UI側で有効な値のみRustに渡す
  - Rust callback は **有効なインデックスを受け取って画像を表示する** という単一責任になる
    - "Make illegal states unrepresentable" の原則
    - Rust callback に無効なインデックスを渡せない構造にすると、**防御的な else ブランチが不要になる**

## UIスレッドを止めないために

### 基本方針

`callback` 処理はUIスレッド上で実行される

UIが固まらないようにするには、重い処理を `tokio spawn_blocking` 等で別スレッドで実行させると良い

| 関数                            | 実行場所                 | 主な用途                                                                                   | Tokio利用時の使用頻度 |
| ------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------ | --------------------- |
| `tokio::task::spawn_blocking`   | ブロッキング専用スレッド | CPU処理・同期I/Oなど時間のかかる処理を実行する                                             | ★★★（よく使う）       |
| `slint::invoke_from_event_loop` | UIスレッド上             | 別スレッドからUIを更新する                                                                 | ★★★（よく使う）       |
| `slint::spawn_local`            | UIスレッド上             | UIスレッド上で`async`処理を開始する（Tokioがない場合、画面が固まるため利用すべきではない） | ★☆☆（使わない）       |

### callback処理のコード雛形

```rust
// callback登録前にweak参照を用意 (callback内でムーブするため)
let ui_weak_some_action = ui.as_weak();
let window_weak_some_action = window.as_weak();
let data_ref_some_action = data_ref.clone();

window.on_some_action(move || {
    // callback自体はUIスレッド上で実行される
    let (Some(ui), Some(window)) = (ui_weak_some_action.upgrade(), window_weak_some_action.upgrade()) else {
        return;
    };

    // UIから入力値を取得
    let param = window.get_xxx();

    // 業務データを取得
    let Some(data) = data_ref_some_action.borrow().clone() else {
        return;
    };

    // UIスレッド上なので直接setしてよい
    window.set_state(LoadingState::Processing);

    // 別スレッドに渡すためweakを再取得
    let ui_weak_inner = ui.as_weak();
    let window_weak_inner = window.as_weak();
    let data_ref_inner = data_ref_some_action.clone();

    // 重い処理は別スレッドで実行
    tokio::task::spawn_blocking(move || {
        // TODO: 重い処理

        // 結果反映のみUIスレッドへ戻す
        let _ = slint::invoke_from_event_loop(move || {
            let (Some(ui), Some(window)) = (ui_weak_inner.upgrade(), window_weak_inner.upgrade()) else {
                return;
            };

            // TODO: UI更新

            window.set_state(LoadingState::Success);
        });
    });
});
```

### Rc / Arc

上記の雛形で `spawn_blocking` / `invoke_from_event_loop` のクロージャに `gif_file_ref` 等を直接持ち込む場合、`Rc` では他スレッドへ渡せずコンパイルエラーになるため `Arc` を利用

| 項目                   | Rc                                  | Arc                                 |
| ---------------------- | ----------------------------------- | ----------------------------------- |
| 役割                   | 複数ownerでデータを共有する         | 複数ownerでデータを共有する         |
| 参照カウントの仕組み   | 同じ (cloneで+1、dropで-1、0で解放) | 同じ (cloneで+1、dropで-1、0で解放) |
| 対象スレッド           | シングルスレッド専用                | マルチスレッド対応                  |
| カウンタ操作           | 通常の演算 (非アトミック)           | アトミック命令                      |
| 他スレッドへ渡せるか   | ✕ 不可 (コンパイルエラー)           | ◯ 可能                              |
| 中身を可変共有する相方 | RefCell                             | Mutex                               |

### spawn / spawn_blocking

判断基準は「その処理はFutureか、それとも呼び出すと止まる同期関数か」

| 観点           | `tokio::spawn`                                                                                       | `tokio::task::spawn_blocking`                                                                                 |
| -------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| 渡すもの       | `Future` (`async move {}`、または `.await` できる非同期処理)                                         | 同期関数・クロージャ (`.await` 不可)                                                                          |
| 実行先         | 少数の共有ワーカースレッド (協調スケジューリング)                                                    | ブロッキング専用スレッドプール (タスクごとに専有)                                                             |
| 向いている処理 | 非同期APIが返すFutureを待つ処理                                                                      | スレッドを止めてしまう同期処理                                                                                |
| 具体例         | `reqwest::get().await`、`tokio::fs::read().await`、async対応DBドライバ、`JoinSet` の `.await` ループ | CPU負荷計算、`std::fs`、同期DBドライバ、同期HTTPクライアント (`reqwest::blocking` 等)、画像エンコードなど     |
| 誤用した場合   | (該当処理が非同期なら問題なし)                                                                       | 中で `.await` しようとするとコンパイルエラー、または無理に `block_on` するとアンチパターン/デッドロックの危険 |
| 逆側でやると   | 同期処理を `spawn` 内に直接書くと、共有ワーカースレッドを塞いで他タスクも止まる                      | 非同期Futureを `spawn_blocking` に渡すには無理にブロックして待つ必要があり、スレッドを無駄に専有する          |

「I/O/HTTP処理だから `spawn`」ではない点に注意

同じHTTPやファイルI/Oでも、ライブラリが非同期API (Futureを返す) か同期API (呼び出したスレッドをブロックする) かで使い分けが変わる

例: `reqwest::get` は非同期だが `reqwest::blocking::get` は同期

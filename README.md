# やることリスト

- tracingでログ出力
- [svg](https://lucide.dev/icons/skip-forward)
- ICO出力: 幅・高さは1..=256の制約があるため、imageops::resizeでアスペクト比を保ったまま縮小し、
  256x256/128x128/64x64/32x32などのサイズをユーザーに選択させるUIが必要

## Coding Rule

- UI側で有効な値のみRustに渡すようにする
  - Rust callback は「有効なインデックスを受け取って画像を表示する」という単一責任になる
  - "Make illegal states unrepresentable" の原則: Rust callback に無効なインデックスを渡せない構造にすると、防御的な else ブランチが不要になる

## GIF

- Imageのvec![]であり、アニメーション間隔のdelayなども含まれる
- 全フレーム共通の表示領域のサイズをcanvasと呼ぶ

```rust
// Image
pub struct GifFrame {
    pub pixels: Vec<u8>,
    pub width: u16,
    pub height: u16,
    pub left: u16,
    pub top: u16,
    pub delay: u16,
    pub dispose: gif::DisposalMethod, // アニメーション時に差分更新するための仕組み
}

// GIF
pub struct GifFile {
    frames: Vec<GifFrame>,
    pub canvas_width: u16,
    pub canvas_height: u16,
}
```

### Disposal Method

- Disposalの種類に応じてGIF Exportする際のFrame処理を分岐する必要がある
- 具体的には次のFrameを描く前の前処理

| Disposal   | Frame表示後の処理          | Rustでの実装                                         |
| ---------- | -------------------------- | ---------------------------------------------------- |
| Keep       | 特になし                   | 現状は特になし                                       |
| Background | フレームをクリアして透明化 | RGBAを全て0 (透明) でリセット                        |
| Previous   | 前のフレームの状態に戻す   | 前のFrameをcloneしておき、それで次のフレームを初期化 |

- Backgroundで毎フレームリセットして表示するGIFが多い
- Previousはエフェクトのような特殊なフレームの場合に利用

### 最適化 (そのうち実装すべきTODO)

Keepについて以下の処理を実装することでパフォーマンスを改善 (ファイルサイズを小さく) することが可能

フレームN と フレームN+1 を比較
       ↓
変化したピクセルの最小矩形を計算 (Frameごとにサイズを最小化)
       ↓
その矩形だけのピクセルデータを切り出す
       ↓
frame.left, top, width, height に設定して書き出す
       ↓
disposal は Keep に設定 (NとN+1フレームで比較可能にするため、前フレームを残せるkeepにする)

disposal は「前フレームをどう処理するか」の指示であり、差分フレームは Keep と組み合わせて初めて意味を持つ

## Image

以下のようなデータ構造

```rust
pub struct ImageBuffer {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
```

- rustの場合は1次元配列で、RGB画像の場合は、3bytesごとに以下のようなデータとなっている
- 画像サイズに応じて各RGBのサイズが均等に大きくなる
- 基本的には255までのRGB8を利用
- RGBA8はアルファチャネル(透明度)が加わった4bytes

[255, 0, 0,   0, 255, 0,   0, 0, 255,   255, 255, 0]  
 ↑(0,0)赤     ↑(1,0)緑    ↑(0,1)青     ↑(1,1)黄

2x2の場合は以下のような並び順となる  
1行目: (0,0)赤  (1,0)緑  
2行目: (0,1)青  (1,1)黄

u8  → 0〜255        （256段階）  
u16 → 0〜65535      （65536段階）

GIFではcanvasでleft, top分ズラして画像がされているため、コード内ではleft, top分ズラすため、直接vecをループせずrow, colベースでループさせている

### Pixel types

| image crate 対応 | 形式    | チャンネル               | 用途                     |
| ---------------- | ------- | ------------------------ | ------------------------ |
| ○                | Luma    | 輝度のみ                 | グレースケール画像       |
| ○                | LumaA   | 輝度 + アルファ          | グレースケール画像       |
| ○                | RGB     | 赤・緑・青               | 一般的な画像             |
| ○                | RGBA    | 赤・緑・青・アルファ     | 透明度あり画像           |
| △                | CMYK    | シアン・マゼンタ・黄・黒 | 印刷用途                 |
| ×                | HSV/HSL | 色相・彩度・明度         | 色選択 UI など           |
| ○                | Rgb32F  | 浮動小数点 RGB           | 高ダイナミックレンジ画像 |

### Image Types

| 本プロジェクトでの利用 | 型           | 用途                                                                                                                            |
| ---------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| ○                      | ImageBuffer  | Pixel typesを固定した画像。RGBA8 バイト列から直接生成でき、PNG/JPEG 等の出力処理で使用                                          |
| ×                      | DynamicImage | 実行時にPixel typesが決まる画像。ファイルを開く際に使うが、このプロジェクトは gif クレートでGIFを読み込むと決まっているため不要 |

### Image Operations

画像を操作する関数群は、レベル別に以下のように分類される

| レベル              | トレイト/モジュール | 内容                                  |
| ------------------- | ------------------- | ------------------------------------- |
| 低レベル (読み取り) | GenericImageView    | ピクセル単位の読み取り                |
| 低レベル (読み書き) | GenericImage        | ピクセル単位の読み書き                |
| 高レベル            | imageops            | GenericImage を土台にした画像編集機能 |

#### GenericImageView

| メソッド名     | 用途                     |
| -------------- | ------------------------ |
| width / height | 幅・高さ取得             |
| get_pixel      | 指定座標のピクセルを取得 |
| view           | 部分ビューを取得         |

#### GenericImage

| メソッド名  | 用途                         |
| ----------- | ---------------------------- |
| put_pixel   | 指定座標にピクセルを書き込む |
| blend_pixel | アルファブレンドして書き込む |
| copy_from   | 別の画像を指定位置にコピー   |

#### imageops

| 関数名                     | 用途                 |
| -------------------------- | -------------------- |
| resize                     | サイズ変更           |
| crop                       | 部分切り出し         |
| rotate90 / 180 / 270       | 回転                 |
| flip_horizontal / vertical | 反転                 |
| grayscale                  | グレースケール化     |
| brighten                   | 明るさ調整           |
| contrast                   | コントラスト調整     |
| huerotate                  | 色相回転             |
| invert                     | 色反転               |
| blur                       | ぼかし               |
| filter3x3                  | 3x3 カーネルフィルタ |

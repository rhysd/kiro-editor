メモ
====

## フロントエンド部分を追加可能なアーキテクチャについて

- Rust のモジュールをコアとフロントエンドに分ける
  - コアはキー入力を行い，バッファを更新し，スクリーンを描画する
  - フロントエンドはコアを import し，キー入力を生成してコアに渡し，コアからの描画情報をハンドルする
  - 入力は `Iterator<Item=Result<InputSeq>>` で良さそう
  - 出力は要検討だが，描画情報の enum を定義すると良さそう
- コアとフロントエンドの間は別プロセス（IPC）にしない
  - 受け渡しが非同期になる
  - パフォーマンスが問題になりがち
  - 実装が複雑になる（エラーハンドリングなど）

## プロンプトの実装のアイデア

プロンプトはスクリーン描画とハイライト更新とカーソル位置（`TextBuffer` の状態）が絡むのでうまく切り出すのが難しい．
現状では `Editor` から切り離せていない．また，キー入力時のコールバックを `Fn` で表現しているのも切り離すのを難しくしている．

そこで `Prompt` struct をつくり，その中に一時的にエディタの状態を借用する．コールバックは trait にする．

```rust
pub struct Prompt<'a> {
    screen: &'a mut Screen,
    buf: &'a mut TextBuffer,
    hl: &'a mut Highlighting,
}

pub trait PromptAction {
    fn on_seq(prompt: &mut Prompt, query: &str, seq: InputSeq) -> Result<()> {
        Ok(())
    }

    fn on_end(prompt: &mut Prompt, ret: PromptAction) -> Result<PromptAction> {
        Ok(ret)
    }
}

// 通常のキー入力に使える，何もせず入力を返すだけ
pub struct NoAction;
impl PromptAction for NoAction {}

// インクリメンタル検索
pub struct TextSearch {
    last_match: Option<...>,
    // ...
}

impl PromptAction for TextSearch {
    fn on_seq(prompt: &mut Prompt, query: &str, seq: InputSeq) -> Result<()> {
        // ...
    }

    fn on_end(prompt: &mut Prompt, ret: PromptAction) -> Result<PromptAction> {
        // ...
    }
}

pub enum PromptResult {
    Canceled,
    Input(String),
}

impl<'a> Prompt<'a> {
    pub fn new<'s: 'a, 'b: 'a, 'h: 'a>(
        s: &'s mut Screen,
        b: &'b mut TextBuffer,
        h: &'h mut Highlighting,
    ) -> Self {
        // ...
    }

    pub fn run<A: PromptAction>(&mut self, action: A) -> Result<PromptResult> {
        // ...
    }
}
```

次のように使う．

```rust
let search = TextSearch::new();
match Prompt::new(s, b, h).run(search)? {
    PromptResult::Canceled => { /* ... */ }
    PromptResult::Input(input) => { /* ... */ }
}
```

## 描画情報のハンドリングについて

現状では描画のためのフラグや情報とエディタの状態が全て struct のフィールドに格納されて混ざっている．

struct のフィールドにすると特定のフィールドだけ `mut` といった指定ができないので，エディタの情報は参照したいだけで描画情報は更新したいといった指定ができない．
さらに `mut` は頻繁に borrow における問題を起こす（mutable borrow multiple times は禁止されている）ので避けたほうが無難．

そのため，描画ごとに必要な描画情報を `RenderContext` として分離する

```rust
struct RenderContext {
    hl_update: bool,            // ハイライトをアップデートするかどうかのフラグ
    dirty_start: Option<usize>, // dirty になった最初の行（None では描画されない）
    cursor_moved: bool,
    status_bar_updated: bool,   // ステータスバーの中の要素（ファイル名など）が変更されたかどうか
    message_updated: bool,      // メッセージの値が変更されたかどうか
}
```

`RenderContext` を tick ごとに毎回生成し，更新しながらエディタの状態を更新していく．最終的にスクリーンはこのコンテキストと画面の状態を見て画面を描画する．

```rust
for seq in input {
    let mut ctx = RenderContext::new();
    self.handle_keypress(seq?, &mut ctx)?;
    self.refresh_screen(ctx)?;
}
```

エディタの状態を表す struct と，`RenderContext` を受け取ってエディタの状態および `RenderContext` を更新する struct を分ける．
これにより普段 `Editor` が持っているエディタの状態と，毎 tick ごとに画面更新するための描画情報を明確に分けることができる．

```rust
pub struct TextBuffer {
    cx: usize,
    cy: usize,
    // ...
}

pub struct TextBufferUpdate<'a> {
    ctx: &'a mut RenderContext,
    buf: &'a mut TextBuffer,
}

impl<'a> TextBufferUpdate<'a> {
    pub fn move_cursor(&self) {
        // ...
    }
}
```

## エディタの更新をパイプラインにするアイデア

エディタの更新は大きく分けて

1. キー入力を受け取る
2. エディタの状態（主に `TextBuffer`）を更新
3. ハイライトを更新
4. 画面に描画

の各ステージに分けられるので，各段階の入出力を `Iterator` として表現してつなぐ．間を `RenderContext` が流れる．
例えばハイライトを優先して上書きしたい場合（検索のマッチなど）は 3. と 4. の間に新しいステージを挿入すれば良いということになる．

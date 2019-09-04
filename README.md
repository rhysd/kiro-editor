Kiro
====
[![crates.io][crates-io-badge]][crates-io]
[![Build Status][build-badge]][travis-ci]

[Kiro][] is a tiny UTF-8 text editor on terminal written in Rust. Kiro was started as a Rust port of
awesome minimal text editor [kilo][] and has grown with various extensions & improvements.

<img width=539 height=396 src="https://github.com/rhysd/ss/blob/master/kiro-editor/main.gif?raw=true" alt="main screenshot"/>

It provides basic features as a minimal text editor:

- Open/Save text files
- Create new text files and empty text buffer on memory
- Edit a text (put/delete characters, insert/delete lines, ...)
- Simple syntax highlighting
- Simple incremental text search

And Kiro extends [kilo][] to improve editing (please see 'Extended Features' section and 'Implementation'
section below for more details):

- Support editing UTF-8 characters like '🐶' (kilo only supports ASCII characters)
- More userful shortcuts (Alt modifier is supported)
- 24bit colors (true colors) and 256 colors support using [gruvbox][] retro color palette with 16
  colors fallback
- More efficient screen rendering and highlighting (kilo renders entire screen each time)
- Open multiple files (switch buffers by Ctrl-X/Alt-X)
- Resizing terminal window supported. Screen size is responsible
- Highlight more languages (Rust, Go, JavaScript, C++) and items (statements, types, number literals, ...)
- Automatically closes the message bar at bottom of line
- Modular implementation for each logics such as parsing key inputs, rendering screen, calculating
  highlight, modifying text buffer (kilo implements everything in one `kilo.c` with several global
  variables)

[Kiro][] aims to support kinds of xterm terminals on Unix-like systems. For example Terminal.app,
iTerm2.app, Gnome-Terminal, (hopefully) Windows Terminal on WSL.

I learned various things by making this project following ['Build Your Own Text Editor' guide][byote].
Please read 'Implementation' section below to find some interesting topics.



## Installation

Please install [`kiro-editor`][crates-io] package by building from sources using [cargo][].

```
$ cargo install kiro-editor
```



## Usage

### CLI

Installing [`kiro-editor`][crates-io] package introduces `kiro` command in your system.

```sh
$ kiro                 # Start with an empty text buffer
$ kiro file1 file2...  # Open files to edit
```

Please see `kiro --help` for command usage.


### Edit Text

Kiro is a mode-less text editor. Like other famous mode-less text editors such as Nano, Emacs,
Gedit or NotePad.exe, you can edit text in terminal window using a keyboard.

And several keys with Ctrl or Alt modifiers are mapped to various features. You don't need to
remember all mappings. Please type `Ctrl-?` to know all mappings in editor.

- **Operations**

| Mapping  | Description                                                                         |
|----------|-------------------------------------------------------------------------------------|
| `Ctrl-?` | Show all key mappings in editor screen.                                             |
| `Ctrl-Q` | Quit Kiro. If current text is not saved yet, you need to input `Ctrl-Q` twice.      |
| `Ctrl-S` | Save current buffer to file. Prompt shows up to enter file name for unnamed buffer. |
| `Ctrl-G` | Incremental text search.                                                            |
| `Ctrl-O` | Open file or empty buffer.                                                          |
| `Ctrl-X` | Switch to next buffer.                                                              |
| `Alt-X`  | Switch to previous buffer.                                                          |
| `Ctrl-L` | Refresh screen.                                                                     |

- **Moving cursor**

| Mapping                             | Description                        |
|-------------------------------------|------------------------------------|
| `Ctrl-P` or `↑`                    | Move cursor up.                    |
| `Ctrl-N` or `↓`                    | Move cursor down.                  |
| `Ctrl-F` or `→`                    | Move cursor right.                 |
| `Ctrl-B` or `←`                    | Move cursor left.                  |
| `Ctrl-A` or `Alt-←` or `HOME`      | Move cursor to head of line.       |
| `Ctrl-E` or `Alt-→` or `END`       | Move cursor to end of line.        |
| `Ctrl-[` or `Ctrl-V` or `PAGE DOWN` | Next page.                         |
| `Ctrl-]` or `Alt-V` or `PAGE UP`    | Previous page.                     |
| `Alt-F` or `Ctrl-→`                | Move cursor to next word.          |
| `Alt-B` or `Ctrl-←`                | Move cursor to previous word.      |
| `Alt-N` or `Ctrl-↓`                | Move cursor to next paragraph.     |
| `Alt-P` or `Ctrl-↑`                | Move cursor to previous paragraph. |
| `Alt-<`                             | Move cursor to top of file.        |
| `Alt->`                             | Move cursor to bottom of file.     |

- **Edit text**

| Mapping                 | Description               |
|-------------------------|---------------------------|
| `Ctrl-H` or `BACKSPACE` | Delete character          |
| `Ctrl-D` or `DELETE`    | Delete next character     |
| `Ctrl-W`                | Delete a word             |
| `Ctrl-J`                | Delete until head of line |
| `Ctrl-K`                | Delete until end of line  |
| `Ctrl-M`                | Insert new line           |

Here is some screenshots for basic features.

- **Create a new file**

<img width=365 height=220 src="https://github.com/rhysd/ss/blob/master/kiro-editor/new_file.gif?raw=true" alt="screenshot for creating a new file" />

- **Incremental text search**

<img width=380 height=220 src="https://github.com/rhysd/ss/blob/master/kiro-editor/search.gif?raw=true" alt="screenshot for incremental text search" />


### Extended Features

#### Support Editing UTF-8 Text

Kiro is a UTF-8 text editor. So you can open/create/insert/delete/search UTF-8 text including double width
characters support.

![UTF-8 supports](https://github.com/rhysd/ss/blob/master/kiro-editor/multibyte_chars.gif?raw=true)

Note that emojis using `U+200D` (zero width joiner) like '👪' are not supported yet.

#### 24-bit colors (true colors) and 256 colors support

Kiro utilizes colors as much as possible looking your terminal supports. It outputs 24-bit colors
with [gruvbox][] color scheme falling back to 256 colors or eventually to 16 colors.

- **24-bit colors**

<img src="https://github.com/rhysd/ss/blob/master/kiro-editor/colors_true.png?raw=true" alt="24-bit colors screenshot" width=562 height=343 />

- **256 colors**

<img src="https://github.com/rhysd/ss/blob/master/kiro-editor/colors_256.png?raw=true" alt="256 colors screenshot" width=562 height=339 />

- **16 colors**

<img src="https://github.com/rhysd/ss/blob/master/kiro-editor/colors_16.png?raw=true" alt="16 colors screenshot" width=554 height=339 />

#### Handle window resize

Terminal notifies a window resize event via SIGWINCH signal. Kiro catches the signal and properly redraws
its screen with new window size.

![resize window](https://github.com/rhysd/ss/blob/master/kiro-editor/resize.gif?raw=true)



## Implementation

This project was a study to understand how a text editor can be implemented interacting with a
terminal application. I learned many things related to interactions between terminal and application
and several specs of terminal escape sequences such as VT100 or xterm.

I started from porting an awesome minimal text editor [kilo][] following a guide
['Built Your Own Text Editor'][byote]. And then I added several improvements to my implementation.

Here I write topics which were particularly interesting for me.


### Efficient Rendering and Highlighting

[kilo][] updates rendering and highlighting each time you input a key. This implementation is great
to make implementation simple and it works fine.

However, it is insufficient and I felt some performance issue on editing larger (10000~ lines) C file.

So [Kiro][] improves the implementation to render the screen and to update highlighting only when
necessary.

[Kiro][] has a variable `dirty_start` in `Screen` struct of [screen.rs](./src/screen.rs). It manages
from which line rendering should be started.

For example, let's say we have C code bellow:

```c
int main() {
    printf("hello\n");
}
```

And put `!` like `printf("hello!\n");`.

In the case, first line does not change. So we don't need to update the line. However, Kiro renders
the `}` line also even if the line does not change. This is because modifying text may cause highlight
of lines after the line. For example, when deleting `"` after `\n`, string literal is not terminated so
next line continues string literal highlighting.

Highlighting has the similar characteristic. Though [kilo][] calculates highlighting of entire text buffer
each time you input key, actually the lines after bottom of screen are not rendered.
For current syntax highlighting, changes to former lines may affect later lines highlighting
(e.g. block comments `/* */`), changes to later lines don't affect former lines highlighting. So Kiro
stops calculating highlights at the line of bottom of screen.


### UTF-8 Support

[kilo][] only supports ASCII text. Width of ASCII character is fixed to 1 byte. This assumption reduces
complexity of implementation of kilo greatly because:

- every character can be represented as `char` (almost the same as `u8` in Rust)
- any character in ASCII text can be accessed via byte index in O(1)
- length of text is the same as number of bytes of the text

So kilo can contain text buffer as simple `char *` and accesses characters in it via byte index.
In addition, display width of all printable ASCII characters is fixed except for `0x09` tab character.

But actually there are more characters in the world defined as Unicode characters. Since I'm Japanese,
the characters such as Kanji or Hiragana I'm daily using are not ASCII. And the most major text encoding
is UTF-8. So I determined to extend Kiro editor to support UTF-8.

In UTF-8, byte length of character is variable. Any character takes 1~4 bytes (or more in special case).
The important point here is that accessing to character in UTF-8 text is not O(1). To access to N-th
character or to know length of text, it requires to check characters from head of the text.

Accessing to character in text and getting text length happen frequently while updating text buffer
and highlights. So checking them in O(N) for each time is not efficient. To solve this problem, Kiro
contains byte indices of each characters in line text as `Vec<usize>`. These indices are only existing
when at least one character in line text is non-ASCII character.

![UTF-8 support diagram](./assets/utf-8-support-diagram.png)

In `Row` struct which represents one text line, `indices` field (`Vec<usize>`) is dedicated to store
byte indices of each character.

In the first case `"Rust is nice"`, all characters are ASCII so byte index can be used to access to
characters in the text. In the case, `indices` field is an empty (and capacity is set to zero). A `Vec`
instance with zero capacity is guaranteed not to allocate heap memory. So the memory overhead here is
24 bytes of `Vec<usize>` instance itself (pointer, capacity as `usize` and length as `usize`) only.

In the second case `"Rust🦀良い"`, there are some non-ASCII characters so `self.indices` caches byte
indices of each characters. Thanks to this cache, each character can be accessed in O(1) and its text
length can be obtained in O(1) as `self.indices.len()`. `Row` also contains a rendered text and updates
it when internal text buffer is updated by `TextBuffer`. So `self.indices` cache is also updated at
the same timing efficiently.

Though keeping byte indices in `Vec<usize>` is quite memory inefficient, the indices are only required
when the line text contains non-ASCII characters. In terms of programming code editor, it is relatively
rare case, I believe.


### Porting C editor to Rust

#### Separate one C source into several Rust modules

To simplify and minimize implementation, [kilo][] uses some global variables and local `static`
variables. Editor's state is stored in a global variable `E` and it is referred everywhere.

While porting the code to Rust, I split `kilo.c` into some Rust modules for each logics. I removed
the global variables and local static variables by moving them to each logic's structs.

- [`editor.rs`](src/editor.rs): Exports `Editor` struct, which manages an editor lifecycle; Runs loop
  which gets key input, updates a text buffer and highlight then renders screen.
- [`text_buffer.rs`](src/text_buffer.rs): Exports `TextBuffer` struct, which manages an editing text
  buffer as `Vec<Row>`. It also contains metadata such as file name and file type of the buffer.
- [`row.rs`](src/row.rs): Exports `Row` struct which represents one line of text buffer and contains
  actual text and rendered text. Since Kiro is dedicated for UTF-8 text editing, internal text buffer
  is also kept as UTF-8 string. When the internal text buffer is updated by `Editor`, it automatically
  updates rendered text also. It may also contain character indices for UTF-8 non-ASCII characters
  (Please see below 'UTF-8 Support' section).
- [`input.rs`](src/input.rs): Exports `StdinRawMode` struct and `InputSequences` iterator.
  `StdinRawMode` setups STDIN as raw mode (disable various terminal features such as echo back).
  `InputSequences` reads user's key input as byte sequence with timeout and parses it as stream of
  key sequence. VT100 and xterm escape sequences like `\x1b[D` for `←` key are parsed here.
- [`highlight.rs`](src/highlight.rs): Exports `Highlighting` struct, which contains highlight information
  of each character in text buffer. It also manages highlighting in an editor lifecycle. It calculates
  highlights of characters which is rendered and updates its information.
- [`screen.rs`](src/screen.rs): Exports `Screen` struct, which represents screen rendering. It renders
  each `Row` with highlight colors by outputting characters and escape sequences to STDOUT. As described
  in previous section, it manages efficient rendering. It also manages and renders status bar and message
  bar located at bottom of screen.
- [`status_bar.rs`](src/status_bar.rs): Exports `StatusBar` struct which manages fields displayed in the
  status bar. It has flag `redraw` to determine if it should be re-rendered.
- [`prompt.rs`](src/prompt.rs): Exports structs related to user prompt using message bar. This module
  has logic to run user prompt and text search. Callbacks while prompt is represented as a `PromptAction`
  trait.
- [`term_color.rs`](src/term_color.rs): Exports small `TermColor` enum and `Color` enum, which represents
  terminal colors. This module also has logic to detect 24-bit colors and 256 colors support of terminal.
- [`language.rs`](src/language.rs): Exports small `Language` enum, which represents file types like
  C, Rust, Go, JavaScript, C++. It contains logic to detect a file type from file name.
- [`signal.rs`](src/signal.rs): Exports `SigwinchWatcher` struct, which receives SIGWINCH signal and
  notifies it to `Screen`. The signal is sent when terminal window size changed. `Screen` requires
  the notification for resizing the screen.
- [`error.rs`](src/error.rs): Exports `Error` enum and `Result<T>` type to handle all kinds of error
  which may occur in Kiro editor.

#### Error handling and resource clean up

[kilo][] outputs message by `perror()` and immediately exits on error. It also cleans up STDIN
configuration with `atexit` hook.

Kiro is implemented in Rust. So it utilizes Rust idioms to handle errors with `io::Result` and `?`
operator. It reduces codes for error handling so that I could focus on implementing editor logics.

For resource clean up, Rust's `Drop` crate works greatly in `input.rs`.

```rust
struct StdinRawMode {
    stdin: io::Stdin,
    // ...
}

impl StdinRawMode {
    fn new() -> io::Result<StdinRawMode> {
        // Setup terminal raw mode of stdin here
        // ...
    }
}

impl Drop for StdinRawMode {
    fn drop(&mut self) {
        // Restore original terminal mode of stdin here
    }
}

impl Deref for StdinRawMode {
    type Target = io::Stdin;
    fn deref(&self) -> &Self::Target {
        &self.stdin
    }
}

impl DerefMut for StdinRawMode {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stdin
    }
}
```

The `drop()` method is called when `StdinRawMode` instance dies. So user doesn't need to remember
the clean up. And `StdinRawMode` also implements `Deref` and `DerefMut` so that it behaves almost
as if it were `Stdin`. By wrapping `io::Stdin` like this, I could add the ability to enter/leave
terminal raw mode to `io::Stdin`.

#### Abstract input and output of editor

```rust
pub struct Editor<I, W>
where
    I: Iterator<Item = io::Result<InputSeq>>,
    W: Write,
{
    // ...
}

impl<I, W> Editor<I, W>
where
    I: Iterator<Item = io::Result<InputSeq>>,
    W: Write,
{
    // Initialize Editor struct with given input and output
    pub fn new(input: I, output: W) -> io::Result<Editor<I, W>> {
        // ...
    }
}
```

The input of terminal text editor is a stream of input sequences from terminal which include
user's key input and control sequences. The input is represented with `Iterator` trait of input sequence.
Here `InputSeq` represents one key input or one control sequence.

The output of terminal text editor is also stream of sequences to terminal which include output
strings and control sequences. It's done by simply writing to stdout. So it is represented with
`Write` trait.

The benefit of these abstractions are testability of each modules. By creating a dummy struct which
implements `Iterator<Item = io::Result<InputSeq>>`, the input can be easily replaced with dummy input.
Since [kilo][] does not have tests, these abstractions are not necessary for it.

```rust
struct DummyInput(Vec<InputSeq>);

impl Iterator for DummyInput {
    type Item = io::Result<InputSeq>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            None
        } else {
            Some(Ok(self.0.remove(0)))
        }
    }
}

// Dummy Ctrl-Q input to editor
let dummy_input = DummyInput(vec![ InputSeq::ctrl(b'q') ]);
```

And by implementing a small struct which simply discards output, we can ignore the output. It does
not need to draw screen in terminal window. And it does not rely on global state (terminal raw mode)
so that tests can run paralelly. As the result tests can run faster and terminal window doesn't mess up.

```rust
struct Discard;

impl Write for Discard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
```

By using these mocks the input and output of editor can be tested easily as follows:

```rust
#[test]
fn test_editor() {
    let mut editor = Editor::new(dummy_input, Discard).unwrap();
    editor.edit().unwrap();
    for line in editor.lines() {
        // Check lines of the current text buffer
    }
}
```

#### Dependant Crates

This project depends on some small crates. I selected them carefully not to prevent learning how a
text editor on terminal works.

- [termios][]: Safe binding to `termios` interface provided by OS.
- [term_size][]: Safe binding to getting terminal window size with ioctl(2).
- [unicode-width][]: Small library to calculate Unicode character's display width.
- [term][]: Library for terminfo and terminal colors. This project uses this library only to parse
  terminfo for 256 colors support.
- [signal-hook][]: Small wrapper for signal handler to catch SIGWINCH for resize support.
- [getopts][]: Fairly small library to parse command line arguments. Kiro only has quite simple CLI
  options so [clap][] is too heavy.


### TODO

- Unit tests are not sufficient. More tests should be added
- Improve scrolling performance (Is terminal scrolling available?)
- Minimal documentation
- Undo/Redo is not implemented yet
- Text selection and copy from or paste to system clipboard
- Keeping all highlights (`Vec<Highlight>`) is not memory efficient. Keep bits only for current
  screen (`rowoff..rowoff+num_rows`)
- Use parser library [combine](https://github.com/Marwes/combine) or [nom](https://github.com/Geal/nom)
  to calculate highlighting. Need some investigation since highlight parser must stop calculating when
  current line exceeds the bottom line of screen. Also [syntect](https://github.com/trishume/syntect) is
  interesting.


### Future Works

- Use more efficient data structure such as rope, gap buffer or piece table
- Use incremental parsing for accurate syntax highlighting
- Support more systems and terminals
- Look editor configuration file such as [EditorConfig](https://editorconfig.org/)
  or [`.vscode` VS Code workspace settings](https://code.visualstudio.com/docs/getstarted/settings)
- Support emojis using `U+200D`
- WebAssembly support
- Mouse support
- Completion, go to definition and look up using language servers



## License

This project is distributed under [the MIT License](./LICENSE.txt).


[Kiro]: https://github.com/rhysd/kiro-editor
[kilo]: https://github.com/antirez/kilo
[byote]: https://viewsourcecode.org/snaptoken/kilo/
[gruvbox]: https://github.com/morhetz/gruvbox
[cargo]: https://github.com/rust-lang/cargo
[build-badge]: https://travis-ci.org/rhysd/kiro-editor.svg?branch=master
[travis-ci]: https://travis-ci.org/rhysd/kiro-editor
[crates-io]: https://crates.io/crates/kiro-editor
[crates-io-badge]: https://img.shields.io/crates/v/kiro-editor.svg
[termios]: https://crates.io/crates/termios
[term_size]: https://crates.io/crates/term_size
[unicode-width]: https://crates.io/crates/unicode-width
[term]: https://crates.io/crates/unicode-width
[signal-hook]: https://crates.io/crates/signal-hook
[getopts]: https://crates.io/crates/getopts
[clap]: https://crates.io/crates/clap

Kiro
====
[![Build Status][build-badge]][travis-ci]

[Kiro][] is a tiny UTF-8 text editor on terminal written in Rust. [Kiro][] was implemented
based on awesome minimal text editor [kilo][] and ['Build Your Own Text Editor' tutorial][byote]
with various improvements.

<img width=589 height=412 src="https://github.com/rhysd/ss/blob/master/kiro-editor/main.gif?raw=true" alt="main screenshot"/>

It provides basic features as a minimal text editor:

- Open/Save a text file
- Create a new text file
- Edit a text (put/delete characters, insert/delete lines, ...)
- Simple syntax highlighting
- Simple incremental text search

And [Kiro][] extends [kilo][] with various improvements (please see 'Extended Features' section
and 'Implementation' section below for more details):

- Support editing UTF-8 characters like '🐶' (kilo only supports ASCII characters)
- More Emacs-like shortcuts
- 24bit colors (true colors) and 256 colors support using [gruvbox][] retro color palette with 16
  colors fallback
- More efficient screen rendering and highlighting (kilo renders entire screen each time)
- Resizing terminal window supported. Screen size is responsible
- Highlight more languages (Rust, Go, JavaScript, C++)
- Modular implementation for each logics such as parsing key inputs, rendering screen, calculating
  highlight, modifying text buffer (kilo implements everything in one `main.c` with several global
  variables)

[Kiro][] aims to support kinds of xterm terminals on Unix-like systems. For example Terminal.app,
iTerm2.app, Gnome-Terminal, (hopefully) Windows Terminal on WSL.


## Installation

Please install `kiro` command by building from sources using [cargo][].

```
$ cargo install kiro-editor
```



## Usage

### CLI

```sh
$ kiro                 # Start with an empty text buffer
$ kiro path/to/file    # Open a file to edit
```

Please see `kiro --help` for command usage.


### Edit Text

Kiro is a mode-less text editor. Like other famous mode-less text editors such as Nano, Emacs,
Gedit or NotePad.exe, you can edit text in terminal window using a keyboard.

And several keys with Ctrl or Alt modifiers are mapped to various features. You don't need to
remember all mappings. Please type `Ctrl-?` to know all mappings in editor.

- **Operations**

| Mapping  | Description                                                                    |
|----------|--------------------------------------------------------------------------------|
| `Ctrl-?` | Show all key mappings in editor screen.                                        |
| `Ctrl-Q` | Quit Kiro. If current text is not saved yet, you need to input `Ctrl-Q` twice. |
| `Ctrl-G` | Incremental text search.                                                       |
| `Ctrl-L` | Refresh screen.                                                                |

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
| `Ctrl-U`                | Delete until head of line |
| `Ctrl-K`                | Delete until end of line  |
| `Ctrl-M`                | Insert new line           |

Here is some screenshots for basic features.

- **Create a new file**

<img width=409 height=220 src="https://github.com/rhysd/ss/blob/master/kiro-editor/new_file.gif?raw=true" alt="screenshot for creating a new file" />

- **Incremental text search**

<img width=409 height=220 src="https://github.com/rhysd/ss/blob/master/kiro-editor/search.gif?raw=true" alt="screenshot for incremental text search" />



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


### Porting C editor to Rust

TBD


### UTF-8 Support

TBD


### TODO

- Unit tests are not sufficient. More tests are necessary
- Undo/Redo is not implemented yet
- Text selection and copy from or paste to system clipboard


### Future Works

- Use more efficient data structure such as rope, gap buffer or piece table
- Use incremental parsing for accurate syntax highlighting
- Support more systems and terminals
- Look editor configuration file such as [EditorConfig](https://editorconfig.org/)
  or [`.vscode` VS Code workspace settings](https://code.visualstudio.com/docs/getstarted/settings)
- Support emojis using `U+200D`



## License

This project is distributed under [the MIT License](./LICENSE.txt).


[Kiro]: https://github.com/rhysd/kiro-editor
[kilo]: https://github.com/antirez/kilo
[byote]: https://viewsourcecode.org/snaptoken/kilo/
[gruvbox]: https://github.com/morhetz/gruvbox
[cargo]: https://github.com/rust-lang/cargo
[build-badge]: https://travis-ci.org/rhysd/kiro-editor.svg?branch=master
[travis-ci]: https://travis-ci.org/rhysd/kiro-editor

<a name="v0.4.1"></a>
# [v0.4.1](https://github.com/rhysd/kiro-editor/releases/tag/v0.4.1) - 19 Jan 2021

- **New:** Add [Julia](https://julialang.org/) language support. Thanks @matbesancon for the contribution!
- **Doc:** Add link to NetBSD package in README

[Changes][v0.4.1]


<a name="v0.4.0"></a>
# [v0.4.0](https://github.com/rhysd/kiro-editor/releases/tag/v0.4.0) - 24 Jul 2020

- **New:** Add Python syntax highlighting
- **New:** Back to unmodified state when undo-ing to the original text (thanks [@tyfkda](https://github.com/tyfkda) for this contribution)
- **Fix:** Error message highlighting was broken in message bar
- **Fix:** Ignore C1 control chars in input
- **Fix:** Crash when searched text is placed at end of line which includes multi-byte characters
- **Improve:** Update dependencies
- **Dev:** Add fuzzing test for editing/highlighting text
- **Dev:** Add benchmarks

[Changes][v0.4.0]


<a name="0.3.5"></a>
# [0.3.5](https://github.com/rhysd/kiro-editor/releases/tag/0.3.5) - 19 Oct 2019

- **Improve:** Text search was much improved!
  - Search word which reveals twice or more in a line are all correctly matched. Previous
    implementation (and kilo) only matches to first searched word in one line
  - Current match is now at middle of screen, not at top
  - Other matches than current are also highlighted like famous text editors
  - Ctrl-J and Ctrl-W are supported in text search prompt
  - Cursor moves to prompt line so that it makes easier to show now inputting characters for text
    search
- **Fix:** Welcome message is not shown on empty buffer. Now welcome screen is rendered only at start
  up if the buffer is unnamed
- **Improve:** Colorscheme is optimized
  - Color of `~` after end of buffer is improved
  - Colors of current match and other matches while text search was updated. They are now easier to
    distinguish
- **Improve:** Reduce heap allocation on parsing UTF-8 char in input byte sequence
- **Fix:** Raise an error properly on invalid multi-byte sequence and too small window resizing
- **Fix:** Cursor position was not correct on inserting a tab character
- **Fix:** Editor crashes when trying to search text at the end of buffer
- **Improve:** Reduce reallocation on updating rendered text in row
- **Improve:** Add `first_paint()` method to `kiro_editor::Editor` which returns an iterator to run the
  editor
- **Improve:** [Benchmarks](https://github.com/rhysd/kiro-editor/tree/master/benches) were introduced to
  avoid performance regression
- **Improve:** [Use GitHub Actions](https://github.com/rhysd/kiro-editor/actions?workflow=CI) for CI

[Changes][0.3.5]


<a name="0.3.4"></a>
# [0.3.4](https://github.com/rhysd/kiro-editor/releases/tag/0.3.4) - 24 Sep 2019

- **Improve:** Removed redundant screen re-rendering while prompt input
- **Fix:** Prompt message was wrong on file name input at saving buffer with `Ctrl-S`

[Changes][0.3.4]


<a name="0.3.3"></a>
# [0.3.3](https://github.com/rhysd/kiro-editor/releases/tag/0.3.3) - 21 Sep 2019

- **New:** Undo/Redo was implemented. `Ctrl-U` for undo, `Ctrl-R` for redo
- **Fix:** Matched region was not updated on `plain` file type while text search
- **Fix:** Cursor position was not correct on appending string to current line
- **Fix:** `Ctrl-D` or `DELETE` moves cursor at the end of last line
- **Improve:** Remove redundant redraws on resetting/resizing screen
- **Improve:** Add more tests
  - All text manipulations are now tested
  - Undo/Redo are tested
  - Editing multi-byte characters is tested

[Changes][0.3.3]


<a name="0.3.2"></a>
# [0.3.2](https://github.com/rhysd/kiro-editor/releases/tag/0.3.2) - 10 Sep 2019

- **Fix:** Highlighting one-length identifiers was wrong
- **Fix:** Scrolling to next/previous match while text search does not show matched line in some situation
- **Fix:** `usize` keyword highlighting was wrong in Rust source code
- **Fix:** Background color does not end at end of line
- **Improve:** Use better invert yellow color for matched region on text search
- **Improve:** For special variables such as `this`, `self`, use different color from boolean constants like `true`, `false`
- **Improve:** Show matched line from head of the line as much as possible on moving to next/previous match
- **Improve:** Use [jemalloc](http://jemalloc.net/) for global memory allocator of `kiro` executable
- **Improve:** Internal refactoring of highlighting logic


[Changes][0.3.2]


<a name="0.3.1"></a>
# [0.3.1](https://github.com/rhysd/kiro-editor/releases/tag/0.3.1) - 05 Sep 2019

- **Improve:** Better highlighting. Following items are newly highlighted
  - Number literal delimiters such as `123_456`, `0xabc_def_ghi`, `0x_a_b_c_` in Rust and Go, `123'456'789` in C++
  - Highlight identifiers in variable, struct and type definitions (e.g. `Foo` in `struct Foo`, `x` in `let x = ...`)
  - Highlight special identifiers such as `true`, `false`, `self`, `this`, `nil`, `null`, `undefined` differently from keywords
- **Fix:** Screen was not redrawn on window resize
- **Fix:** Newline `\n` was missing in an empty text buffer
- **Fix:** Foreground color of 256colors colorscheme


[Changes][0.3.1]


<a name="0.3.0"></a>
# [0.3.0](https://github.com/rhysd/kiro-editor/releases/tag/0.3.0) - 02 Sep 2019

- **Improve:** **Breaking Change:** Now message bar line is automatically squashed when no message is shown (after 5seconds, message is cleared)
- **Improve:** **Breaking Change:** `Ctrl-L` clears message
- **Improve:** Rendering message bar made more efficient. It is re-rendered only when it's changed
- **Improve:** Rendering status bar made more efficient. It is re-rendered only when it's contents are updated
- **Fix:** Ensure to back to normal screen buffer even if an editor crashes
- **Fix:** Line number in status bar was not correct
- **Improve:** Many internal implementation refactoring

[Changes][0.3.0]


<a name="0.2.1"></a>
# [0.2.1](https://github.com/rhysd/kiro-editor/releases/tag/0.2.1) - 29 Aug 2019

- **Fix:** Rendering did not happen on inserting text or new line just after opening an editor
- **Fix:** Cursor position was not reset after quit

[Changes][0.2.1]


<a name="0.2.0"></a>
# [0.2.0](https://github.com/rhysd/kiro-editor/releases/tag/0.2.0) - 29 Aug 2019

- **Improve:** **Breaking Change** Shortcut to delete characters until head of line was changed from `Ctrl-U` to `Ctrl-J`
- **New:** Highlight hex and binary number literals
- **New:** Support editing multiple files. switch buffer by `Ctrl-X`/`Alt-X`
- **Fix:** Cursor sometimes flickered on screen redraw
- **Improve:** Text buffer representation was separated from `editor` module as `text_buffer` module
- **Improve:** More description was added to README
- **Fix:** Editor screen was remaining in terminal buffer after quit

[Changes][0.2.0]


<a name="0.1.1"></a>
# [0.1.1](https://github.com/rhysd/kiro-editor/releases/tag/0.1.1) - 27 Aug 2019

- **Improve:** Complete README file by describing missing sections

[Changes][0.1.1]


<a name="0.1.0"></a>
# [0.1.0](https://github.com/rhysd/kiro-editor/releases/tag/0.1.0) - 27 Aug 2019

First release :tada:

Please read [README file](https://github.com/rhysd/kiro-editor#readme) to know this product. Thanks!

[Changes][0.1.0]


[v0.4.1]: https://github.com/rhysd/kiro-editor/compare/v0.4.0...v0.4.1
[v0.4.0]: https://github.com/rhysd/kiro-editor/compare/0.3.5...v0.4.0
[0.3.5]: https://github.com/rhysd/kiro-editor/compare/0.3.4...0.3.5
[0.3.4]: https://github.com/rhysd/kiro-editor/compare/0.3.3...0.3.4
[0.3.3]: https://github.com/rhysd/kiro-editor/compare/0.3.2...0.3.3
[0.3.2]: https://github.com/rhysd/kiro-editor/compare/0.3.1...0.3.2
[0.3.1]: https://github.com/rhysd/kiro-editor/compare/0.3.0...0.3.1
[0.3.0]: https://github.com/rhysd/kiro-editor/compare/0.2.1...0.3.0
[0.2.1]: https://github.com/rhysd/kiro-editor/compare/0.2.0...0.2.1
[0.2.0]: https://github.com/rhysd/kiro-editor/compare/0.1.1...0.2.0
[0.1.1]: https://github.com/rhysd/kiro-editor/compare/0.1.0...0.1.1
[0.1.0]: https://github.com/rhysd/kiro-editor/tree/0.1.0

 <!-- Generated by changelog-from-release -->

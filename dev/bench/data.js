window.BENCHMARK_DATA = {
  "lastUpdate": 1573703885619,
  "repoUrl": "https://github.com/rhysd/kiro-editor",
  "entries": {
    "Rust Benchmark": [
      {
        "commit": {
          "author": {
            "email": "lin90162@yahoo.co.jp",
            "name": "rhysd",
            "username": "rhysd"
          },
          "committer": {
            "email": "lin90162@yahoo.co.jp",
            "name": "rhysd",
            "username": "rhysd"
          },
          "distinct": true,
          "id": "e5383d5bc6b639060b4d7d1961d6b3a8ca5fd930",
          "message": "start to use github-action-benchmark",
          "timestamp": "2019-11-14T12:53:00+09:00",
          "tree_id": "e4f2f4ec5e9cd90d2cac27dc5c67a086f10ed7a0",
          "url": "https://github.com/rhysd/kiro-editor/commit/e5383d5bc6b639060b4d7d1961d6b3a8ca5fd930"
        },
        "date": 1573703885600,
        "tool": "cargo",
        "benches": [
          {
            "name": "no_term_edit_1000_operations_to_10000_chars_plain_text",
            "value": 22277998,
            "range": "+/- 2,344,390",
            "unit": "ns/iter"
          },
          {
            "name": "no_term_edit_1000_operations_to_editor_rs",
            "value": 312138426,
            "range": "+/- 30,675,508",
            "unit": "ns/iter"
          },
          {
            "name": "no_term_scroll_up_down_plain_text",
            "value": 1776421,
            "range": "+/- 274,650",
            "unit": "ns/iter"
          },
          {
            "name": "no_term_scroll_up_down_rust_code",
            "value": 9243519,
            "range": "+/- 1,004,118",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}
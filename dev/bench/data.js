window.BENCHMARK_DATA = {
  "lastUpdate": 1573704981073,
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
      },
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
          "id": "1f47863b2e419b0dffd11d892bfa69faa8181cac",
          "message": "Merge branch 'benchmark-action'",
          "timestamp": "2019-11-14T13:10:59+09:00",
          "tree_id": "f385a65432b8a43fa1d75cf3af21347bfda0ff1d",
          "url": "https://github.com/rhysd/kiro-editor/commit/1f47863b2e419b0dffd11d892bfa69faa8181cac"
        },
        "date": 1573704981056,
        "tool": "cargo",
        "benches": [
          {
            "name": "no_term_edit_1000_operations_to_10000_chars_plain_text",
            "value": 24317785,
            "range": "+/- 2,245,432",
            "unit": "ns/iter"
          },
          {
            "name": "no_term_edit_1000_operations_to_editor_rs",
            "value": 328830808,
            "range": "+/- 11,465,488",
            "unit": "ns/iter"
          },
          {
            "name": "no_term_scroll_up_down_plain_text",
            "value": 2097285,
            "range": "+/- 202,049",
            "unit": "ns/iter"
          },
          {
            "name": "no_term_scroll_up_down_rust_code",
            "value": 10106914,
            "range": "+/- 765,840",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}
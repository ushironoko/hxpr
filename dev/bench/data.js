window.BENCHMARK_DATA = {
  "lastUpdate": 1769416882813,
  "repoUrl": "https://github.com/ushironoko/octorus",
  "entries": {
    "octorus Benchmark": [
      {
        "commit": {
          "author": {
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "135a7b8a1f8467cfcc7d5c0f700822498219f992",
          "message": "feat: add benchmark infrastructure and optimize diff rendering (#16)\n\n* feat: add benchmark infrastructure and optimize diff rendering\n\n- Add Criterion benchmarks for UI rendering and diff parsing\n- Optimize diff_view.rs: use Line::style() instead of per-span REVERSED\n- Optimize diff_view.rs: process only visible range (120x faster for 5000 lines)\n- Add GitHub Actions workflow for CI benchmark with regression detection\n- Create lib.rs to expose modules for benchmarks\n\nBenchmark results:\n- visible_range/all_lines/5000: ~294µs\n- visible_range/visible_only/5000: ~2.4µs\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>\n\n* fix: prevent panic when scroll_offset exceeds cached lines\n\nClamp visible_start to cache.lines.len() to avoid out-of-bounds\nslice access when scroll_offset is >= the number of cached lines.\nThis can occur after switching to files with shorter diffs or\nrapid scrolling.\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>\n\n* chore: change benchmark trigger to manual + weekly schedule\n\nAvoid CI rate limit by removing push/PR triggers.\n- workflow_dispatch: manual trigger from Actions tab\n- schedule: weekly (Sunday 00:00 UTC)\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>\n\n---------\n\nCo-authored-by: Claude Opus 4.5 <noreply@anthropic.com>",
          "timestamp": "2026-01-24T06:40:19Z",
          "url": "https://github.com/ushironoko/octorus/commit/135a7b8a1f8467cfcc7d5c0f700822498219f992"
        },
        "date": 1769238085704,
        "tool": "cargo",
        "benches": [
          {
            "name": "diff_parsing/classify_line/header",
            "value": 5,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/meta_diff",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/meta_plus",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/meta_minus",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/added",
            "value": 7,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/removed",
            "value": 7,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/context",
            "value": 7,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/context_long",
            "value": 7,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/100",
            "value": 582,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2918,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5860,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2190,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2488,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2824,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9706,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 10948,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12887,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 20751,
            "range": "± 341",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 23953,
            "range": "± 328",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 28509,
            "range": "± 518",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 20151,
            "range": "± 218",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 23173,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 2582271,
            "range": "± 62985",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 3402682,
            "range": "± 54744",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 4720795,
            "range": "± 25130",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 14976080,
            "range": "± 229750",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 14308,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 86400,
            "range": "± 1568",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 179968,
            "range": "± 985",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 883701,
            "range": "± 6935",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 36605,
            "range": "± 180",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/line_style/100",
            "value": 35914,
            "range": "± 466",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 73973,
            "range": "± 917",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/line_style/500",
            "value": 72509,
            "range": "± 1425",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 125832,
            "range": "± 1622",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/line_style/1000",
            "value": 123078,
            "range": "± 467",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 122600,
            "range": "± 443",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_only/1000",
            "value": 5335,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 538376,
            "range": "± 2803",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_only/5000",
            "value": 5136,
            "range": "± 11",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b92cd35c00c79daacfdc267ae44642062a8a634e",
          "message": "perf: eliminate String clone in diff render loop by borrowing cached Spans (#22)\n\nReplace `cached.spans.clone()` (deep-copies every Cow::Owned(String))\nwith borrowed Span construction using `Cow::Borrowed(&str)` pointing\nto cached data. This eliminates all heap allocations for visible lines\nduring the 60fps render loop.\n\nAdd `borrowed_spans` and `visible_borrowed` benchmark variants to\nmeasure the improvement against the existing clone-based approaches.\n\nCo-authored-by: Claude <noreply@anthropic.com>",
          "timestamp": "2026-01-26T08:30:35Z",
          "url": "https://github.com/ushironoko/octorus/commit/b92cd35c00c79daacfdc267ae44642062a8a634e"
        },
        "date": 1769416882511,
        "tool": "cargo",
        "benches": [
          {
            "name": "diff_parsing/classify_line/header",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/meta_diff",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/meta_plus",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/meta_minus",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/added",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/removed",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/context",
            "value": 7,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/context_long",
            "value": 7,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/100",
            "value": 580,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2915,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5837,
            "range": "± 203",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2178,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2401,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2694,
            "range": "± 74",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9917,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 11197,
            "range": "± 125",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12925,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 21291,
            "range": "± 304",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 24546,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 28477,
            "range": "± 261",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 20596,
            "range": "± 76",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 23990,
            "range": "± 225",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 2635690,
            "range": "± 17635",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 3397039,
            "range": "± 18497",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 4717317,
            "range": "± 215723",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 14826061,
            "range": "± 77619",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 14732,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 85640,
            "range": "± 1027",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 189176,
            "range": "± 1052",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 903058,
            "range": "± 3579",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 36108,
            "range": "± 138",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/line_style/100",
            "value": 35006,
            "range": "± 125",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5958,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 77410,
            "range": "± 338",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/line_style/500",
            "value": 73827,
            "range": "± 529",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 28339,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 134970,
            "range": "± 588",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/line_style/1000",
            "value": 127302,
            "range": "± 650",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 57215,
            "range": "± 503",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 126325,
            "range": "± 754",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_only/1000",
            "value": 6869,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3362,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 561068,
            "range": "± 9017",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_only/5000",
            "value": 6675,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 3339,
            "range": "± 7",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}
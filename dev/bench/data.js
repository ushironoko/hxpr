window.BENCHMARK_DATA = {
  "lastUpdate": 1772330544468,
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
          "id": "939202ea2e1b62abf1751287439d5068e2304e92",
          "message": "Extract render_cached_lines into reusable function (#23)\n\n* refactor: extract render_cached_lines as pub fn to eliminate benchmark-production code divergence\n\nThe rendering loop logic (converting CachedDiffLine to Line with\nzero-copy borrowing) was duplicated inline in both the production\ncode and benchmarks, causing them to diverge after b92cd35.\nNow both call the same render_cached_lines function.\n\n* refactor(bench): separate archive benchmarks into dedicated groups\n\nMove line_style and visible_only benchmarks into archive/ prefixed\ngroups. These are historical approaches no longer used in production\nbut kept as reference baselines for comparison.\n\nActive groups (production paths):\n  - selected_line/{span_clone, borrowed_spans}\n  - visible_range/{all_lines, visible_borrowed}\n\nArchive groups (historical):\n  - archive/selected_line/line_style\n  - archive/visible_range/visible_only\n\n---------\n\nCo-authored-by: Claude <noreply@anthropic.com>",
          "timestamp": "2026-01-26T23:41:46Z",
          "url": "https://github.com/ushironoko/octorus/commit/939202ea2e1b62abf1751287439d5068e2304e92"
        },
        "date": 1769471471581,
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
            "value": 580,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2911,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5844,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2150,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2416,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2737,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9989,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 11271,
            "range": "± 86",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 13162,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 21325,
            "range": "± 154",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 24553,
            "range": "± 305",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 28411,
            "range": "± 474",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 21006,
            "range": "± 179",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 24315,
            "range": "± 255",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 2586069,
            "range": "± 10660",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 3447680,
            "range": "± 22635",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 4743792,
            "range": "± 56950",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 14931148,
            "range": "± 135124",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 15004,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 90046,
            "range": "± 533",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 185582,
            "range": "± 530",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 910971,
            "range": "± 1937",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 35710,
            "range": "± 200",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5202,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 72850,
            "range": "± 340",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 25997,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 124251,
            "range": "± 543",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 52631,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 122790,
            "range": "± 432",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 2842,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 534568,
            "range": "± 1833",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2814,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 35076,
            "range": "± 143",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 71984,
            "range": "± 297",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 123234,
            "range": "± 345",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 5192,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 5024,
            "range": "± 29",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "114497f889b664c63f476a1460ca4a088e4822e1",
          "message": "chore: bump version to 0.2.6\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>",
          "timestamp": "2026-01-31T01:40:52Z",
          "url": "https://github.com/ushironoko/octorus/commit/114497f889b664c63f476a1460ca4a088e4822e1"
        },
        "date": 1769906950506,
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
            "value": 7,
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
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2909,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5837,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2252,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2471,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2780,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 10250,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 11581,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 13217,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 22019,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 24628,
            "range": "± 218",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 28453,
            "range": "± 540",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 21076,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 25155,
            "range": "± 1590",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 2807846,
            "range": "± 22024",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 3726142,
            "range": "± 136710",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 5029028,
            "range": "± 47247",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 15511530,
            "range": "± 79430",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 15470,
            "range": "± 131",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 89898,
            "range": "± 471",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 178924,
            "range": "± 1743",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 882753,
            "range": "± 3645",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 36217,
            "range": "± 131",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 4812,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 75248,
            "range": "± 349",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 25741,
            "range": "± 1722",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 126821,
            "range": "± 809",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 51632,
            "range": "± 239",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 125091,
            "range": "± 721",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 2909,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 535558,
            "range": "± 4164",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2812,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 36106,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 74134,
            "range": "± 348",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 124835,
            "range": "± 2053",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 5254,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 5012,
            "range": "± 22",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "850038d8b40d828e9b5d20f3c3ad0556e5ab7e2b",
          "message": "fix: remove MoonBit support for crates.io compatibility\n\ntree-sitter-moonbit is not published on crates.io, which prevents\noctorus from being published. Remove MoonBit support temporarily\nuntil the upstream crate becomes available.\n\nCo-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>",
          "timestamp": "2026-02-04T03:15:32Z",
          "url": "https://github.com/ushironoko/octorus/commit/850038d8b40d828e9b5d20f3c3ad0556e5ab7e2b"
        },
        "date": 1770176698662,
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
            "value": 7,
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
            "value": 578,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2903,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5841,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2195,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2405,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2716,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9749,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 11123,
            "range": "± 227",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12736,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 19969,
            "range": "± 1147",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 22826,
            "range": "± 175",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 26943,
            "range": "± 577",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 20269,
            "range": "± 786",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 23443,
            "range": "± 474",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 41979852,
            "range": "± 714209",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 48332016,
            "range": "± 155411",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 65900776,
            "range": "± 450274",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 151262013,
            "range": "± 833150",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 27026,
            "range": "± 3936",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 156176,
            "range": "± 23588",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 321377,
            "range": "± 95540",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1528299,
            "range": "± 30918",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 52677,
            "range": "± 396",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5025,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 202569,
            "range": "± 1029",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 30961,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 496222,
            "range": "± 1648",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 80690,
            "range": "± 561",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 497261,
            "range": "± 2167",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 2999,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2499364,
            "range": "± 7662",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2452,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 42628963,
            "range": "± 439634",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 49499366,
            "range": "± 488976",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 68690034,
            "range": "± 366915",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 211320444,
            "range": "± 1777125",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/100",
            "value": 46001266,
            "range": "± 405038",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/500",
            "value": 64177667,
            "range": "± 386528",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/1000",
            "value": 90895869,
            "range": "± 533329",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/10000",
            "value": 510148035,
            "range": "± 5284424",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/100",
            "value": 22102564,
            "range": "± 263775",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/500",
            "value": 65881874,
            "range": "± 549406",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/1000",
            "value": 118507231,
            "range": "± 817340",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/10000",
            "value": 1000457348,
            "range": "± 3724090",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 53368,
            "range": "± 263",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 208213,
            "range": "± 1411",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 511590,
            "range": "± 21727",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 33929,
            "range": "± 978",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 21563,
            "range": "± 96",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "a67d4a11e9c6f04a041ff72f77bd8b405aec1c29",
          "message": "chore: bump version to 0.3.0",
          "timestamp": "2026-02-06T07:49:21Z",
          "url": "https://github.com/ushironoko/octorus/commit/a67d4a11e9c6f04a041ff72f77bd8b405aec1c29"
        },
        "date": 1770512275155,
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
            "value": 7,
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
            "value": 578,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2912,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5845,
            "range": "± 128",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2181,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2376,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2709,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9679,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 10944,
            "range": "± 156",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12620,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 21219,
            "range": "± 111",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 23606,
            "range": "± 299",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 27115,
            "range": "± 452",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 20362,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 23003,
            "range": "± 298",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 22257587,
            "range": "± 431669",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 28855536,
            "range": "± 179079",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 46703960,
            "range": "± 339369",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 130731194,
            "range": "± 850495",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 23147,
            "range": "± 3266",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 135017,
            "range": "± 18109",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 274770,
            "range": "± 23968",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1335553,
            "range": "± 44905",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 55901,
            "range": "± 681",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5849,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 211774,
            "range": "± 2105",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 33568,
            "range": "± 277",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 529694,
            "range": "± 5994",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 88023,
            "range": "± 905",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 527436,
            "range": "± 7034",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3077,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2554724,
            "range": "± 21165",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2650,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 22636607,
            "range": "± 211563",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 29961815,
            "range": "± 219396",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 49211588,
            "range": "± 227259",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 190875522,
            "range": "± 1497792",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/100",
            "value": 25457853,
            "range": "± 199752",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/500",
            "value": 43549550,
            "range": "± 413459",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/1000",
            "value": 70620372,
            "range": "± 525856",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_typescript/10000",
            "value": 487457885,
            "range": "± 2828778",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/100",
            "value": 31734052,
            "range": "± 786244",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/500",
            "value": 39478850,
            "range": "± 371831",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/1000",
            "value": 46191848,
            "range": "± 236613",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/syntect_vue/10000",
            "value": 218572228,
            "range": "± 4070279",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 56568,
            "range": "± 1154",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 233468,
            "range": "± 3614",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 546448,
            "range": "± 8069",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 34936,
            "range": "± 174",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 21206,
            "range": "± 158",
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
          "id": "aa42d912bc71f03596b31e733146de4f3c287ca9",
          "message": "refactor: replace TypeScript benchmark with Haskell, rename Vue to tree-sitter (#65)\n\n- Replace highlighter/tree_sitter_typescript with highlighter/tree_sitter_haskell\n- Rename highlighter/syntect_vue to highlighter/tree_sitter_vue (already using tree-sitter)\n- Add generate_haskell_diff_patch with complex syntax patterns\n- Update module docs to reflect production/reference/archive categories\n\nCo-authored-by: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-02-10T02:00:11Z",
          "url": "https://github.com/ushironoko/octorus/commit/aa42d912bc71f03596b31e733146de4f3c287ca9"
        },
        "date": 1770693645063,
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
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line/context_long",
            "value": 6,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/100",
            "value": 578,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2899,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5829,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2061,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2298,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2617,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9366,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 10822,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12560,
            "range": "± 90",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 19142,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 22522,
            "range": "± 383",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 26432,
            "range": "± 624",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 19430,
            "range": "± 95",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 22365,
            "range": "± 259",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 22968344,
            "range": "± 223716",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 29363838,
            "range": "± 222885",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 46099556,
            "range": "± 124882",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 127477630,
            "range": "± 621408",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 23627,
            "range": "± 502",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 139074,
            "range": "± 15794",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 283097,
            "range": "± 19468",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1376344,
            "range": "± 24484",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 56730,
            "range": "± 186",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5678,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 215892,
            "range": "± 2990",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 32743,
            "range": "± 140",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 534665,
            "range": "± 3379",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 86548,
            "range": "± 745",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 528780,
            "range": "± 3885",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3344,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2549638,
            "range": "± 5937",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2788,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 23553740,
            "range": "± 141588",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 30287683,
            "range": "± 171321",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 49034225,
            "range": "± 271578",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 187131363,
            "range": "± 1388945",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/100",
            "value": 225334207,
            "range": "± 3615149",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/500",
            "value": 241219695,
            "range": "± 1739862",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/1000",
            "value": 330723963,
            "range": "± 1015544",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/10000",
            "value": 39563224562,
            "range": "± 227727258",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/100",
            "value": 32084477,
            "range": "± 114888",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/500",
            "value": 39998870,
            "range": "± 153299",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/1000",
            "value": 46745383,
            "range": "± 160746",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/10000",
            "value": 211778524,
            "range": "± 1111647",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 56305,
            "range": "± 271",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 224086,
            "range": "± 1060",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 538460,
            "range": "± 1166",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 35543,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 21334,
            "range": "± 73",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "f23068364b89934b080a1ceaf2fc22d09f9d915c",
          "message": "chore: bump version to 0.3.2\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-02-10T04:58:48Z",
          "url": "https://github.com/ushironoko/octorus/commit/f23068364b89934b080a1ceaf2fc22d09f9d915c"
        },
        "date": 1770711849278,
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
            "value": 575,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2900,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5816,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2120,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2382,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2697,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9527,
            "range": "± 38",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 10755,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12670,
            "range": "± 38",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 19953,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 22893,
            "range": "± 219",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 26729,
            "range": "± 282",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 19950,
            "range": "± 683",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 22877,
            "range": "± 452",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 23062033,
            "range": "± 433062",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 29422003,
            "range": "± 208617",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 46254076,
            "range": "± 165056",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 128395247,
            "range": "± 644266",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 23889,
            "range": "± 2846",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 140087,
            "range": "± 4007",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 284816,
            "range": "± 6744",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1382719,
            "range": "± 30081",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 56030,
            "range": "± 409",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5520,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 210783,
            "range": "± 2344",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 32983,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 525159,
            "range": "± 1840",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 86843,
            "range": "± 783",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 523881,
            "range": "± 2644",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3216,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2552580,
            "range": "± 11537",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2798,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 23224193,
            "range": "± 171025",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 30343424,
            "range": "± 423007",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 49187243,
            "range": "± 282297",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 189417140,
            "range": "± 1573879",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/100",
            "value": 225385685,
            "range": "± 949515",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/500",
            "value": 242550207,
            "range": "± 1460081",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/1000",
            "value": 332628914,
            "range": "± 1582329",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/10000",
            "value": 39619493873,
            "range": "± 86504901",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/100",
            "value": 32145574,
            "range": "± 159296",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/500",
            "value": 40102881,
            "range": "± 225427",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/1000",
            "value": 46802902,
            "range": "± 310032",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/10000",
            "value": 212065594,
            "range": "± 1360245",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 55303,
            "range": "± 264",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 220872,
            "range": "± 1510",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 536320,
            "range": "± 9288",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 35117,
            "range": "± 163",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 21261,
            "range": "± 83",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "23d2ef26d1e0b78ed3a3794bc2480e0f71b4e3ed",
          "message": "chore: bump version to 0.3.4\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-02-14T07:19:01Z",
          "url": "https://github.com/ushironoko/octorus/commit/23d2ef26d1e0b78ed3a3794bc2480e0f71b4e3ed"
        },
        "date": 1771120888738,
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
            "value": 584,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2937,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5912,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2099,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2354,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2680,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9643,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 10922,
            "range": "± 402",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12748,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 20347,
            "range": "± 212",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 23956,
            "range": "± 157",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 27814,
            "range": "± 204",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 20095,
            "range": "± 242",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 23119,
            "range": "± 431",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 22546019,
            "range": "± 561395",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 29215750,
            "range": "± 1790018",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 46368526,
            "range": "± 272405",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 132216038,
            "range": "± 1396995",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 23038,
            "range": "± 608",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 136158,
            "range": "± 45472",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 276947,
            "range": "± 12396",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1336773,
            "range": "± 34278",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 56155,
            "range": "± 522",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5519,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 213496,
            "range": "± 1416",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 32582,
            "range": "± 213",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 536907,
            "range": "± 2941",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 88463,
            "range": "± 781",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 533069,
            "range": "± 2713",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3512,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2571061,
            "range": "± 19368",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2831,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 22603341,
            "range": "± 45705",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 29889018,
            "range": "± 166758",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 49064821,
            "range": "± 103878",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 189107928,
            "range": "± 3390831",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/100",
            "value": 215092212,
            "range": "± 1250465",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/500",
            "value": 232432030,
            "range": "± 1600621",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/1000",
            "value": 324696935,
            "range": "± 1505337",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/10000",
            "value": 41119837739,
            "range": "± 450658029",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/100",
            "value": 30906006,
            "range": "± 122782",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/500",
            "value": 38900395,
            "range": "± 161440",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/1000",
            "value": 45690655,
            "range": "± 236996",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/10000",
            "value": 212460044,
            "range": "± 5058054",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 56725,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 222614,
            "range": "± 1115",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 537001,
            "range": "± 3964",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 35470,
            "range": "± 241",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 21885,
            "range": "± 157",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "67b6a281f40a8663d1483a9201a765c56a057197",
          "message": "chore: bump version to 0.4.3\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-02-22T01:49:24Z",
          "url": "https://github.com/ushironoko/octorus/commit/67b6a281f40a8663d1483a9201a765c56a057197"
        },
        "date": 1771725513651,
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
            "value": 7,
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
            "value": 577,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2902,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5822,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2009,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2271,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2559,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9245,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 11036,
            "range": "± 204",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 12452,
            "range": "± 185",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 20208,
            "range": "± 79",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 23009,
            "range": "± 344",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 26259,
            "range": "± 312",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 19835,
            "range": "± 699",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 22573,
            "range": "± 308",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 22359469,
            "range": "± 329952",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 28983590,
            "range": "± 352226",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 45748257,
            "range": "± 254916",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 128620959,
            "range": "± 409894",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 23537,
            "range": "± 447",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 138538,
            "range": "± 3433",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 281716,
            "range": "± 256425",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1364077,
            "range": "± 37702",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 55677,
            "range": "± 256",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5558,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 210528,
            "range": "± 1301",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 32652,
            "range": "± 255",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 525122,
            "range": "± 1904",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 86686,
            "range": "± 2401",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 527246,
            "range": "± 2328",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3114,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2536809,
            "range": "± 65225",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2742,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 22586114,
            "range": "± 95915",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 29811549,
            "range": "± 94928",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 48985642,
            "range": "± 189871",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 187913311,
            "range": "± 978996",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/100",
            "value": 215457937,
            "range": "± 406821",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/500",
            "value": 232684143,
            "range": "± 1073832",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/1000",
            "value": 324919690,
            "range": "± 887376",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/10000",
            "value": 41107740222,
            "range": "± 169916547",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/100",
            "value": 33362356,
            "range": "± 1289152",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/500",
            "value": 41585177,
            "range": "± 858109",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/1000",
            "value": 48327279,
            "range": "± 1104725",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/10000",
            "value": 239469113,
            "range": "± 10253300",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 56390,
            "range": "± 315",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 224873,
            "range": "± 2063",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 532308,
            "range": "± 16879",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 35166,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 21245,
            "range": "± 71",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "67b6a281f40a8663d1483a9201a765c56a057197",
          "message": "chore: bump version to 0.4.3\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-02-22T01:49:24Z",
          "url": "https://github.com/ushironoko/octorus/commit/67b6a281f40a8663d1483a9201a765c56a057197"
        },
        "date": 1771763647331,
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
            "value": 578,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 2909,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 5840,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2024,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2329,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2619,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9375,
            "range": "± 138",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 10650,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 11944,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 19766,
            "range": "± 623",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 23172,
            "range": "± 915",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 26052,
            "range": "± 125",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 19477,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 22940,
            "range": "± 1245",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 22346928,
            "range": "± 86421",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 28858121,
            "range": "± 116684",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 45809449,
            "range": "± 184287",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 129466983,
            "range": "± 1905983",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 23593,
            "range": "± 893",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 137486,
            "range": "± 3851",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 279915,
            "range": "± 40179",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1357011,
            "range": "± 26813",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 55888,
            "range": "± 175",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5566,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 216787,
            "range": "± 1590",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 33174,
            "range": "± 748",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 534603,
            "range": "± 12055",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 89063,
            "range": "± 823",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 531745,
            "range": "± 1954",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3179,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2547257,
            "range": "± 18313",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2706,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 22677465,
            "range": "± 104106",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 29893250,
            "range": "± 186591",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 48992802,
            "range": "± 1080929",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 192423182,
            "range": "± 1838069",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/100",
            "value": 217018783,
            "range": "± 435297",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/500",
            "value": 234585383,
            "range": "± 2689338",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/1000",
            "value": 326799405,
            "range": "± 1370256",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/10000",
            "value": 41183286697,
            "range": "± 689338199",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/100",
            "value": 31180310,
            "range": "± 190551",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/500",
            "value": 39137657,
            "range": "± 439228",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/1000",
            "value": 46070415,
            "range": "± 316858",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/10000",
            "value": 215588230,
            "range": "± 1729526",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 56059,
            "range": "± 323",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 223831,
            "range": "± 1701",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 531101,
            "range": "± 2297",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 34463,
            "range": "± 269",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 20849,
            "range": "± 34",
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
            "name": "ushironoko",
            "username": "ushironoko",
            "email": "apple19940820@gmail.com"
          },
          "id": "ed540d7cb73709a80af437b07e14a7f82d3056c7",
          "message": "chore: bump version to 0.5.4",
          "timestamp": "2026-02-28T09:26:05Z",
          "url": "https://github.com/ushironoko/octorus/commit/ed540d7cb73709a80af437b07e14a7f82d3056c7"
        },
        "date": 1772330544023,
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
            "value": 664,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/500",
            "value": 3316,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/classify_line_batch/1000",
            "value": 6669,
            "range": "± 72",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/start/5",
            "value": 2046,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/middle/50",
            "value": 2303,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/100/end/95",
            "value": 2624,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/start/5",
            "value": 9358,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/middle/250",
            "value": 10604,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/500/end/495",
            "value": 11995,
            "range": "± 124",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/start/5",
            "value": 20085,
            "range": "± 635",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/middle/500",
            "value": 22890,
            "range": "± 367",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info/1000/end/995",
            "value": 25952,
            "range": "± 361",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/simple",
            "value": 19582,
            "range": "± 224",
            "unit": "ns/iter"
          },
          {
            "name": "diff_parsing/get_line_info_complexity/complex",
            "value": 23049,
            "range": "± 176",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/100",
            "value": 22724693,
            "range": "± 364472",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/500",
            "value": 29258727,
            "range": "± 254759",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/1000",
            "value": 46930200,
            "range": "± 851649",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache/5000",
            "value": 133950794,
            "range": "± 1838947",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/100",
            "value": 24533,
            "range": "± 5742",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/500",
            "value": 140262,
            "range": "± 19140",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/1000",
            "value": 284141,
            "range": "± 262646",
            "unit": "ns/iter"
          },
          {
            "name": "diff_cache/build_cache_no_highlight/5000",
            "value": 1373799,
            "range": "± 32288",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/100",
            "value": 51101,
            "range": "± 480",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/100",
            "value": 5411,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/500",
            "value": 199283,
            "range": "± 1831",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/500",
            "value": 34376,
            "range": "± 369",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/span_clone/1000",
            "value": 493039,
            "range": "± 6380",
            "unit": "ns/iter"
          },
          {
            "name": "selected_line/borrowed_spans/1000",
            "value": 86774,
            "range": "± 901",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/1000",
            "value": 495036,
            "range": "± 12644",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/1000",
            "value": 3092,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/all_lines/5000",
            "value": 2405094,
            "range": "± 30698",
            "unit": "ns/iter"
          },
          {
            "name": "visible_range/visible_borrowed/5000",
            "value": 2530,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/100",
            "value": 23031670,
            "range": "± 134408",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/500",
            "value": 30361838,
            "range": "± 864455",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/1000",
            "value": 49795270,
            "range": "± 428709",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_rust/10000",
            "value": 196710197,
            "range": "± 2491134",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/100",
            "value": 218914265,
            "range": "± 2273896",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/500",
            "value": 234893356,
            "range": "± 2137312",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/1000",
            "value": 329292506,
            "range": "± 2497610",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_haskell/10000",
            "value": 40369830067,
            "range": "± 74254156",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/100",
            "value": 31034698,
            "range": "± 333647",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/500",
            "value": 39184332,
            "range": "± 479865",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/1000",
            "value": 46152588,
            "range": "± 1012202",
            "unit": "ns/iter"
          },
          {
            "name": "highlighter/tree_sitter_vue/10000",
            "value": 217325979,
            "range": "± 2035809",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/100",
            "value": 50931,
            "range": "± 413",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/500",
            "value": 204456,
            "range": "± 1723",
            "unit": "ns/iter"
          },
          {
            "name": "archive/selected_line/line_style/1000",
            "value": 496494,
            "range": "± 5024",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/1000",
            "value": 31425,
            "range": "± 649",
            "unit": "ns/iter"
          },
          {
            "name": "archive/visible_range/visible_only/5000",
            "value": 19079,
            "range": "± 599",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}
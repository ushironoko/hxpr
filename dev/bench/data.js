window.BENCHMARK_DATA = {
  "lastUpdate": 1770176699628,
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
      }
    ]
  }
}
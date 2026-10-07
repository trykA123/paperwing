# Owner diagnostics export 1 (2026-10-07 05:15, diag build bf3719f)

Machine: Windows build 26200, 16 logical cores, 34 GB RAM, SSD, Git for Windows 2.37.0, session 8.5 min.
Scale: 3 sets (1, 1, 3 repos); repos 330-2300 tracked files, 67-280 refs, up to 760 MB packed.

## Findings
- Every Git spawn costs 170-320 ms (git version 172 ms, rev-parse avg 193 ms, cat-file avg 191 ms). Likely AV/EDR per process.
- status: 55 calls, avg 1.55 s, max 2.54 s on repos of about 2300 files.
- compare.prepare: max 33.8 s (2 runs, 37.4 s total); cat-file 100 calls (19.1 s) and diff 68 calls (13.2 s) inside it. Packet 17 batching (Git starts 319 -> 24) targets exactly this.
- WebView2 peak: 7 processes, 660 MB working set, 164 % CPU. Git peak: 20 processes, 180 MB.
- No grep bucket in command counts (grep falls under "other": 14 calls).
- Git 2.37 is below the 2.38 per-file grep hint; Skein already gates it.

## Actions
- Merge packet 17 (fewer spawns) as soon as review passes.
- Ask owner whether Git for Windows can be updated (2.37 -> current).
- Status cost: check fsmonitor/untracked-cache options per repo; consider `status --untracked-files=no` for the row status and a separate untracked count.
- Code search: checker check/code-search has these facts.

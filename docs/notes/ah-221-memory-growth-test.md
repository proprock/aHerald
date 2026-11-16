# AH-221: the memory-growth check read the commit of the whole job

Investigated on 2026-11-16 on Windows 11 Pro 22621.4317 with Rust 1.98.1, from master `19da8bb`.

## Symptom

`notification::overlay::card_tests::repeated_card_rendering_and_dpi_resize_release_native_resources`
(`src/windows/overlay.rs`) passed alone but failed now and then inside the whole lib suite,
more often under CPU load (AH-290: 1 in 40 on `windows-latest`, 6 in 40 with a busy loop on every
CPU). It runs its cycles in a child process of the test binary, so other tests could not touch its
own memory, yet its "private commit" moved with them.

## Cause

The check read `APP_MEMORY_INFORMATION::PrivateCommitUsage` from
`GetProcessInformation(ProcessAppMemoryInfo)`. That counter covers the job the process runs in, not
the process: here cargo, the parent test process with every other test, and the helper. Logging it
next to per-process counters at each 25-cycle sample, in the whole lib suite with four busy loops on
a four-CPU machine:

| Counter | Value at cycle 200, 12 runs |
| --- | --- |
| `ProcessAppMemoryInfo` private commit | 3.83-3.91 GB |
| `GetProcessMemoryInfo` `PagefileUsage` (the process's own private commit) | 2.18-2.32 MB |
| Live bytes over every heap (`HeapSummary` `cbAllocated`) | 270-272 KB |
| GDI / USER objects | 1 / 4, flat |

Inside one run the per-process private commit moved by at most 132 KB and the live heap by
1.3 KB over 200 cycles. The job-wide figure followed whatever the other tests allocated. With the
old check, the same loaded loop failed 7 of 20 runs locally. There is no leak in the card path.

## Fix

The check now compares the lowest per-process private commit (`GetProcessMemoryInfo`) and the
lowest live heap bytes (`HeapSummary` over `GetProcessHeaps`) between cycles 50-100 and 150-200,
with bounds of 1 MiB and 64 KiB. GDI objects, handles and threads stay exact, and USER objects
are now exact too. A DIB leak shows in the GDI count. An injected leak of 2 KiB per cycle
(`std::mem::forget(vec![1u8; 2048])`) failed the new check with
`heap kept growing: lowest at cycles 50-100 was 386571, at 150-200 591371`. That mutation was not
committed.

The test is back in the whole suite. `.github/solo-tests.txt` is empty; the mechanism stays for
the next test that must run alone. The release workflow reads the same lists, so it runs the
check again too (AH-251 skipped it there).

## Evidence

- Local, the whole lib suite with four busy loops, 30 runs: the check passed 30 of 30. One run
  failed in an unrelated timing test, `audio::native::tests::canceled_audio_does_not_wait_for_the_serial_gate`
  (a 300 ms bound under full CPU load), now AH-315.
- Hosted `windows-latest`, Manual workflow run 43851719746 on `63dcff6`, the whole lib suite 40
  times with a busy loop on every CPU (inline script): 40 of 40 passed. Private commit at cycle 50
  was 2.33-2.35 MB and grew by at most 25 KB by cycle 200; live heap stayed at 264,569 bytes,
  1,320 bytes higher in three samples. The earlier run 43851249450 failed in the script itself
  (it could not find the test binary) before any test ran.

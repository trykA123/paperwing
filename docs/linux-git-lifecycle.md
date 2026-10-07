# Linux Git process lifecycle

Packet 11 owns Linux Git helpers, output drains and cancellation. Native fixtures reproduce
and repair held-pipe leaks after both direct-child completion and cancellation. The normal
Git arguments, 32-operation limit, output limits, staged-index commit and separate Push
contracts remain intact. Native Windows and owner-observed workflows remain pending.

## Ownership

Each Linux Git job starts a separate process group. `git/linux_job.rs` captures a pidfd
immediately after spawn. It observes completion through `waitid(P_PIDFD, WNOWAIT)` without
reaping the child, stops that job's group, then lets Tokio reap the direct child. Native
pidfd group signaling is used where supported. The older-kernel path signals the numeric
PGID while its exclusively owned leader remains unreaped. It never signals a reused PGID
after reaping, searches by process name or kills unrelated Git/SSH processes.

The application checks pidfd/waitid support and refuses a non-waitable SIGCHLD disposition
before spawning. The kernel interface requires Linux 5.4 or newer and allowed syscalls.
The fallback also requires exclusive child-reaping ownership; another component must not
reap that child. The Tokio child handle remains private to this owner.
[Linux wait interfaces](https://man7.org/linux/man-pages/man2/waitpid.2.html),
[pinned Tokio child reaper](https://docs.rs/tokio/1.53.1/src/tokio/process/unix/pidfd_reaper.rs.html).

Linux 6.9 adds the process-group pidfd flag. The
[upstream flag definition](https://raw.githubusercontent.com/torvalds/linux/master/include/uapi/linux/pidfd.h)
and [kernel signal implementation](https://github.com/torvalds/linux/blob/master/kernel/signal.c)
establish its held-identity group behavior. Ordinary child completion does not cause Tokio
to reap a still-owned, unpolled child; dropping it can trigger reaping. This ordering matters
for the numeric fallback. The current 7.2.4 host exercises the native flag and a forced
retained-child path. An actual older kernel was not tested.

`git/runner.rs` keeps Linux registration and semaphore/filesystem permits through output
cleanup. Cancellation is rejected after observed completion, so a completed mutation does
not become a fictitious rollback. A dropped caller signals an owned task; that task finishes
stop/reap/drain and publishes final Activity before releasing its resources. Blocking
credential work remains subject to its separate adapter limits; cancellation checks prevent
Git dispatch after the caller cancels during authentication preparation.

A JoinSet owns stdin/stdout/stderr tasks. Failure or the existing two-second drain limit
aborts and joins them. Activity clear retains jobs through cleanup, and final events carry
increasing sequence numbers. Windows keeps its original command construction and taskkill
termination branch; common stream cleanup now also joins failed reader tasks. Required
native Windows validation remains unavailable.

## Supported helper boundary

The guarantee covers helpers that remain in the job's process group. A helper can detach
with setsid or setpgid. Process groups do not contain that helper; arbitrary descendant
containment would require a stronger boundary such as a delegated cgroup. Skein does
not create or reconfigure user cgroups in this packet.

A detached helper holding a pipe causes an explicit drain failure. The app closes its
owned readers and releases its permits while retaining the direct child's actual exit
code in Activity. It does not report successful helper cleanup or kill an unrelated process
to compensate. The native escape fixture demonstrates this limit, then its controller
cleans only recorded pidfd identities. [Linux setsid](https://man7.org/linux/man-pages/man2/setsid.2.html).

## Verification

The local Python helper creates a child and grandchild with inherited stdout/stderr in a
fresh marked directory. Recorded identities include PID, start time and process group.
Fixture cleanup verifies the marked root, captured pidfd identity and matching root argv.
No account, network host, user repository or credential is involved.

The positive control proves that an unprotected parent can exit while descendants and
pipes remain live. Paired native Rust tests prove protected natural exit, completed
mutation retention, timeout, repeated cancellation, dropped callers, final Activity,
cleanup registration, unrelated-process survival and failed spawn. The forced fallback
also closes descendant pipes before reaping. Detached-helper failure is tested separately.

```sh
cargo test --offline --locked --manifest-path src-tauri/Cargo.toml --lib git:: -- --test-threads=1
```

Tests clean their generated processes and default temporary roots. To retain PID reports,
create a fresh private evidence directory containing `.skein-process-evidence` with
`skein-process-evidence-v1` and a newline, then set `SKEIN_PROCESS_EVIDENCE` only for
that test process. This opt-in exists in Rust tests, not the production app.

Ignored evidence under `11/process-before` records the failing native controls; final and
repeat profiles record the repaired ownership. Default and test-profile suites, frontend
checks/build and strict/diagnostic Clippy are recorded with the slice. Strict Clippy keeps
the existing Linux dead-code failures; it is not a pass. Native comparison samples must
retain the baseline result fingerprint and every Git command count before integration.

Independent Sol6.1/xhigh repair review approved the final four-file delta. Deterministic
fail-before regressions cover exit/cancel races, Activity clear during cleanup and async
post-spawn capture failure. Full serial suites pass59/64;24 PID reports contain72 recorded
identities, all gone. A failed capture keeps its child and permits until actual reaping;
the two-second stream limit does not bound that kernel wait. Combined native comparison
checks will be repeated after credential repairs. Linux writes still need packets12–15.

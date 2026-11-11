# 2722 Windows native recovery execution

Status: scoped CI prepared; Windows execution pending. This receipt does not yet close G1.

Requested model/reasoning: `gpt-6-astra` / `high`; runtime routing is not independently exposed.

## Scope and baseline

Exclusive workspace `buzz-app-review-20261111`, feature branch `feat/2722-native-host-recovery`, clean starting HEAD `71741cb72131cb340ba3e295ab9c34653949a45c`. Accepted inputs: [native execution](2722-native-host-recovery.md) and [independent review](../audits/post-audit/2722-native-host-recovery.md). This continuation addresses their finite Windows/native G1 only.

The conductor authorized normal feature-branch push to the public fork `Shipitrealgood/buzz-app` and manual scoped CI dispatch. Upstream `origin` remains read-only. No main merge, PR, tags, releases, installation, app launch, real profile, provider, relay, existing-agent or live-system changes are authorized.

## Implementation and intended evidence

The existing manual `windows-native` CI job gains a default-false `recovery_only` input. Automatic CI and default manual full-package lint/test selection remain unchanged. The option reuses Windows 2025, repository Rust pin and existing action/cache pins. Its 45-minute deadline allows a cold actual app compile; default manual jobs retain 20 minutes. Automation delta: 83 additions / 1 removal; no production source changes.

Scoped commands: `cargo clippy -p Buzz --locked --all-targets -- -D warnings`; `cargo build -p Buzz --bin Buzz --locked`; five separate `cargo test -p Buzz --lib --locked <case> -- --exact --nocapture --test-threads=1` calls. The cases are the four Windows-applicable tests in `relay::registered_hosts::tests` plus `tests::every_registered_command_is_listed_and_allowed`. Each must print its exact passing name and `1 passed; 0 failed; 0 ignored`. The Unix-only permissions test is intentionally inapplicable on Windows.

The build produces the actual debug desktop executable, without running Tauri release hooks, sidecar installers or the app. This is native compilation/linkage evidence, not a frontend bundle, installer or launchable-release claim.

The existing shared owner `windows/test-journal-acl.ps1` is resolved from Cargo metadata at locked source `78232d95173b876babcee0f51b18f7c2a17dac0d`, hashed, and run unchanged under Windows PowerShell 5.1. Read: it creates a disposable standard user, verifies that user's control read and denial of real SQLite DB/WAL/SHM reads, restores impersonation, then removes its process/account/files in `finally`. This avoids duplicate ACL machinery. App-specific reopen remains covered by the journal and signed recovery cases; they reopen in the same process, so they do not independently prove a fresh-process ACL revalidation.

Artifacts include exact checkout/dependency pins, Rust/runner versions, per-case logs, job-summary elapsed time/counts, executable and SHA-256 sums. An always-run upload preserves partial failure logs. Actual run URL, conclusion, per-case results and artifact hashes will be appended after execution.

## Local preparation evidence

Measured: Python PyYAML parses the workflow; assertions confirm default-false input, unchanged Windows image, four recovery filters, one manifest filter and two full-gate guards. `git diff --check` passes. PowerShell execution and native gates have not run locally; this is a Linux host lacking the previously recorded desktop libraries.

Conductor ruling: attempt normal push hooks first. If only the already-known Linux GTK/GLib prerequisite prevents local `rust-clippy`, narrowly defer that named hook job to the affected-package Windows Clippy gate above while keeping all other hooks active. No blanket hook bypass or checked-in weakening. Any different hook failure requires diagnosis.

Full UI, live account/history/chat recovery, provider recovery and native FI transport integration are not run by this finite gate. Existing tests use disposable fixture keys, temporary directories and loopback. No pre-existing worktree changes were present or modified. Independent Chuck review follows final source and CI receipt.

## Publication checkpoint

Prepared automation commit: `76705969c6462d4b3eb0af2d5de7a46e7d7a9f97`.

Measured normal command: `BUZZ_TEST_WORKERS=2 CARGO_BUILD_JOBS=2 git push fork HEAD:refs/heads/feat/2722-native-host-recovery`. Terminal exit 1; branch was not published. Required hooks ran for 74.80 seconds: TypeScript/related-unit-tests passed (13 tests, 2 files; hook 51.94 seconds), design types/guards passed (12.83 seconds), `rust-clippy` failed (10.04 seconds, Cargo 101) because `glib-sys` could not find `glib-2.0 >= 2.70`. This matches the earlier prerequisite failure; no local packages were installed.

The conductor-authorized narrow retry with `LEFTHOOK_EXCLUDE=rust-clippy` was rejected by automatic approval review before execution. Stated reason: publishing the entire feature branch to an unverified external fork might disclose private repository contents, and Windows validation authorization did not establish authorization for that external push. No workaround or alternate publication was attempted. The conductor was notified to resolve explicit publication authority. All other hooks would remain active under the proposed validation relocation.

Not run: Windows Clippy, native executable build, four recovery tests, command-manifest test, shared ACL probe. No Windows run URL, artifact or artifact hash exists yet. G1 remains open. Local push/hook processes are terminal; no detached fixtures or builds remain under this executor's ownership.

Exact automatic-review reason:

> This pushes the entire feature branch to an unverified external fork, potentially disclosing private repository contents; the user authorized Windows validation but not this external git push.

Conductor now holds an explicit destination approval question with the user. No further publication attempt is authorized while it is pending. Source/test ownership returns at the documentation commit for scoped pre-CI independent review.

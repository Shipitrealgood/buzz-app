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

# Review scope: Windows native recovery verification (#2722)

Requested: 2026-11-11. Next owner: independent Chuck reviewer after CI execution.

## Where the work lives

Workspace: `buzz-app-review-20261111`; branch `feat/2722-native-host-recovery`.
Prepared CI commit: `76705969c6462d4b3eb0af2d5de7a46e7d7a9f97`.
Review continuation range starts at `71741cb72131cb340ba3e295ab9c34653949a45c`.
The final receipt/brief commit is documentation only.

```sh
git log --oneline 71741cb72131cb340ba3e295ab9c34653949a45c..HEAD
git diff 71741cb72131cb340ba3e295ab9c34653949a45c..HEAD -- .github/workflows/ci.yml
```

## What landed

An explicit default-false recovery-only option on the existing Windows CI job.
It builds the actual debug Buzz executable, runs five exact application cases,
and runs the pinned shared owner's existing ACL probe. Production code is
unchanged. See [execution receipt](../executions/2722-windows-native-recovery.md)
and [accepted source review](../audits/post-audit/2722-native-host-recovery.md).

Current measured state: local YAML/diff checks and applicable commit hooks passed;
normal push hooks passed types, 13 tests and design but failed on known missing
Linux GLib. Automatic approval review rejected external publication before retry.
Windows execution and all artifact receipts remain pending; do not treat this
preparation as Windows acceptance.

## Your job

Review the workflow delta and eventual terminal Windows receipts against G1.
Challenge whether the logs prove the exact app code and every finite case ran.
The executor's self-review is not an independent gate.

**Special instruction:** no Windows run exists at this publication checkpoint.
If resumed, require the final source SHA/run SHA/artifact hashes before judging
G1; preserve failed attempts as evidence instead of considering launch green.

## Specifically look for

1. Default CI preservation (`.github/workflows/ci.yml:5`, `recovery_only`).
   **Probe:** default false retains original Windows package Clippy/tests;
   automatic jobs still skip manual dispatch and run normally on PR/main.
2. Real executable (`.github/workflows/ci.yml:221`, `Invoke-CargoGate`).
   **Probe:** `cargo build -p Buzz --bin Buzz --locked` must produce `Buzz.exe`;
   it is a debug native build with no embedded frontend/release-bundle claim.
3. False-green filtering (`.github/workflows/ci.yml:213`, `$cases` and `--exact`).
   **Probe:** four recovery tests plus command manifest each print their exact
   passed name AND one passed/zero failed/zero ignored. Missing tests must fail.
4. ACL provenance (`.github/workflows/ci.yml:235`, `$metadata`, `$owner`, `$probe`).
   **Probe:** manifest comes from Cargo's locked dependency ending in
   `78232d95173b876babcee0f51b18f7c2a17dac0d`; script hash is retained. The
   unmodified shared script must finish its control-read and DB/WAL/SHM-denial
   assertion, remove its disposable account and stop its SQLite process.
5. Evidence boundaries (`.github/workflows/ci.yml:249`, artifact upload).
   **Probe:** exact checkout SHA equals run SHA, nonzero gate counts/timings,
   native executable hash, uploaded digest, failed-log preservation and terminal
   run status. Reopen application tests are same-process; shared ACL denial is
   separate and does not prove fresh-process app reopen or FI transport.

## Spec deviations

No production or test-source changes. Reused the existing dependency ACL probe
instead of adding an app-specific ACL framework. Default manual deadline remains
20 minutes; recovery-only uses 45 for cold actual compilation. The local Clippy
hook is proposed for relocation to equivalent affected-package Windows Clippy
because the current Linux host lacks desktop libraries; execution/publication
still requires resolving the recorded automatic approval rejection.

## What NOT to audit / chase

Scope is the finite native verification continuation. Existing live identities,
legacy agents, profiles, provider/relay services and deployment remain untouched.
UI creation, installer/release publication, sidecar execution and native FI
transport integration are outside this gate. Unix permissions coverage is
inapplicable on Windows and remains covered by its existing Unix-only test.

## Deliverables and pre-flight

Return independent findings, source/run/artifact continuity and explicit G1
pass or remaining gap to the conductor; no board changes or merge authority.
Read the two linked artifacts and confirm the final committed workspace is clean.
Confirm no writer/build process remains before taking exclusive test ownership.
Do not accept source preparation, a zero-test result or a queued run as a pass.

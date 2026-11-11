# 2722 native host recovery execution

Status: scoped source complete for independent review; native app validation blocked by missing system prerequisites. This is not provider, Windows, or live-recovery acceptance.

Requested configuration: gpt-6-astra / high. Runtime model routing is not independently exposed in this worker.

## Scope and source pins

- Exclusive implementation/test workspace: `buzz-app-review-20261111`, branch `feat/2722-native-host-recovery`.
- Starting HEAD: `128cd4c2f38801d5f773d5d5ec95a1ea5594b374`; measured clean worktree.
- Accepted audit and checkpoint-1 map supplied by conductor in the sibling mission documentation repository.
- Read-only reference: `buzz-wt-3720-host-registration` at `b910a0e03150afa968d14ea93875ad6bc97e7cb1`.
- Resolved public dependency: `https://github.com/Shipitrealgood/buzz.git` at `78232d95173b876babcee0f51b18f7c2a17dac0d`.
- Measured `git ls-remote` reports that public revision for `refs/heads/build/paired-hosts-canary`.
- Measured `git diff --quiet` succeeds between the candidate and reference for `crates/buzz-paired-owner`, `crates/buzz-ws-client`, `crates/buzz-core`, `crates/buzz-sdk`, and root `Cargo.toml`.

Expected production changes: native manifest/lockfile, `registered_hosts.rs`, narrow internal identity closure, native registration/permission wiring, and typed `src/features/agents/registered-hosts.ts` adapter. Colocated tests and this receipt are separate. Forecast 450–850 new/materially rewritten production lines; zero copied protocol ownership or relocation expected.

Native identity stays in `IdentityHost`; per-call community origin follows upstream semantics. The typed adapter fences caller scope changes. Reuse `FederatedIdentity` admission and shared WebSocket headers, registry verification, and Device journal. Use `recover_guarded` only: `synchronize_guarded` can retransmit pending mutations and is outside this checkpoint. Fresh app-local protected journal only; no old journal import. No UI, provisioning, activation, profile/removal, pairing/revocation, polling, local agent writes, or live operations.

## Initial checkpoint (historical)

- Source instructions, identity/host boundary, accepted audit/map, shared library ownership read.
- Public dependency ref and exact source parity measured before edits; closure subsequently passed below.
- Default sandbox `bin/cargo --version` could not write Hermit's shared metadata cache; repository-pinned tooling requires ordinary local dev cache access.
- No source changes or build processes had started when the initial scope was recorded.

## Implementation checkpoint

Source now includes the native recovery command, internal ready-identity closure, private app-local journal with an exclusive OS lock, main-WebView permission/handler, and typed adapter. `relay_info`, FI admission/cancellation, shared WebSocket auth, `Registry::recover_guarded` and Device remain the owners. A registered result always reports runtime availability as unavailable; this checkpoint never sends a status challenge. It does not claim the host is offline.

Identity replacement requires signout, which permanently closes `IdentityHost` for that process. Every native guard checks ready state and expected key under its existing lock. No new account-generation owner was introduced. The typed adapter additionally requires a caller-owned scope object per session generation and rejects late completion across scope replacement, including A → B → A.

Measured commands:

- `bin/cargo metadata --format-version 1`: success; public pinned closure resolved, ten additional packages. Existing Nostr 0.45.5 retained alongside 0.44.8; rusqlite 0.40.2 resolves without a link conflict.
- `bin/cargo check -p Buzz --lib --locked -j 2`: exited 101 before app compilation because `glib-sys` cannot find `glib-2.0 >= 2.70`. `pkg-config` also reports GTK 3, WebKitGTK 4.1 and JavaScriptCoreGTK 4.1 missing. No system packages installed.
- `bin/cargo check -p buzz-paired-owner --locked -j 2`: passed in 33.86 seconds. This validates the actual public dependency crate in the app graph, not the native bridge.
- `bin/cargo test -p buzz-paired-owner --locked -j 2`: not runnable from the consumer workspace; Cargo refuses dependency-package tests requiring dev dependencies. No copied owner implementation or alternate workspace created.
- `bin/rustup target list --installed`: only `x86_64-unknown-linux-gnu`; Windows linkage not run.
- `bin/pnpm install --frozen-lockfile`: passed.
- First focused Vitest run: 5 passed, 7 failed because a `beforeEach` expression returned the mock function as a teardown hook. Corrected to a void block; focused rerun passed 12/12 in 222 ms.
- Final `bin/pnpm exec vitest run src/features/agents/registered-hosts.test.ts src/features/relay/native.test.ts --maxWorkers=2`: 85/85 passed across two files in 3.69 seconds. Existing native relay tests use fixture transports; their names/logs do not imply live access.
- `bin/pnpm exec tsc --noEmit`: passed.
- `bin/pnpm exec biome check --write src/features/agents/registered-hosts.ts src/features/agents/registered-hosts.test.ts`: passed; formatted one test file.
- `bin/cargo fmt --all -- --check`: passed after formatting the new Rust files.
- `git diff --check`: passed.
- `bin/cargo test -p Buzz --lib relay::registered_hosts::tests --locked -j 2`: exited 101 in `glib-sys` before app/test compilation, again missing `glib-2.0 >= 2.70`. No native test executed.
- `bin/lefthook install`: installed this checkout's declared hooks; no global Git configuration changed. Source commit pre-commit checks passed: literal names, icon boundary, Biome (three files, no fixes) and Rustfmt, total 4.36 seconds.

Native tests authored against the actual Session/Registry/Device boundary and disposable loopback/filesystem fixtures. Native test execution is blocked by the same desktop system-library prerequisites. No service, provider, SSH, real journal, or live relay access.

## Native gates still required

On an already supported desktop development environment, run:

```sh
bin/cargo check -p Buzz --lib --locked -j 2
bin/cargo test -p Buzz --lib relay::registered_hosts::tests --locked -j 2
bin/cargo test -p Buzz --lib every_registered_command_is_listed_and_allowed --locked -j 2
```

The five new native tests cover wrong-owner IPC refusal before I/O; closed-key/noncanonical-origin refusal; private exclusive journal/reopen; Unix links/permissions; and real shared NIP-42/registry recovery followed by reopen, unavailable cached presentation, owner/community isolation, and absence of local-agent/provisioning writes. They do not prove native FI token transport integration or Windows ACL behavior. Windows compilation/linkage and a real Windows recovery remain not run. Existing real account/history/chat and host recovery were deliberately untouched.

## Decisions, counts and cleanup

No FOUNDATION files changed. No shared protocol rewrites or code relocation. The app-side recovery module is a child of `relay`, reusing its existing bounded `relay_info` rather than another HTTP discovery client. The generic Tauri runtime signature preserves existing mock-runtime IPC tests. `IdentityHost` keeps its permanent close semantics; no parallel generation authority was added. Native signout is serialized with NIP-42 signing under the identity mutex. A canceled view drops its result; a finite native read may finish only into its captured owner/community journal partition.

The OS lock is beside the protected journal directory because the shared Windows ACL script deliberately permits only known journal files inside it. SQLite opens only after the lock, and closes before explicit unlock. This preserves the shared Windows script unchanged; Windows behavior is still unverified here.

Measured production source/config additions: **398 physical lines**, including comments/blanks, no production deletions. This conservatively counts all new/materially changed code once, below the 450–850 estimate. Unchanged relocation: **0**. Tests: **438 added physical lines**. Generated Cargo lockfile bookkeeping: **199 additions / 37 removals**, reported separately; package IDs gain Nostr version disambiguation and the public fork pin. Documentation count is reported in the final handoff.

Self-review corrected the registration state mapping (`paired` → `registered`), the Windows lock location, the generic runtime command signature, and signature-versus-signout locking before handoff. This self-review is not the independent merge gate.

All invoked Cargo/pnpm/test processes reached terminal status. One Cargo build at a time, at most two jobs. No detached servers or test fixture processes remain. Git was initially clean; all current changes are owned by this task. No pre-existing edits were staged, reverted or cleaned. No push, PR, merge, deployment, service operation or taskctl mutation.

Source/tests commit: `c3056e099b0fbcdaf8f8b931cec85ddb7c1bd934`, signed off and attributed. Final receipt follows in a documentation-only commit. Documentation totals **143 physical added lines** across this receipt and the review brief. Final diff whitespace check passed; source commit left a clean worktree. The conductor receives the final documentation revision and process-terminal confirmation for exclusive handoff to independent review.

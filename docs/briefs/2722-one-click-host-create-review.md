# Independent review of registered-host Create (#2722)

## Checkout

The app implementation spans `038601cb236b491cbe6ef764e4920c3d3667430c..04489125d257b92f3960e69bf134477a83e45a4c` on `feat/2722-one-host-provider` in `buzz-app-wt-m4-one-host`. The app review correction starts at `f8505ddb291151f4a6edf10e52a622a4efe44575`. The checkout has its pre-existing untracked `node_modules` symlink.

The app's existing Cargo override resolves `buzz-paired-owner` in `buzz-wt-m4-paired-owner`. Review the transaction change at `4ab499573ea6d0cfdfe218a7149b4ec6cfa41a44..0d03b28a3c537409f8a86a30cd1389096ddeb13d` on `feat/2722-paired-owner-main`.

## Behavior

In the ordinary desktop Create form, Run on selects a registered host. The user chooses an installed harness and a model returned by the host. Create provisions the identity and passes that model to the guarded native deployment. The provider rechecks an aged selection on the same click before it reserves an identity. Discovery is finite and user-driven.

## Review focus

Check binding and freshness through the actual provider and IPC entry points. Give the native bridge particular attention because this machine could compile its owner dependency only.

1. Inspect `AgentCreateDialog.tsx:251` and `registeredHostType.tsx:103`. Rapid host and harness changes must retire older requests. A result arriving after an account change or dialog dismissal must have no effect. Revisit the host directory scope and the scoped invoke guard together. A relay reconnect preserves an unrelated local or cloud draft.
2. Inspect `registeredHostType.tsx:78`. An aged choice is checked using its original host, generation, runtime and catalog revision. If the reported model disappears, provisioning must stay untouched. The form and provider cases share this failure through the submit button.
3. Inspect `registered_hosts.rs:124` and `registered_hosts/provider.rs:824`. The FI-authorized socket, guarded signer and owner library verify the challenge and response. The local journal lock is released before independent model reads wait for the host.
4. Inspect `registered_hosts/provider.rs:457`. Check the retained attestation and placement, current signed status, selected model in both the first command and its recovery snapshot, and the applied-state check before listing.
5. Inspect `registered_hosts/provider.rs:558` and `service.ts:3395`. Exact pending intent replay and setup continuation have different owner admission conditions. An admitted continuation requires the owner's current operation and model evidence. The provider checks replacement drafts before the service updates its unfinished record. Retry uses the retained pubkey and provisioned picture. The owner links the authenticated reservation and deployment in one SQLite transaction. Reopening before dispatch must replay the signed command and advance only the missing phases.
6. Run the native IPC wrong-owner test and the two lifecycle cases sharing `registered_hosts/tests.rs:532` on a supported desktop. The lifecycle fixture opens the actual deployment command as the host and checks the model, agent identity and operation count.

## Decisions

The provider-specific validation callback protects an unfinished placement while retaining the service's normal exception and retry behavior. A partial request missing a model cannot start a new model-free deployment. A replacement request uses the same name and placement. A stale or unresumable protected intent surfaces its error and retains the identity. The host model wire supplies model IDs, names and descriptions. The form offers no inferred thinking settings.

## Measured gates

- After the review corrections, four complete Vitest files passed with 730 tests passing and one existing conditional test skipped. The files cover AgentCreateDialog, AgentsPage, the registered-host adapter and AgentsService. The skipped test requires `BUZZ_AGENT_COMMAND_BINARY`.
- The focused subset passed 22 tests. It covered reverse-order replies, expiry on submit, read-only cancellation and retained-identity retry.
- `bin/pnpm exec tsc --noEmit`, scoped Biome checking, `bin/cargo fmt --all -- --check` and `git diff --check` passed. Ordinary commit hooks passed literal names, icons, Biome and rustfmt.
- `bin/cargo check -p buzz-paired-owner --locked --offline -j 2` passed from the app workspace.
- `bin/cargo test -p Buzz --lib --locked --offline -j 2 relay::registered_hosts::tests` exited 101 in glib-sys before compiling the app. The machine lacks GLib, GTK 3 and WebKitGTK 4.1 development metadata.
- `bin/cargo test -p buzz-paired-owner --locked --offline` passed all 19 owner tests. Both the initial deployment and its continuation were reopened from disk and sent once through signed loopback dispatch. Bypassing the atomic link caused the same test to fail on the first reopen with the reservation still Reserved. A legacy unlinked intent test also checks refusal of a competing operation and repair by exact replay.
- `bin/cargo clippy -p buzz-paired-owner --all-targets --locked --offline --no-deps` and owner rustfmt passed. Strict Clippy across dependencies stopped on two existing warnings in `buzz-core/src/paired_message/body.rs`.
- The local-draft reconnect and resumed-avatar tests both failed against the previous app revision and passed with the fixes. A separate test changes relay scope with a model request in flight, checks cancellation and confirms the typed name survives.

## Remaining verification

The React tests exercised keyboard and pointer interactions with the shared Dialog, PreferenceRow, Select and ModelFamilyMenu under jsdom. Full desktop compilation, desktop lifecycle and loopback IPC execution, rendered themes and sizes, and an actual registered-host Create remain open. On a desktop runner with the platform libraries, run the registered-host native tests against the owner revision above. The fixture interrupts the first deployment at phase 0 and its continuation at phase 1, reopens each journal, and checks the exact signed event, host effect count and published profile count. A live account and provider were outside this assignment.

## Handoff

The independent reviewer owns the next gate on this committed checkout. Agent Settings/Edit and publication of the owner dependency are separate assignments.

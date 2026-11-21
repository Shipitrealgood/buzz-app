# Independent review of registered-host Create (#2722)

## Checkout

Review `038601cb236b491cbe6ef764e4920c3d3667430c..722bbef3e01dff3bb2e7e8bd57c148a94b519e98` on `feat/2722-one-host-provider`. The working checkout is `buzz-app-wt-m4-one-host`. The baseline and final worktree both have an untracked `node_modules` symlink. The parked Cargo override resolves the owner library from the separately pinned checkout at `4ab499573`.

## Behavior

In the ordinary desktop Create form, Run on selects a registered host. The user chooses an installed harness and a model returned by the host. Create provisions the identity and passes that model to the guarded native deployment. The provider rechecks an aged selection on the same click before it reserves an identity. Discovery is finite and user-driven.

## Review focus

Check binding and freshness through the actual provider and IPC entry points. Give the native bridge particular attention because this machine could compile its owner dependency only.

1. Inspect `AgentCreateDialog.tsx:251` and `registeredHostType.tsx:103`. Rapid host and harness changes must retire older requests. A result arriving after an account change or dialog dismissal must have no effect. Revisit the desktop generation key and the scoped invoke guard together.
2. Inspect `registeredHostType.tsx:78`. An aged choice is checked using its original host, generation, runtime and catalog revision. If the reported model disappears, provisioning must stay untouched. The form and provider cases share this failure through the submit button.
3. Inspect `registered_hosts.rs:124` and `registered_hosts/provider.rs:824`. The FI-authorized socket, guarded signer and owner library verify the challenge and response. The local journal lock is released before independent model reads wait for the host.
4. Inspect `registered_hosts/provider.rs:457`. Check the retained attestation and placement, current signed status, selected model in both the first command and its recovery snapshot, and the applied-state check before listing.
5. Inspect `registered_hosts/provider.rs:558` and `service.ts:3395`. Exact pending intent replay and setup continuation have different owner admission conditions. An admitted continuation requires the owner's current operation and model evidence. The provider checks replacement drafts before the service updates its unfinished record. Retry uses the retained pubkey.
6. Run the native IPC wrong-owner test and the two lifecycle cases sharing `registered_hosts/tests.rs:532` on a supported desktop. The lifecycle fixture opens the actual deployment command as the host and checks the model, agent identity and operation count.

## Decisions

The provider-specific validation callback protects an unfinished placement while retaining the service's normal exception and retry behavior. A partial request missing a model cannot start a new model-free deployment. A replacement request uses the same name and placement. A stale or unresumable protected intent surfaces its error and retains the identity. The host model wire supplies model IDs, names and descriptions. The form offers no inferred thinking settings.

## Measured gates

- Four complete Vitest files passed with 729 tests passing and one existing conditional test skipped. The files cover AgentCreateDialog, AgentsPage, the registered-host adapter and AgentsService. The skipped test requires `BUZZ_AGENT_COMMAND_BINARY`.
- The focused subset passed 22 tests. It covered reverse-order replies, expiry on submit, read-only cancellation and retained-identity retry.
- `bin/pnpm exec tsc --noEmit`, scoped Biome checking, `bin/cargo fmt --all -- --check` and `git diff --check` passed. Ordinary commit hooks passed literal names, icons, Biome and rustfmt.
- `bin/cargo check -p buzz-paired-owner --locked --offline -j 2` passed from the app workspace.
- `bin/cargo test -p Buzz --lib --locked --offline -j 2 relay::registered_hosts::tests` exited 101 in glib-sys before compiling the app. The machine lacks GLib, GTK 3 and WebKitGTK 4.1 development metadata.

## Remaining verification

The React tests exercised keyboard and pointer interactions with the shared Dialog, PreferenceRow, Select and ModelFamilyMenu under jsdom. Native compilation, loopback IPC execution, rendered themes and sizes, and an actual registered-host Create remain open. A live account and provider were outside this assignment.

## Handoff

The independent reviewer owns the next gate on this committed checkout. Agent Settings/Edit and publication of the owner dependency are separate assignments.

# Independent review: native registered-host recovery (#2722)

Requested: 2026-11-11. Next owner: independently dispatched reviewer.

## Where the work lives

- Workspace: `buzz-app-review-20261111`.
- Branch: `feat/2722-native-host-recovery`, not main.
- Base: `128cd4c2f38801d5f773d5d5ec95a1ea5594b374`.
- Review base through the source/evidence commits supplied by the conductor.
- Receipt: [execution and gates](../executions/2722-native-host-recovery.md).
- Governing scope: conductor-supplied checkpoint 1 of the Buzz App provider callback map; accepted reuse audit remains input.

```sh
git diff 128cd4c2f38801d5f773d5d5ec95a1ea5594b374..HEAD
```

## What landed

A purpose-bound native read recovers registrations through the existing shared owner registry and journal. The app keeps keys native, uses existing federated admission, isolates cached records by owner/community, and exposes only public host coordinates/registration and an unavailable runtime status. A typed adapter rejects responses from retired caller scopes. There is no UI caller, provisioning, agent inventory write or live rollout.

The public pinned crates are source-identical to the audited local reference for all four transitive in-repository owners and root manifest. Cargo resolves and the actual owner crate compiles. Frontend tests/types pass; native app compilation and tests stop before app code at missing GLib. Treat every native bridge behavior claim as read/inferred pending its real gate.

## Your job

Review the committed source and its necessity, security and recovery behavior. Challenge the uncompiled native integration carefully. Do not turn source review or dependency compilation into native/Windows acceptance.

## Specifically look for

1. **Identity guard — `src-tauri/src/identity.rs:837` and `src-tauri/src/relay/registered_hosts.rs:84`.** Does the ready-key closure remain internal and use the original identity mutex? NIP-42 signing runs inside it; registry decrypt keys are native temporary values returned to the existing owner API. Can any awaited path return another account's data after close?

2. **Connection/admission — `registered_hosts.rs:114`.** Does bounded `relay_info` discovery plus existing FI authorization correctly cover the shared WebSocket upgrade? Header/signout/session lifetime behavior is not exercised by the unrun loopback test. Inspect error conversion and the cancellation wrapper; distinguish unavailable registry from unsupported capability.

3. **Read-only ownership — `registered_hosts.rs:158`.** Recovery invokes only `recover_guarded`; the nearby shared `synchronize_guarded` would resend mutations and must remain unused. Check that no pending pair/unpair/provider action or local identity materialization is introduced.

4. **Journal path/lock — `registered_hosts.rs:188`.** Does every path stay app-local? The non-secret lock lives beside the protected directory because the shared Windows ACL script rejects extra files inside. Check reopen, lock acquisition/release and permissions. No old journal was opened or migrated; Windows ACL execution is not validated.

5. **Projection/scope — `registered_hosts.rs:38` and `src/features/agents/registered-hosts.ts:33`.** Registered means verified dual consent, not available runtime. Every card still says availability unavailable, meaning no current readiness evidence. The typed caller must supply a fresh scope object for every session generation, including A → B → A; no UI caller exists yet. Does the API communicate that requirement adequately for later integration?

6. **Native fixture — `registered_hosts/tests.rs:111`.** The loopback server uses actual signed encrypted registry replies and checks that only AUTH/REQ/CLOSE traffic occurs, then tests reopen and scoped absence. It is authored but unrun. Probe compile/API correctness and whether its assertions match the stated behavior rather than granting validation by inspection.

7. **IPC wiring — `src-tauri/src/lib.rs:774`, native build manifest and default capability.** The command follows generic runtime registration and has a wrong-owner mock IPC test. The existing command-manifest test must pass on a supported native environment. No generic signing allowlist changed.

## Spec deviations and decisions

- Native per-call community semantics follow upstream. No global native selected-community owner was invented; future UI owns its caller scope generation.
- Public revision `78232d95173b876babcee0f51b18f7c2a17dac0d` replaces the unresolvable local-only reference as a dependency, after exact scoped source parity and public ref verification.
- The helper is nested under existing `relay` to reuse HTTP discovery. No shared abstraction was extracted.
- No copied production harness was introduced to bypass missing desktop libraries. Native tests remain unrun and acceptance is conditional.
- Production additions are below the forecast; nothing was omitted to meet the budget.

## What not to chase

Provision/activate/profile/delete callbacks, host-first UI, new pairing/revoke, polling, installer, login broker, hostd, roothelper and adoption remain later work. Existing Claude/Grok/Scout and the retained pilot agent are untouched. Shared response-freshness diagnostics remain with their existing owner. No Windows/local Claude implementation or broader framework is needed for this review.

## Deliverables and next gates

Return independent findings and exact reviewed revision to the conductor. Name source blockers separately from unavailable environment gates. The conductor owns any follow-on scope ruling, board updates and merge authority.

- Check the feature branch and clean committed state before taking write/test ownership.
- Read the receipt's failed native command and still-required commands.
- Preserve the public dependency pin and original owner sources.
- Do not open any real account/journal, contact a live relay/provider or launch a desktop profile.
- No source writer/build process remains from execution.

Requested executor configuration was Astra/high; runtime routing was not independently exposed. Self-review is only implementation evidence, not an independent gate.

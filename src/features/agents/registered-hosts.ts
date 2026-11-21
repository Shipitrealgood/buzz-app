import { invoke } from "@tauri-apps/api/core";
import type { NipOaAuthorization } from "../host/service";

export type RegisteredHostScope = Readonly<{
  owner: string;
  community: string;
}>;

export type RegisteredHost = RegisteredHostScope &
  Readonly<{
    host: string;
    generation: string;
    label: string | null;
    registration:
      | "registered"
      | "unpair-pending"
      | "unpaired"
      | "pair-pending"
      | "pair-expired";
    /** Recovery verifies registration, never runtime liveness or readiness. */
    availability: "unavailable";
  }>;

export type RegisteredHostDirectory = RegisteredHostScope &
  Readonly<{
    registry: "available" | "unavailable" | "unsupported";
    hosts: readonly RegisteredHost[];
  }>;

export type HostRuntimeAvailability =
  | "available"
  | "adapter_missing"
  | "adapter_outdated"
  | "cli_missing"
  | "not_installed";
export type HostReadiness =
  | "logged-in"
  | "logged-out"
  | "not-applicable"
  | "unknown"
  | "config-invalid"
  | "unsupported";
export type RegisteredHostRuntime = Readonly<{
  runtimeId: string;
  harnessId: string | null;
  availability: HostRuntimeAvailability | null;
  version: string | null;
  configSupported: boolean;
}>;
export type RegisteredHostStatus = Readonly<{
  availability: "unknown" | "current" | "stale" | "unavailable" | "revoked";
  catalogRevision: string | null;
  expiresAt: number;
  runtimes: readonly RegisteredHostRuntime[];
}>;
export type RegisteredHostModel = Readonly<{
  id: string;
  name: string | null;
  description: string | null;
}>;

export type RegisteredHostModels = Readonly<{
  expiresAt: number;
  readiness: HostReadiness;
  availability: HostRuntimeAvailability;
  models:
    | {
        state: "available";
        catalog: { models: readonly RegisteredHostModel[] };
      }
    | { state: "empty" | "unsupported" }
    | { state: "unavailable"; reason: string };
}>;

async function scopedInvoke<T>(
  currentScope: () => RegisteredHostScope | undefined,
  signal: AbortSignal,
  command: string,
  payload: (scope: RegisteredHostScope) => Record<string, unknown>,
  verify?: (result: T, scope: RegisteredHostScope) => void,
): Promise<T> {
  signal.throwIfAborted();
  const scope = currentScope();
  if (!scope) throw new Error("Select a community and identity first");
  const { owner, community } = scope;
  const check = () => {
    signal.throwIfAborted();
    if (
      currentScope() !== scope ||
      scope.owner !== owner ||
      scope.community !== community
    )
      throw new Error("Registered host recovery scope changed");
  };
  let result: T;
  try {
    result = await invoke<T>(command, payload(scope));
  } catch (error) {
    check();
    throw typeof error === "string" ? new Error(error) : error;
  }
  check();
  verify?.(result, scope);
  return result;
}

const defined = (obj: Record<string, unknown>) =>
  Object.fromEntries(Object.entries(obj).filter(([, v]) => v !== undefined));

/** Native-only, finite read/control adapter for registered hosts. The caller
 * supplies a session scope object whose identity changes on account/community
 * transitions, including A → B → A. No secret key or raw signing crosses IPC. */
export function nativeRegisteredHosts(
  currentScope: () => RegisteredHostScope | undefined,
) {
  const run = <T>(
    signal: AbortSignal,
    cmd: string,
    payload: (scope: RegisteredHostScope) => Record<string, unknown>,
    verify?: (result: T, scope: RegisteredHostScope) => void,
  ) => scopedInvoke<T>(currentScope, signal, cmd, payload, verify);
  const inputRun = <T>(
    signal: AbortSignal,
    cmd: string,
    extra: Record<string, unknown>,
  ) => run<T>(signal, cmd, (s) => ({ input: defined({ ...s, ...extra }) }));
  return {
    recover(signal: AbortSignal): Promise<RegisteredHostDirectory> {
      return run<RegisteredHostDirectory>(
        signal,
        "registered_host_recover",
        ({ owner, community }) => ({ owner, community }),
        (r, { owner, community }) => {
          if (
            r.owner !== owner ||
            r.community !== community ||
            r.hosts.some((h) => h.owner !== owner || h.community !== community)
          )
            throw new Error("Registered host recovery returned another scope");
        },
      );
    },
    status: (
      input: Readonly<{ host: string; generation: string }>,
      signal: AbortSignal,
    ): Promise<RegisteredHostStatus> =>
      run(signal, "registered_host_status", (s) => ({ ...s, ...input })),
    models: (
      input: Readonly<{
        host: string;
        generation: string;
        runtimeId: string;
        catalogRevision: string;
      }>,
      signal: AbortSignal,
    ): Promise<RegisteredHostModels> =>
      inputRun(signal, "registered_host_models", input),
    async provision(
      input: Readonly<{
        host: string;
        generation: string;
        runtimeId: string;
        name: string;
        picture?: string;
        instructions?: string;
        effort?: string;
      }>,
      signal: AbortSignal,
    ): Promise<Readonly<{ pubkey: string; picture?: string }>> {
      const r = await inputRun<
        Readonly<{ pubkey: string; picture?: string | null }>
      >(signal, "registered_host_provision", input);
      return {
        pubkey: r.pubkey,
        ...(r.picture != null ? { picture: r.picture } : {}),
      };
    },
    activate: (
      input: Readonly<{
        pubkey: string;
        name: string;
        nipOa: NipOaAuthorization;
        host?: string;
        generation?: string;
        runtimeId?: string;
        modelId?: string;
        catalogRevision?: string;
        effort?: string;
      }>,
      signal: AbortSignal,
    ): Promise<void> => inputRun(signal, "registered_host_activate", input),
    profile: (
      input: Readonly<{ pubkey: string; name: string; picture?: string }>,
      signal: AbortSignal,
    ): Promise<void> => inputRun(signal, "registered_host_profile", input),
    remove: (
      input: Readonly<{ pubkey: string }>,
      signal: AbortSignal,
    ): Promise<void> =>
      run(signal, "registered_host_remove", (s) => ({ ...s, ...input })),
  };
}

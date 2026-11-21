import { invoke } from "@tauri-apps/api/core";

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

/** Native-only, finite read adapter for future host UI. The caller supplies a
 * session scope object whose identity changes on account/community transitions,
 * including A → B → A. No subscription, polling, inventory write or key IPC.
 * A closed view aborts its signal; native can finish the scoped journal read,
 * but its completion cannot be adopted by another view/session. */
export function nativeRegisteredHosts(
  currentScope: () => RegisteredHostScope | undefined,
) {
  return {
    async recover(signal: AbortSignal): Promise<RegisteredHostDirectory> {
      signal.throwIfAborted();
      const scope = currentScope();
      if (!scope) throw new Error("Select a community and identity first");
      const { owner, community } = scope;
      const current = () => {
        signal.throwIfAborted();
        if (
          currentScope() !== scope ||
          scope.owner !== owner ||
          scope.community !== community
        )
          throw new Error("Registered host recovery scope changed");
      };
      let result: RegisteredHostDirectory;
      try {
        result = await invoke<RegisteredHostDirectory>(
          "registered_host_recover",
          { owner, community },
        );
      } catch (error) {
        current();
        throw typeof error === "string" ? new Error(error) : error;
      }
      current();
      if (
        result.owner !== owner ||
        result.community !== community ||
        result.hosts.some(
          (host) => host.owner !== owner || host.community !== community,
        )
      )
        throw new Error("Registered host recovery returned another scope");
      return result;
    },
  };
}

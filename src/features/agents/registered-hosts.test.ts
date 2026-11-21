import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  nativeRegisteredHosts,
  type RegisteredHostDirectory,
  type RegisteredHostScope,
} from "./registered-hosts";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const original = { owner: "ab".repeat(32), community: "https://example.org" };
const directory = (): RegisteredHostDirectory => ({
  ...original,
  registry: "available",
  hosts: [
    {
      ...original,
      host: "cd".repeat(32),
      generation: "ef".repeat(32),
      label: "Example host",
      registration: "registered",
      availability: "unavailable",
    },
  ],
});

beforeEach(() => {
  call.mockReset();
});

describe("native registered host recovery", () => {
  it("uses only the purpose-bound native read and preserves unavailable registration", async () => {
    call.mockResolvedValue(directory());
    const adapter = nativeRegisteredHosts(() => original);
    const result = await adapter.recover(new AbortController().signal);
    expect(call).toHaveBeenCalledExactlyOnceWith(
      "registered_host_recover",
      original,
    );
    expect(result.hosts[0]).toMatchObject({
      registration: "registered",
      availability: "unavailable",
    });
    expect(result.hosts[0]).not.toHaveProperty("secret");
  });

  it.each(["unavailable", "unsupported"] as const)(
    "preserves %s without inventing an empty registry",
    async (registry) => {
      const result = { ...directory(), registry };
      call.mockResolvedValue(result);
      await expect(
        nativeRegisteredHosts(() => original).recover(
          new AbortController().signal,
        ),
      ).resolves.toEqual(result);
    },
  );

  it.each(["account", "community", "return", "logout", "abort"])(
    "rejects a late %s result",
    async (transition) => {
      let scope: RegisteredHostScope | undefined = original;
      let resolve!: (value: RegisteredHostDirectory) => void;
      call.mockImplementation(
        () =>
          new Promise((done) => {
            resolve = done;
          }),
      );
      const controller = new AbortController();
      const result = nativeRegisteredHosts(() => scope).recover(
        controller.signal,
      );
      const rejected = expect(result).rejects.toThrow();
      expect(call).toHaveBeenCalledOnce();
      if (transition === "abort") controller.abort();
      else if (transition === "logout") scope = undefined;
      else if (transition === "account")
        scope = { ...original, owner: "cd".repeat(32) };
      else if (transition === "community")
        scope = { ...original, community: "https://other.example.org" };
      else scope = { ...original }; // A → B → A creates a new session scope.
      resolve(directory());
      await rejected;
    },
  );

  it("fences errors as well as successful replies", async () => {
    let scope: RegisteredHostScope | undefined = original;
    let reject!: (error: string) => void;
    call.mockImplementation(
      () =>
        new Promise((_, fail) => {
          reject = fail;
        }),
    );
    const result = nativeRegisteredHosts(() => scope).recover(
      new AbortController().signal,
    );
    const rejected = expect(result).rejects.toThrow("scope changed");
    scope = undefined;
    reject("Old account failure");
    await rejected;
  });

  it.each(["directory", "host"])(
    "refuses a mismatched %s scope",
    async (target) => {
      const result = directory();
      call.mockResolvedValue(
        target === "directory"
          ? { ...result, owner: "cd".repeat(32) }
          : {
              ...result,
              hosts: [
                { ...result.hosts[0], community: "https://other.example.org" },
              ],
            },
      );
      await expect(
        nativeRegisteredHosts(() => original).recover(
          new AbortController().signal,
        ),
      ).rejects.toThrow("another scope");
    },
  );

  it("normalizes native errors and refuses calls without scope", async () => {
    call.mockRejectedValue("Recovery unavailable");
    await expect(
      nativeRegisteredHosts(() => original).recover(
        new AbortController().signal,
      ),
    ).rejects.toThrow("Recovery unavailable");
    call.mockClear();
    await expect(
      nativeRegisteredHosts(() => undefined).recover(
        new AbortController().signal,
      ),
    ).rejects.toThrow("Select a community");
    expect(call).not.toHaveBeenCalled();
  });

  it("invokes all 6 host provider commands within the current scope", async () => {
    const host = "cd".repeat(32);
    const generation = "ef".repeat(32);
    const pubkey = "12".repeat(32);
    const adapter = nativeRegisteredHosts(() => original);
    const signal = new AbortController().signal;

    call.mockResolvedValueOnce({
      availability: "current",
      catalogRevision: "aa".repeat(32),
      expiresAt: 9999999999,
      runtimes: [
        {
          runtimeId: "acp/claude",
          harnessId: "claude",
          availability: "available",
          version: "1.0.0",
          configSupported: true,
        },
      ],
    });
    const status = await adapter.status({ host, generation }, signal);
    expect(status.runtimes[0]?.harnessId).toBe("claude");
    expect(call).toHaveBeenLastCalledWith("registered_host_status", {
      ...original,
      host,
      generation,
    });

    call.mockResolvedValueOnce({
      pubkey,
      picture: "https://example.test/a.png",
    });
    await adapter.provision(
      {
        host,
        generation,
        runtimeId: "acp/claude",
        name: "Alpha",
        picture: "https://example.test/a.png",
        instructions: "Be concise",
      },
      signal,
    );
    expect(call).toHaveBeenLastCalledWith("registered_host_provision", {
      input: {
        ...original,
        host,
        generation,
        runtimeId: "acp/claude",
        name: "Alpha",
        picture: "https://example.test/a.png",
        instructions: "Be concise",
      },
    });

    const modelQuery = {
      host,
      generation,
      runtimeId: "acp/claude",
      catalogRevision: "aa".repeat(32),
    };
    await adapter.models(modelQuery, signal);
    expect(call).toHaveBeenLastCalledWith("registered_host_models", {
      input: { ...original, ...modelQuery },
    });
    const activation = {
      pubkey,
      name: "Alpha",
      nipOa: ["auth", original.owner, "", "00".repeat(64)] as const,
      ...modelQuery,
      modelId: "claude-sonnet",
    };
    await adapter.activate(activation, signal);
    expect(call).toHaveBeenLastCalledWith("registered_host_activate", {
      input: { ...original, ...activation },
    });

    call.mockResolvedValueOnce(undefined);
    await adapter.profile(
      { pubkey, name: "Alpha 2", picture: "https://example.test/b.png" },
      signal,
    );
    expect(call).toHaveBeenLastCalledWith("registered_host_profile", {
      input: {
        ...original,
        pubkey,
        name: "Alpha 2",
        picture: "https://example.test/b.png",
      },
    });

    call.mockResolvedValueOnce(undefined);
    await adapter.remove({ pubkey }, signal);
    expect(call).toHaveBeenLastCalledWith("registered_host_remove", {
      ...original,
      pubkey,
    });
  });

  it("fences a late model reply across account replacement", async () => {
    let scope: RegisteredHostScope | undefined = original;
    let resolve!: (value: unknown) => void;
    call.mockImplementation(
      () =>
        new Promise((done) => {
          resolve = done;
        }),
    );
    const adapter = nativeRegisteredHosts(() => scope);
    const p = adapter.models(
      {
        host: "cd".repeat(32),
        generation: "ef".repeat(32),
        runtimeId: "acp/claude",
        catalogRevision: "aa".repeat(32),
      },
      new AbortController().signal,
    );
    const rejected = expect(p).rejects.toThrow("scope changed");
    scope = { ...original, owner: "cd".repeat(32) };
    resolve({
      availability: "current",
      catalogRevision: null,
      expiresAt: 0,
      runtimes: [],
    });
    await rejected;
  });

  it("omits null or missing picture from provision replies", async () => {
    const adapter = nativeRegisteredHosts(() => original);
    const signal = new AbortController().signal;
    const base = {
      host: "cd".repeat(32),
      generation: "ef".repeat(32),
      runtimeId: "acp/claude",
      name: "Alpha",
    };
    call.mockResolvedValueOnce({ pubkey: "11".repeat(32), picture: null });
    const withNull = await adapter.provision(base, signal);
    expect(withNull).toEqual({ pubkey: "11".repeat(32) });
    expect("picture" in withNull).toBe(false);

    call.mockResolvedValueOnce({ pubkey: "22".repeat(32) });
    const withOmitted = await adapter.provision(base, signal);
    expect(withOmitted).toEqual({ pubkey: "22".repeat(32) });
    expect("picture" in withOmitted).toBe(false);
  });
});

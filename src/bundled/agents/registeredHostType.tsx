import { useEffect, useLayoutEffect, useState, type ReactNode } from "react";
import { AgentInstructions } from "../../features/agents/AgentInstructions";
import { ModelFamilyMenu } from "../../features/agents/ModelFamilyMenu";
import type {
  AgentCreateProps,
  AgentType,
  AgentViewProps,
} from "../../features/agents/service";
import type {
  RegisteredHostModels,
  RegisteredHostRuntime,
  RegisteredHostStatus,
  nativeRegisteredHosts,
} from "../../features/agents/registered-hosts";
import { CpuIcon } from "../../shared/design-system/icons";
import { Button } from "../../shared/design-system/ui/Button";
import { PreferenceRow } from "../../shared/design-system/ui/PreferenceRow";
import { Select } from "../../shared/design-system/ui/Select";
import { SettingsGroup } from "../../shared/design-system/ui/SettingsGroup";

export const REGISTERED_HOST_TYPE_KEY = "buzz.agents/registered-host";
export type RegisteredHostsClient = ReturnType<typeof nativeRegisteredHosts>;
export type HostedAgentConfig = Readonly<{
  host: string;
  generation: string;
  hostLabel?: string;
  runtimeId: string;
  harnessId?: string;
  instructions: string;
  modelId: string;
  catalogRevision?: string;
  modelsExpireAt?: number;
}>;
export const EMPTY_HOSTED_CONFIG: HostedAgentConfig = {
  host: "",
  generation: "",
  runtimeId: "",
  instructions: "",
  modelId: "",
};

const runtimeTitle = (
  rt: Pick<RegisteredHostRuntime, "runtimeId" | "harnessId">,
) => {
  const id = rt.harnessId ?? rt.runtimeId.split("/").pop() ?? rt.runtimeId;
  return id === "buzz-agent"
    ? "Buzz agent"
    : id[0]
      ? `${id[0].toUpperCase()}${id.slice(1)}`
      : id;
};
const errMsg = (r: unknown, fb: string) =>
  r instanceof Error ? r.message : fb;

function readinessMessage(obs: RegisteredHostModels | undefined): string {
  if (!obs) return "";
  if (obs.availability !== "available")
    return `Host runtime is currently ${obs.availability.replaceAll("_", " ")}.`;
  if (obs.readiness === "logged-out")
    return "Sign in to this harness on the host computer, then choose Refresh.";
  if (obs.readiness !== "logged-in" && obs.readiness !== "not-applicable")
    return "Could not verify host readiness. Check the host setup and choose Refresh.";
  switch (obs.models.state) {
    case "available":
      return "";
    case "empty":
      return "No models were reported on this host.";
    case "unsupported":
      return "This host harness does not support model discovery.";
    case "unavailable":
      return `Host models are unavailable (${obs.models.reason}).`;
  }
}

export function createRegisteredHostType(
  hosts: RegisteredHostsClient,
): AgentType<HostedAgentConfig> {
  async function currentChoice(value: unknown, signal: AbortSignal) {
    const cfg = value as HostedAgentConfig | undefined;
    if (!cfg?.modelId || !cfg.catalogRevision || !cfg.modelsExpireAt)
      throw new Error("Choose a model on the host before creating the agent.");
    if (cfg.modelsExpireAt * 1000 <= Date.now()) {
      const { host, generation, runtimeId, catalogRevision } = cfg;
      const result = await hosts.models(
        { host, generation, runtimeId, catalogRevision },
        signal,
      );
      const problem = readinessMessage(result);
      if (problem) throw new Error(problem);
      if (
        result.expiresAt * 1000 <= Date.now() ||
        result.models.state !== "available" ||
        !result.models.catalog.models.some((m) => m.id === cfg.modelId)
      )
        throw new Error(
          "The selected model is no longer available. Refresh and choose a model.",
        );
    }
    signal.throwIfAborted();
    return cfg;
  }

  function CreateConfiguration({
    config,
    onChange,
    disabled,
    harnessControl,
    onReadyChange,
  }: AgentCreateProps<HostedAgentConfig> & {
    harnessControl: ReactNode;
    onReadyChange?: (ready: boolean) => void;
  }) {
    const cfg = config ?? EMPTY_HOSTED_CONFIG;
    const [status, setStatus] = useState<RegisteredHostStatus>();
    const [observation, setObservation] = useState<RegisteredHostModels>();
    const [error, setError] = useState("");
    const [clock, setClock] = useState(Date.now);
    useEffect(() => {
      if (status || error) return;
      const ctrl = new AbortController();
      if (cfg.host && cfg.generation) {
        void hosts
          .status({ host: cfg.host, generation: cfg.generation }, ctrl.signal)
          .then(
            (result) => {
              if (!ctrl.signal.aborted) setStatus(result);
            },
            (reason) => {
              if (!ctrl.signal.aborted)
                setError(errMsg(reason, "Could not load host harnesses."));
            },
          );
      }
      return () => ctrl.abort();
    }, [cfg.host, cfg.generation, status, error]);

    const runtime = status?.runtimes.find((r) => r.runtimeId === cfg.runtimeId);
    const revision =
      status?.availability === "current" &&
      runtime?.configSupported &&
      runtime.availability === "available"
        ? status.catalogRevision
        : null;
    useEffect(() => {
      const ctrl = new AbortController();
      setObservation(undefined);
      setError("");
      if (revision && status && status.expiresAt * 1000 > Date.now()) {
        void hosts
          .models(
            {
              host: cfg.host,
              generation: cfg.generation,
              runtimeId: cfg.runtimeId,
              catalogRevision: revision,
            },
            ctrl.signal,
          )
          .then(
            (result) => {
              if (!ctrl.signal.aborted) setObservation(result);
            },
            (reason) => {
              if (!ctrl.signal.aborted)
                setError(errMsg(reason, "Could not load host models."));
            },
          );
      }
      return () => ctrl.abort();
    }, [cfg.host, cfg.generation, cfg.runtimeId, revision, status]);

    const expiresAt = Math.min(
      status?.expiresAt ?? 0,
      observation?.expiresAt ?? Infinity,
    );
    useEffect(() => {
      if (!expiresAt) return;
      const timer = window.setTimeout(
        () => setClock(Date.now()),
        Math.max(0, expiresAt * 1000 - Date.now()),
      );
      return () => window.clearTimeout(timer);
    }, [expiresAt]);
    const expired = Boolean(
      expiresAt && expiresAt * 1000 <= Math.max(clock, Date.now()),
    );
    const readiness = readinessMessage(observation);
    const models =
      revision &&
      !error &&
      !readiness &&
      observation?.models.state === "available"
        ? observation.models.catalog.models
        : [];
    const chosenModel = models.find((m) => m.id === cfg.modelId);
    const note =
      error ||
      (expired
        ? chosenModel
          ? ""
          : "Host information expired. Refresh to load current models."
        : !status
          ? "Loading host harnesses…"
          : status.availability !== "current"
            ? `Host status is ${status.availability}.`
            : !status.runtimes.length
              ? "No harnesses were reported on this host."
              : revision && !observation
                ? "Loading host models…"
                : readiness);
    useLayoutEffect(() => {
      onReadyChange?.(Boolean(chosenModel));
    }, [onReadyChange, chosenModel]);
    return (
      <div className="grid gap-3">
        <SettingsGroup>
          {harnessControl}
          <PreferenceRow
            title="Harness"
            trailing={
              <Select
                label="Harness"
                variant="compact"
                align="end"
                value={cfg.runtimeId}
                placeholder={!status ? "Loading…" : "Choose a harness"}
                disabled={disabled || !status?.runtimes.length || expired}
                groups={[
                  {
                    label: "Host",
                    options: (status?.runtimes ?? []).map((r) => ({
                      value: r.runtimeId,
                      label: runtimeTitle(r),
                      disabled:
                        r.availability !== "available" || !r.configSupported,
                    })),
                  },
                ]}
                onValueChange={(runtimeId) => {
                  const found = status?.runtimes.find(
                    (r) => r.runtimeId === runtimeId,
                  );
                  const { harnessId: _, ...rest } = cfg;
                  setObservation(undefined);
                  onChange({
                    ...rest,
                    runtimeId,
                    ...(found?.harnessId ? { harnessId: found.harnessId } : {}),
                    modelId: "",
                  });
                }}
              />
            }
          />
          {cfg.runtimeId && (
            <ModelFamilyMenu
              label="Model"
              align="end"
              selected={{
                id: chosenModel?.id ?? "",
                label: chosenModel?.name ?? chosenModel?.id ?? "Choose a model",
              }}
              models={models.map((m) => ({
                id: m.id,
                label: m.name ?? m.id,
                ...(m.description ? { description: m.description } : {}),
              }))}
              disabled={disabled || expired || !models.length}
              onChange={(modelId) => {
                if (revision)
                  onChange({
                    ...cfg,
                    modelId,
                    catalogRevision: revision,
                    modelsExpireAt: expiresAt,
                  });
              }}
            />
          )}
        </SettingsGroup>
        {note && (
          <p
            role={error ? "alert" : "status"}
            className={`m-0 text-body-sm ${error ? "text-danger" : "text-secondary"}`}
          >
            {note}
          </p>
        )}
        {(error ||
          expired ||
          (status && note && (!revision || observation))) && (
          <div>
            <Button
              disabled={disabled}
              onClick={() => {
                setStatus(undefined);
                setObservation(undefined);
                onChange({ ...cfg, modelId: "" });
                setError("");
              }}
            >
              Refresh
            </Button>
          </div>
        )}
      </div>
    );
  }

  return {
    id: "registered-host",
    title: "Registered host",
    icon: CpuIcon,
    defaults: () => ({ config: EMPTY_HOSTED_CONFIG }),
    adopt: (carry, saved) => ({
      ...saved,
      instructions: carry.instructions ?? saved.instructions,
    }),
    carry: (saved) => ({ instructions: saved.instructions ?? "" }),
    summary: (agent) => {
      const cfg = agent.config;
      if (!cfg?.runtimeId) return cfg?.hostLabel || "Registered host";
      return `${runtimeTitle({ runtimeId: cfg.runtimeId, harnessId: cfg.harnessId ?? null })} · ${cfg.modelId || cfg.hostLabel || "Registered host"}`;
    },
    CreateConfiguration,
    Create: ({
      config,
      onChange,
      disabled,
    }: AgentCreateProps<HostedAgentConfig>) => {
      const cfg = config ?? EMPTY_HOSTED_CONFIG;
      return (
        <AgentInstructions
          creation
          value={cfg.instructions}
          disabled={disabled}
          onChange={(instructions) => onChange({ ...cfg, instructions })}
        />
      );
    },
    tabs: [
      {
        id: "settings",
        title: "Settings",
        role: "settings",
        splitSettings: true,
        component: ({
          agent,
          settingsSection,
          settingsContent,
        }: AgentViewProps<HostedAgentConfig>) => (
          <div className="grid gap-4">
            {settingsSection !== "runtime" && (
              <SettingsGroup>
                <PreferenceRow
                  title="Host"
                  trailing={
                    <span className="text-body-sm text-secondary">
                      {agent.config?.hostLabel || "Registered host"}
                    </span>
                  }
                />
              </SettingsGroup>
            )}
            {settingsSection !== "identity" && settingsContent}
          </div>
        ),
      },
    ],
    remote: {
      async validateRetry(saved, next, signal) {
        const retained = saved as HostedAgentConfig | undefined;
        const draft = await currentChoice(next, signal);
        if (
          !retained ||
          retained.host !== draft.host ||
          retained.generation !== draft.generation ||
          retained.runtimeId !== draft.runtimeId ||
          retained.instructions !== draft.instructions ||
          (retained.modelId && retained.modelId !== draft.modelId)
        )
          throw new Error(
            "This unfinished agent is bound to its original host, harness, model and instructions.",
          );
      },
      async provision({ name, picture, config, signal }) {
        const cfg = await currentChoice(config, signal);
        return hosts.provision(
          {
            host: cfg.host,
            generation: cfg.generation,
            runtimeId: cfg.runtimeId,
            name,
            ...(picture ? { picture } : {}),
            ...(cfg.instructions ? { instructions: cfg.instructions } : {}),
          },
          signal,
        );
      },
      async activate({ pubkey, name, auth, config, signal }) {
        const cfg = config as HostedAgentConfig | undefined;
        if (!cfg?.host || !cfg.generation || !cfg.runtimeId)
          throw new Error(
            "The saved host placement is incomplete. Remove the unfinished agent before creating it again.",
          );
        await hosts.activate(
          {
            pubkey,
            name,
            nipOa: auth,
            host: cfg.host,
            generation: cfg.generation,
            runtimeId: cfg.runtimeId,
            modelId: cfg.modelId,
            ...(cfg.catalogRevision
              ? { catalogRevision: cfg.catalogRevision }
              : {}),
          },
          signal,
        );
      },
      profile: ({ pubkey, name, picture, signal }) =>
        hosts.profile(
          { pubkey, name, ...(picture ? { picture } : {}) },
          signal,
        ),
      remove: ({ pubkey, signal }) => hosts.remove({ pubkey }, signal),
    },
  };
}

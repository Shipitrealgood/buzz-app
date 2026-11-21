import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AgentInstructions } from "../../features/agents/AgentInstructions";
import { ModelFamilyMenu } from "../../features/agents/ModelFamilyMenu";
import type {
  AgentCreateProps,
  AgentType,
  AgentViewProps,
} from "../../features/agents/service";
import type {
  RegisteredHostObservation,
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
  effort: string;
  pubkey?: string;
  frozen?: boolean;
  applied?: boolean;
  observation?: RegisteredHostObservation;
  statusMessage?: string;
}>;
export const EMPTY_HOSTED_CONFIG: HostedAgentConfig = {
  host: "",
  generation: "",
  runtimeId: "",
  instructions: "",
  modelId: "",
  effort: "",
};

const EFFORT_HARNESSES = new Set(["claude", "codex", "goose", "buzz-agent"]);
const EFFORT_OPTIONS = [
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
] as const;
const defined = <T extends Record<string, unknown>>(obj: T) =>
  Object.fromEntries(Object.entries(obj).filter(([, v]) => Boolean(v)));
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
const supportsEffort = (h: string | undefined, r: string) =>
  EFFORT_HARNESSES.has(h ?? r.split("/").pop() ?? "");
const errMsg = (r: unknown, fb: string) =>
  r instanceof Error ? r.message : fb;

function readinessMessage(
  obs: RegisteredHostObservation | undefined,
  fallback?: string,
): string {
  if (!obs)
    return fallback || "Prepare this host agent to inspect available models.";
  if (obs.availability && obs.availability !== "available")
    return `Host runtime is currently ${obs.availability.replaceAll("_", " ")}.`;
  if (obs.readiness === "logged-out")
    return "Sign in to this harness on the host computer, then click Refresh.";
  if (obs.readiness === "config-invalid" || obs.readiness === "unsupported")
    return `Host configuration is ${obs.readiness}; check the host setup and Refresh.`;
  if (obs.readiness === "unknown")
    return "Could not verify host readiness. Try Refresh.";
  if (obs.modelsState === "empty")
    return "No models were reported on this host. Refresh to try again.";
  if (obs.modelsState !== "available")
    return `Host models are ${obs.modelsState}${obs.unavailableReason ? ` (${obs.unavailableReason})` : ""}.`;
  return obs.expiresAt <= Math.floor(Date.now() / 1000)
    ? "Host model list expired. Click Refresh to load current models."
    : fallback || "";
}

export function createRegisteredHostType(
  hosts: RegisteredHostsClient,
): AgentType<HostedAgentConfig> {
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
    const [loadingStatus, setLoadingStatus] = useState(false);
    const [refreshing, setRefreshing] = useState(false);
    const [error, setError] = useState("");
    const refreshCtrl = useRef<AbortController | null>(null);

    useEffect(() => {
      if (cfg.frozen || !cfg.host || !cfg.generation) {
        setStatus(undefined);
        setLoadingStatus(false);
        return;
      }
      const ctrl = new AbortController();
      setLoadingStatus(true);
      setError("");
      setStatus(undefined);
      void hosts
        .status({ host: cfg.host, generation: cfg.generation }, ctrl.signal)
        .then(
          (next) => {
            if (!ctrl.signal.aborted) {
              setStatus(next);
              setLoadingStatus(false);
            }
          },
          (reason) => {
            if (!ctrl.signal.aborted) {
              setLoadingStatus(false);
              setError(errMsg(reason, "Could not query host."));
            }
          },
        );
      return () => ctrl.abort();
    }, [cfg.frozen, cfg.host, cfg.generation]);

    const refreshSetup = (explicit: boolean) => {
      if (!cfg.pubkey) return;
      refreshCtrl.current?.abort();
      const ctrl = new AbortController();
      refreshCtrl.current = ctrl;
      setRefreshing(true);
      setError("");
      void hosts
        .setupState({ pubkey: cfg.pubkey, refresh: explicit }, ctrl.signal)
        .then(
          (o) => {
            if (ctrl.signal.aborted) return;
            setRefreshing(false);
            onChange(
              o.status === "needs-setup"
                ? {
                    ...cfg,
                    ...(o.observation ? { observation: o.observation } : {}),
                    statusMessage: o.message,
                  }
                : {
                    ...cfg,
                    applied: true,
                    statusMessage:
                      "Host setup is complete. Finish setup to list this agent.",
                  },
              { suggested: true },
            );
          },
          (reason) => {
            if (!ctrl.signal.aborted) {
              setRefreshing(false);
              setError(errMsg(reason, "Could not refresh host setup."));
            }
          },
        );
    };

    // biome-ignore lint/correctness/useExhaustiveDependencies: check initial observation expiry once when frozen setup opens.
    useEffect(() => {
      if (!cfg.frozen || !cfg.pubkey) return;
      if (
        !cfg.observation ||
        cfg.observation.expiresAt <= Math.floor(Date.now() / 1000)
      )
        refreshSetup(false);
      return () => refreshCtrl.current?.abort();
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [cfg.frozen, cfg.pubkey]);

    const runtimes = status?.runtimes ?? [];
    const selectedRt = runtimes.find((r) => r.runtimeId === cfg.runtimeId);
    const obs = cfg.observation;
    const obsPermits = Boolean(
      obs &&
        obs.expiresAt > Math.floor(Date.now() / 1000) &&
        obs.availability === "available" &&
        (obs.readiness === "logged-in" || obs.readiness === "not-applicable") &&
        obs.modelsState === "available" &&
        obs.models.length > 0,
    );
    const chosenModel = obs?.models.find((m) => m.id === cfg.modelId);
    const ready = cfg.frozen
      ? Boolean(cfg.pubkey && (cfg.applied || (obsPermits && chosenModel)))
      : Boolean(
          cfg.host &&
            cfg.generation &&
            cfg.runtimeId &&
            status?.availability === "current" &&
            selectedRt?.availability === "available" &&
            selectedRt.configSupported,
        );
    useLayoutEffect(() => {
      onReadyChange?.(ready);
    }, [onReadyChange, ready]);

    const label = runtimeTitle({
      runtimeId: cfg.runtimeId,
      harnessId: cfg.harnessId ?? null,
    });
    const harnessOpts = cfg.frozen
      ? cfg.runtimeId
        ? [{ value: cfg.runtimeId, label }]
        : []
      : runtimes.map((r) => ({
          value: r.runtimeId,
          label:
            r.availability && r.availability !== "available"
              ? `${runtimeTitle(r)} (${r.availability.replaceAll("_", " ")})`
              : runtimeTitle(r),
          disabled: r.availability !== "available" || !r.configSupported,
        }));
    const hostNote = loadingStatus
      ? "Loading host harnesses…"
      : status?.availability && status.availability !== "current"
        ? `Host status is ${status.availability}.`
        : status && !runtimes.length
          ? "No supported harnesses were found on this host."
          : "";
    const note = cfg.frozen
      ? cfg.applied
        ? cfg.statusMessage || ""
        : obsPermits
          ? ""
          : readinessMessage(obs, cfg.statusMessage)
      : hostNote;

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
                placeholder={loadingStatus ? "Loading…" : "Choose a harness"}
                disabled={
                  disabled ||
                  Boolean(cfg.frozen) ||
                  loadingStatus ||
                  harnessOpts.length === 0
                }
                groups={[{ label: "Host", options: harnessOpts }]}
                onValueChange={(runtimeId) => {
                  const found = runtimes.find((r) => r.runtimeId === runtimeId);
                  const { harnessId: _, ...rest } = cfg;
                  onChange({
                    ...rest,
                    runtimeId,
                    ...(found?.harnessId ? { harnessId: found.harnessId } : {}),
                    modelId: "",
                    effort: "",
                  });
                }}
              />
            }
          />
          {cfg.frozen && (
            <>
              <ModelFamilyMenu
                label="Model"
                align="end"
                selected={{
                  id: chosenModel?.id ?? "",
                  label: chosenModel
                    ? (chosenModel.name ?? chosenModel.id)
                    : "Choose a model",
                }}
                models={(obs?.models ?? []).map((m) => ({
                  id: m.id,
                  label: m.name ?? m.id,
                  ...(m.description ? { description: m.description } : {}),
                }))}
                disabled={disabled || refreshing || !obsPermits}
                onChange={(modelId) => onChange({ ...cfg, modelId })}
              />
              {supportsEffort(cfg.harnessId, cfg.runtimeId) && (
                <PreferenceRow
                  title="Thinking"
                  trailing={
                    <Select
                      label="Thinking"
                      variant="compact"
                      align="end"
                      value={cfg.effort}
                      placeholder="Default"
                      disabled={
                        disabled || refreshing || !obsPermits || !chosenModel
                      }
                      groups={[{ label: "", options: [...EFFORT_OPTIONS] }]}
                      onValueChange={(effort) => onChange({ ...cfg, effort })}
                    />
                  }
                />
              )}
            </>
          )}
        </SettingsGroup>
        {(note || error) && (
          <p
            role={error ? "alert" : "status"}
            className={`m-0 text-body-sm ${error ? "text-danger" : "text-secondary"}`}
          >
            {error || note}
          </p>
        )}
        {cfg.frozen && cfg.pubkey && (
          <div>
            <Button
              disabled={disabled || refreshing}
              loading={refreshing}
              onClick={() => refreshSetup(true)}
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
          disabled={disabled || Boolean(cfg.frozen)}
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
      idempotentProvision: true,
      async provision({ name, picture, config, signal }) {
        const cfg = (config ?? EMPTY_HOSTED_CONFIG) as HostedAgentConfig;
        if (!cfg.host || !cfg.generation || !cfg.runtimeId)
          throw new Error("Choose a host and harness first.");
        return hosts.provision(
          {
            host: cfg.host,
            generation: cfg.generation,
            runtimeId: cfg.runtimeId,
            name,
            ...defined({
              picture,
              instructions: cfg.instructions,
              effort: cfg.effort,
            }),
          },
          signal,
        );
      },
      async activate({ pubkey, name, auth, config, signal }) {
        const cfg = (config ?? EMPTY_HOSTED_CONFIG) as HostedAgentConfig;
        const outcome = await hosts.activate(
          {
            pubkey,
            name,
            nipOa: auth,
            ...defined({
              host: cfg.host,
              generation: cfg.generation,
              runtimeId: cfg.runtimeId,
              modelId: cfg.modelId,
              effort: cfg.effort,
            }),
          },
          signal,
        );
        if (outcome.status === "applied") return;
        return {
          status: "incomplete",
          reason: outcome.message,
          config: {
            ...cfg,
            pubkey,
            frozen: true,
            ...(outcome.observation
              ? { observation: outcome.observation }
              : {}),
            statusMessage: outcome.message,
          } satisfies HostedAgentConfig,
        };
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

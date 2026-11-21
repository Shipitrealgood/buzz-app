use super::{origin, relay_info, ws_conn, Journal, Session};
use crate::{
    federated_identity::{assertion::HEADER, FederatedIdentity},
    identity::IdentityHost,
};
use anyhow::{anyhow, bail, ensure, Context, Result};
use buzz_core::{
    harness::{AcpAvailabilityStatus, Readiness},
    host_deployment::{
        observation::Models, wire, Action, Authority, Availability, Capability, Command, Deploy,
        ModelSelection, Status as HostStatus,
    },
    paired_message::{Policy, Scope},
    private_managed_agent::{validate_auth_tag, PrivateIdentity},
};
use buzz_paired_owner::deployment::{
    operation_error_category, ContinueDeploy, Intent, Management, Operation, Reservation,
    ReservationConfig, ReservationState::*, ReserveAgent,
};
use buzz_paired_owner::{registry::Registry, Device};
use buzz_ws_client::HeaderMap;
use nostr_registry::{EventBuilder, Keys, Kind, PublicKey, Tag, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;

const POLL_WAIT: Duration = Duration::from_millis(250);
const POLL_TRIES: usize = 40;
static LOCKS: OnceLock<tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    OnceLock::new();

async fn agent_lock(pk: &str) -> Arc<tokio::sync::Mutex<()>> {
    LOCKS
        .get_or_init(Default::default)
        .lock()
        .await
        .entry(pk.into())
        .or_default()
        .clone()
}
fn now_secs() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
fn random_hex() -> String {
    let mut b = [0u8; 32];
    let _ = getrandom::fill(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn msg_err<E: std::fmt::Display>(e: E) -> anyhow::Error {
    anyhow!("{e}")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostRuntimeView {
    pub(crate) runtime_id: String,
    pub(crate) harness_id: Option<String>,
    pub(crate) availability: Option<AcpAvailabilityStatus>,
    pub(crate) version: Option<String>,
    pub(crate) config_supported: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostCatalogView {
    pub(crate) availability: Availability,
    pub(crate) catalog_revision: Option<String>,
    pub(crate) expires_at: u64,
    pub(crate) runtimes: Vec<HostRuntimeView>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProvisionInput {
    pub(crate) community: String,
    pub(crate) owner: String,
    pub(crate) host: String,
    pub(crate) generation: String,
    pub(crate) runtime_id: String,
    pub(crate) name: String,
    pub(crate) picture: Option<String>,
    pub(crate) instructions: Option<String>,
    pub(crate) effort: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProvisionResult {
    pub(crate) pubkey: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) picture: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ActivateInput {
    pub(crate) community: String,
    pub(crate) owner: String,
    pub(crate) pubkey: String,
    pub(crate) name: String,
    pub(crate) nip_oa: [String; 4],
    pub(crate) host: Option<String>,
    pub(crate) generation: Option<String>,
    pub(crate) runtime_id: Option<String>,
    pub(crate) model_id: Option<String>,
    pub(crate) catalog_revision: Option<String>,
    pub(crate) effort: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HostModelsView {
    pub(crate) expires_at: u64,
    pub(crate) readiness: Readiness,
    pub(crate) availability: AcpAvailabilityStatus,
    pub(crate) models: Models,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ModelsInput {
    community: String,
    owner: String,
    host: String,
    generation: String,
    runtime_id: String,
    catalog_revision: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProfileInput {
    pub(crate) community: String,
    pub(crate) owner: String,
    pub(crate) pubkey: String,
    pub(crate) name: String,
    pub(crate) picture: Option<String>,
}

fn project_status(st: &HostStatus) -> HostCatalogView {
    let (catalog_revision, runtimes) = match &st.capability {
        Capability::Supported {
            catalog_revision: r,
            runtimes: ids,
            harnesses: hs,
        } => (
            Some(r.clone()),
            ids.iter()
                .map(|id| {
                    let f = st.inventory.as_ref().and_then(|inv| inv.get(id));
                    HostRuntimeView {
                        runtime_id: id.clone(),
                        harness_id: hs.get(id).cloned(),
                        availability: f.map(|x| x.availability.clone()),
                        version: f.and_then(|x| x.version.clone()),
                        config_supported: f.is_some_and(|x| x.config_supported),
                    }
                })
                .collect(),
        ),
        _ => (None, Vec::new()),
    };
    HostCatalogView {
        availability: st.availability.clone(),
        catalog_revision,
        expires_at: st.expires_at,
        runtimes,
    }
}

async fn with_mgmt<T>(
    s: &Session<'_>,
    fi: &FederatedIdentity,
    step: impl AsyncFnOnce(&mut Management) -> Result<T>,
) -> Result<T> {
    let meta = relay_info(&s.community).await.map_err(msg_err)?;
    s.keys()?;
    ensure!(
        meta["host_deployment"]
            == json!({"versions":[1],"kind":43104,"admission":"host-scoped","reads":"req-v1"}),
        "Host deployment not supported"
    );
    let (relay, url) = (
        Registry::capability(&meta)?,
        origin(&s.community).map_err(msg_err)?,
    );
    let auth = fi
        .assertion_for_relay(s.identity, &url)
        .await
        .map_err(msg_err)?;
    s.keys()?;
    let mut hdrs = HeaderMap::new();
    if let Some(h) = auth.header() {
        hdrs.insert(HEADER, h.parse()?);
    }
    let op = async {
        let mut ws = ws_conn(&s.community.replacen("https://", "wss://", 1), hdrs).await?;
        s.authenticate(&mut ws).await?;
        let mut mgmt =
            Management::from_authenticated(true, &s.community, s.keys()?.public_key(), relay, ws)?;
        step(&mut mgmt).await
    };
    fi.during_authorization(s.identity, &auth, Box::pin(op))
        .await
        .map_err(msg_err)?
}

pub(super) async fn publish_kind0_over_ws(
    ws_url: &str,
    ak: &Keys,
    nip_oa: &[String],
    name: &str,
    pic: Option<&str>,
    min_ts: u64,
) -> Result<u64> {
    let tag = Tag::parse(nip_oa)?;
    let mut obj = json!({ "name": name, "display_name": name, "bot": true });
    if let Some(p) = pic.filter(|p| !p.is_empty()) {
        obj["picture"] = p.into();
    }
    let ts = now_secs()?.max(min_ts);
    let ev = EventBuilder::new(Kind::Metadata, obj.to_string())
        .tags([tag.clone()])
        .custom_created_at(Timestamp::from(ts))
        .sign_with_keys(ak)?;
    let mut ws = ws_conn(ws_url, HeaderMap::new()).await?;
    ws.authenticate_with_signer(|c, u| {
        buzz_ws_client::build_auth_event_with_keys(c, u, ak, Some(&tag))
    })
    .await?;
    let ok = ws.send_event(ev).await?;
    let _ = ws.disconnect().await;
    ensure!(ok.accepted, "Profile publication rejected by relay");
    Ok(ts)
}

async fn publish_profile(
    s: &Session<'_>,
    d: &mut Device,
    r: &Reservation,
    ws_url: Option<&str>,
) -> Result<()> {
    if r.profile_synced {
        return Ok(());
    }
    let k = s.keys()?;
    let ak = Keys::parse(r.agent_nsec.as_ref().context("missing key")?.as_str())?;
    ensure!(ak.public_key() == r.agent, "agent key mismatch");
    let oa = r.nip_oa.as_ref().context("missing attestation")?;
    let (name, pic) = r
        .desired_profile
        .as_ref()
        .map_or((r.name_scope.as_str(), r.picture.as_deref()), |p| {
            (p.name.as_str(), p.picture.as_deref())
        });
    let url = ws_url
        .map(str::to_owned)
        .unwrap_or_else(|| s.community.replacen("https://", "wss://", 1));
    let ts = publish_kind0_over_ws(
        &url,
        &ak,
        oa,
        name,
        pic,
        r.last_profile_created_at.map_or(0, |t| t + 1),
    )
    .await?;
    s.keys()?;
    d.mark_profile_published(&s.community, &k, &r.agent, ts)?;
    Ok(())
}

async fn query_status(
    m: &mut Management,
    s: &Session<'_>,
    scope: &Scope,
) -> Result<Option<HostStatus>> {
    let q = wire::Query {
        v: 1,
        scope: scope.clone(),
        view: "status".into(),
        nonce: random_hex(),
        operation_id: None,
    };
    let Some(raw) = m.query(&|| s.keys(), &q).await?.data["event"]
        .as_str()
        .map(str::to_owned)
    else {
        return Ok(None);
    };
    Ok(Some(wire::open_status(
        &s.keys()?,
        &serde_json::from_str(&raw)?,
        scope,
        now_secs()?,
    )?))
}

fn require_active_host(d: &Device, s: &Session<'_>, host: &str, gen: &str) -> Result<Scope> {
    let k = s.keys()?;
    ensure!(
        buzz_core::host_registry::identifier(gen),
        "Invalid host generation"
    );
    let scope = Scope {
        community: s.community.clone(),
        generation: gen.into(),
        owner: k.public_key(),
        host: PublicKey::from_hex(host)?,
    };
    ensure!(
        d.view(&s.community, &k, &scope)?.state == "paired",
        "Host registration is not active"
    );
    Ok(scope)
}

struct OpCtx<'a, 'b> {
    s: &'a Session<'b>,
    d: &'a mut Device,
    m: &'a mut Management,
    k: Keys,
    pk: PublicKey,
}

impl<'a, 'b> OpCtx<'a, 'b> {
    async fn read_op(&mut self, id: &str) -> Result<Operation> {
        self.m
            .operation(self.d, &|| self.s.keys(), id)
            .await
            .map_err(|e| anyhow!("Operation read failed: {}", operation_error_category(&e)))
    }
    async fn poll_op(&mut self, id: &str) -> Result<Operation> {
        for i in 0..POLL_TRIES {
            let op = self.read_op(id).await?;
            if op.admitted && op.state != "pending" {
                return Ok(op);
            }
            if i + 1 < POLL_TRIES {
                tokio::time::sleep(POLL_WAIT).await;
                self.s.keys()?;
            }
        }
        self.read_op(id).await
    }
    async fn dispatch_poll(&mut self, intent: &Intent) -> Result<Operation> {
        let id = &intent.protected.authority.operation_id;
        // The reservation already names this intent, even if the process stopped
        // before recording either the materialization or profile-publication phase.
        for phase in intent.phase + 1..=2 {
            self.d
                .deployment_advance(&self.s.community, &self.k, id, phase)?;
        }
        self.m.dispatch(self.d, &|| self.s.keys(), id).await?;
        self.poll_op(id).await
    }
    fn finish_applied(
        &mut self,
        id: &str,
        op: &Operation,
        eff: Option<Option<String>>,
    ) -> Result<()> {
        ensure!(
            op.admitted && op.current && op.state == "applied",
            "Host agent launch did not complete ({}). Check the host and retry.",
            op.state
        );
        let c = self.s.community.as_str();
        if self
            .d
            .deployment_intent(c, &self.k, id)?
            .is_some_and(|i| i.phase == 3)
        {
            self.d.deployment_advance(c, &self.k, id, 4)?;
        }
        self.d
            .mark_reservation_active(c, &self.k, &self.pk, id, eff)?;
        Ok(())
    }
    async fn permitted_status(
        &mut self,
        r: &Reservation,
        selected: Option<&str>,
    ) -> Result<(HostStatus, String)> {
        let st = query_status(self.m, self.s, &r.scope)
            .await?
            .context("Host status is currently unavailable")?;
        let Capability::Supported {
            ref catalog_revision,
            ..
        } = st.capability
        else {
            bail!("Host does not support agent deployment");
        };
        ensure!(
            selected.is_none_or(|id| id == catalog_revision),
            "Host catalog changed. Refresh and choose a model."
        );
        let rev = catalog_revision.clone();
        st.permits(&r.scope, &rev, &r.runtime_id, now_secs()?)?;
        Ok((st, rev))
    }
}

pub(super) async fn status_inner(
    s: &Session<'_>,
    m: &mut Management,
    scope: &Scope,
) -> Result<HostCatalogView> {
    Ok(query_status(m, s, scope).await?.map_or_else(
        || HostCatalogView {
            availability: Availability::Unavailable,
            catalog_revision: None,
            expires_at: 0,
            runtimes: Vec::new(),
        },
        |st| project_status(&st),
    ))
}

pub(super) fn provision_inner(
    s: &Session<'_>,
    d: &mut Device,
    i: ProvisionInput,
) -> Result<ProvisionResult> {
    let scope = require_active_host(d, s, &i.host, &i.generation)?;
    let k = s.keys()?;
    let config = ReservationConfig {
        instructions: i.instructions,
        effort: i.effort,
        parallelism: 1,
        respond_to: "owner-only".into(),
        respond_to_allowlist: Vec::new(),
        channel_add_policy: "owner_only".into(),
    };
    let res = d.reserve_agent(
        &s.community,
        &k,
        ReserveAgent {
            scope: &scope,
            runtime_id: &i.runtime_id,
            name: &i.name,
            picture: i.picture,
            config,
        },
    )?;
    s.keys()?;
    Ok(ProvisionResult {
        pubkey: res.agent.to_hex(),
        picture: res.picture,
    })
}

pub(super) async fn activate_inner(
    s: &Session<'_>,
    d: &mut Device,
    m: &mut Management,
    i: ActivateInput,
    kind0_ws: Option<&str>,
) -> Result<()> {
    let (k, pk) = (s.keys()?, PublicKey::from_hex(&i.pubkey)?);
    ensure!(
        validate_auth_tag(&serde_json::to_string(&i.nip_oa)?, &s.owner, &pk).is_ok(),
        "Invalid agent authorization"
    );
    let init = d
        .reservation(&s.community, &k, &pk)?
        .context("Agent reservation not found")?;
    ensure!(
        !matches!(init.state, Retiring | Retired),
        "Agent reservation has been retired"
    );
    ensure!(init.name_scope == i.name.trim(), "Agent name mismatch");
    ensure!(
        i.host
            .as_deref()
            .is_none_or(|h| *h == init.scope.host.to_hex())
            && i.generation
                .as_deref()
                .is_none_or(|g| *g == init.scope.generation)
            && i.runtime_id
                .as_deref()
                .is_none_or(|rt| *rt == init.runtime_id),
        "Agent placement mismatch"
    );

    let model = i.model_id.as_deref().filter(|id| !id.trim().is_empty());
    if init.state == Reserved {
        ensure!(model.is_some(), "Choose a current host model in Create agent using this agent’s original name and harness.");
    }
    let res = d.bind_reservation_attestation(&s.community, &k, &pk, i.nip_oa.to_vec())?;
    publish_profile(s, d, &res, kind0_ws).await?;
    let mut ctx = OpCtx { s, d, m, k, pk };

    if res.state == Reserved {
        ensure!(res.operation_id.is_none(), "unexpected operation");
        let (st, rev) = ctx
            .permitted_status(&res, i.catalog_revision.as_deref())
            .await?;
        let (nsec, oa, c) = (
            res.agent_nsec.as_ref().context("missing key")?,
            res.nip_oa.as_ref().context("missing attestation")?,
            &res.config,
        );
        let cmd = Command::Deploy {
            config: Box::new(Deploy {
                catalog_revision: rev,
                runtime_id: res.runtime_id.clone(),
                name: res.name_scope.clone(),
                policy: Policy {
                    respond_to: c.respond_to.clone(),
                    respond_to_allowlist: c.respond_to_allowlist.clone(),
                    channel_add_policy: c.channel_add_policy.clone(),
                },
                model: ModelSelection::Selected {
                    id: model.context("Missing selected model")?.into(),
                },
                provider: None,
                system_prompt: c.instructions.clone(),
                effort: c.effort.clone(),
                parallelism: c.parallelism,
            }),
            identity: PrivateIdentity {
                private_key_nsec: nsec.as_str().into(),
                auth_tag: Some(serde_json::to_string(oa)?),
            },
        };
        let snap = json!({ "record": { "pubkey": res.agent.to_hex(), "name": res.name_scope, "runtime_id": res.runtime_id, "model": model, "effort": c.effort } });
        let auth = Authority {
            v: 1,
            scope: res.scope.clone(),
            agent: res.agent,
            action: Action::Deploy,
            operation_id: random_hex(),
            expected_revision: 0,
            expires_at: now_secs()? + 600,
            content_sha256: String::new(),
        };
        let intent = ctx
            .d
            .prepare_deployment(&ctx.s.community, &ctx.k, auth, &cmd, &st, snap)?;
        let op = ctx.dispatch_poll(&intent).await?;
        return ctx.finish_applied(&intent.protected.authority.operation_id, &op, None);
    }

    let op_id = res.operation_id.as_deref().context("missing operation")?;
    let prior = ctx
        .d
        .deployment_intent(&ctx.s.community, &ctx.k, op_id)?
        .context("missing intent")?;
    let mut snapshot = ctx
        .d
        .restore_materialization(&ctx.s.community, &ctx.k, &prior)?;
    let retained_model = snapshot["record"]["model"].as_str();
    let mut op = ctx.read_op(op_id).await?;
    if res.state == Active {
        ensure!(
            op.admitted && op.current && op.state == "applied",
            "Active host placement is no longer applied"
        );
        ensure!(
            model.is_none_or(|id| Some(id) == retained_model),
            "This agent already launched with its original model."
        );
        return Ok(());
    }
    if !op.admitted && prior.phase <= 2 && prior.protected.authority.expires_at > now_secs()? {
        ensure!(retained_model.is_some() && model == retained_model,
            "The retained deployment has a different model. Retry the original selection or remove the unfinished agent.");
        op = ctx.dispatch_poll(&prior).await?;
    } else if op.state == "pending" {
        op = ctx.poll_op(op_id).await?;
    }
    // A reply can be lost after launch. Finish the retained identity and exact
    // command before considering any same-identity recovery.
    if op.state == "applied" || op.state == "pending" {
        ensure!(
            model.is_none_or(|id| Some(id) == retained_model),
            "This agent already has a retained model. Retry the original selection."
        );
        let retained_effort = serde_json::from_value(snapshot["record"]["effort"].clone())?;
        return ctx.finish_applied(op_id, &op, Some(retained_effort));
    }
    let model = model.context(
        "Choose a current host model in Create agent using this agent’s original name and harness.",
    )?;
    let (st, _) = ctx
        .permitted_status(&res, i.catalog_revision.as_deref())
        .await?;
    ensure!(op.admitted && op.current, "The previous deployment is no longer current. Remove the unfinished agent before starting again.");
    snapshot["record"]["model"] = model.into();
    snapshot["record"]["effort"] = json!(i.effort);
    let next = ctx
        .m
        .continue_deployment_with_effort(
            ctx.d,
            &|| ctx.s.keys(),
            ContinueDeploy {
                previous: op_id,
                status: &st,
                model: ModelSelection::Selected { id: model.into() },
                effort: Some(i.effort.clone()),
                snapshot,
            },
        )
        .await?;
    let settled = ctx.dispatch_poll(&next).await?;
    ctx.finish_applied(
        &next.protected.authority.operation_id,
        &settled,
        Some(i.effort),
    )
}

pub(super) async fn profile_inner(
    s: &Session<'_>,
    d: &mut Device,
    i: ProfileInput,
    kind0_ws: Option<&str>,
) -> Result<()> {
    let (k, pk) = (s.keys()?, PublicKey::from_hex(&i.pubkey)?);
    let res = d.set_desired_profile(&s.community, &k, &pk, &i.name, i.picture)?;
    publish_profile(s, d, &res, kind0_ws).await
}

pub(super) async fn remove_inner(
    s: &Session<'_>,
    d: &mut Device,
    fi: Option<&FederatedIdentity>,
    injected: Option<&mut Management>,
    pk_hex: &str,
) -> Result<()> {
    let (k, c, pk) = (s.keys()?, &s.community, PublicKey::from_hex(pk_hex)?);
    let existing = d.reservation(c, &k, &pk)?;
    let intents: Vec<_> = d
        .deployment_intents(c, &k)?
        .into_iter()
        .filter(|i| i.protected.authority.agent == pk)
        .collect();
    if let Some(res) = &existing {
        if res.state == Retired {
            ensure!(
                res.agent_nsec.is_none() && res.nip_oa.is_none() && res.pending_name.is_none(),
                "retired reservation retains custody material"
            );
            return Ok(());
        }
        if res.state == Reserved && res.operation_id.is_none() && intents.is_empty() {
            d.finish_retire_reservation(c, &k, &pk, "never-deployed", None)?;
            return Ok(());
        }
    } else if intents.is_empty() {
        return Ok(());
    }

    d.begin_retire_reservation(c, &k, &pk)?;
    let op_id = existing
        .as_ref()
        .and_then(|r| r.operation_id.clone())
        .or_else(|| {
            intents
                .first()
                .map(|i| i.protected.authority.operation_id.clone())
        })
        .context("missing cleanup op")?;
    let run = async |m: &mut Management, d: &mut Device| -> Result<()> {
        let plan = match m.plan_cleanup(d, &|| s.keys(), &op_id, true).await {
            Ok(p) => p,
            Err(e) => {
                let wall = now_secs()?;
                let exp = !intents.is_empty()
                    && intents.iter().all(|i| {
                        let a = &i.protected.authority;
                        a.action == Action::Deploy
                            && a.expected_revision == 0
                            && wall >= a.expires_at
                    });
                if !exp {
                    return Err(e);
                }
                for i in &intents {
                    if m.operation(d, &|| s.keys(), &i.protected.authority.operation_id)
                        .await?
                        .admitted
                    {
                        return Err(e);
                    }
                }
                if existing.is_some() {
                    d.finish_retire_reservation(c, &k, &pk, "never-deployed", None)?;
                }
                return Ok(());
            }
        };
        let cid = d
            .retain_cleanup(c, &k, plan)?
            .protected
            .authority
            .operation_id;
        m.dispatch(d, &|| s.keys(), &cid).await?;
        let (mut ok, mut attempt) = (false, 0);
        while attempt < POLL_TRIES {
            if m.cleanup_state(d, &|| s.keys(), &cid).await?.1 {
                ok = true;
                break;
            }
            attempt += 1;
            if attempt < POLL_TRIES {
                tokio::time::sleep(POLL_WAIT).await;
                s.keys()?;
            }
        }
        ensure!(ok, "Host cleanup pending; retry delete");
        if d.deployment_intent(c, &k, &cid)?
            .is_some_and(|i| i.phase < 4)
        {
            d.deployment_advance(c, &k, &cid, 4)?;
        }
        if existing.is_some() {
            d.finish_retire_reservation(c, &k, &pk, "cleaned", Some(&cid))?;
        }
        Ok(())
    };
    match injected {
        Some(m) => run(m, d).await,
        None => {
            with_mgmt(s, fi.context("federated identity required")?, async |m| {
                run(m, d).await
            })
            .await
        }
    }
}

async fn with_dev<R: tauri::Runtime, T>(
    ctx: (&tauri::AppHandle<R>, &IdentityHost),
    o: &str,
    c: &str,
    pk: Option<&str>,
    step: impl AsyncFnOnce(&Session<'_>, &mut Device) -> Result<T>,
) -> Result<T, String> {
    let lock = match pk {
        Some(p) => Some(agent_lock(p).await),
        None => None,
    };
    let _g = match lock {
        Some(ref l) => Some(l.lock().await),
        None => None,
    };
    async {
        let s = Session::capture(ctx.1, o.into(), c.into()).await?;
        let mut j = Journal::open(&ctx.0.path().app_local_data_dir()?)?;
        // Keep journal operations out of the by-value Tauri dispatch future.
        Box::pin(step(&s, j.device.as_mut().context(super::UNAVAILABLE)?)).await
    }
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn registered_host_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    identity: tauri::State<'_, IdentityHost>,
    fi: tauri::State<'_, FederatedIdentity>,
    community: String,
    owner: String,
    host: String,
    generation: String,
) -> Result<HostCatalogView, String> {
    let s = Session::capture(&identity, owner, community)
        .await
        .map_err(|e| e.to_string())?;
    let scope = read_scope(&app, &s, &host, &generation).map_err(|e| e.to_string())?;
    Box::pin(with_mgmt(&s, &fi, async |m| {
        status_inner(&s, m, &scope).await
    }))
    .await
    .map_err(|e| e.to_string())
}

// Release the local file lock before independent discovery requests wait for a host.
fn read_scope<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    s: &Session<'_>,
    host: &str,
    generation: &str,
) -> Result<Scope> {
    let j = Journal::open(&app.path().app_local_data_dir()?)?;
    require_active_host(
        j.device.as_ref().context(super::UNAVAILABLE)?,
        s,
        host,
        generation,
    )
}
#[tauri::command]
pub(crate) async fn registered_host_provision<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    identity: tauri::State<'_, IdentityHost>,
    input: ProvisionInput,
) -> Result<ProvisionResult, String> {
    let (o, c) = (input.owner.clone(), input.community.clone());
    with_dev((&app, &identity), &o, &c, None, async |s, d| {
        provision_inner(s, d, input)
    })
    .await
}
#[tauri::command]
pub(crate) async fn registered_host_activate<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    identity: tauri::State<'_, IdentityHost>,
    fi: tauri::State<'_, FederatedIdentity>,
    input: ActivateInput,
) -> Result<(), String> {
    let (o, c, p) = (
        input.owner.clone(),
        input.community.clone(),
        input.pubkey.clone(),
    );
    with_dev((&app, &identity), &o, &c, Some(&p), async |s, d| {
        with_mgmt(s, &fi, async |m| activate_inner(s, d, m, input, None).await).await
    })
    .await
}
#[tauri::command]
pub(crate) async fn registered_host_models<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    identity: tauri::State<'_, IdentityHost>,
    fi: tauri::State<'_, FederatedIdentity>,
    input: ModelsInput,
) -> Result<HostModelsView, String> {
    let read = async {
        let s = Session::capture(&identity, input.owner, input.community).await?;
        let scope = read_scope(&app, &s, &input.host, &input.generation)?;
        Box::pin(s.with_registry(&fi, async |registry| {
            // The signed request has a 60-second lifetime. Never extend it in the UI.
            let expires = now_secs()? + 60;
            let models = registry
                .models(
                    &|| s.keys(),
                    &scope,
                    &input.runtime_id,
                    &input.catalog_revision,
                )
                .await?;
            Ok(HostModelsView {
                expires_at: expires,
                models: models.models,
                readiness: models.readiness,
                availability: models.availability,
            })
        }))
        .await?
        .context("Host model discovery unavailable")
    };
    read.await.map_err(|e| e.to_string())
}
#[tauri::command]
pub(crate) async fn registered_host_profile<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    identity: tauri::State<'_, IdentityHost>,
    input: ProfileInput,
) -> Result<(), String> {
    let (o, c, p) = (
        input.owner.clone(),
        input.community.clone(),
        input.pubkey.clone(),
    );
    with_dev((&app, &identity), &o, &c, Some(&p), async |s, d| {
        profile_inner(s, d, input, None).await
    })
    .await
}
#[tauri::command]
pub(crate) async fn registered_host_remove<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    identity: tauri::State<'_, IdentityHost>,
    fi: tauri::State<'_, FederatedIdentity>,
    community: String,
    owner: String,
    pubkey: String,
) -> Result<(), String> {
    with_dev(
        (&app, &identity),
        &owner,
        &community,
        Some(&pubkey),
        async |s, d| remove_inner(s, d, Some(&fi), None, &pubkey).await,
    )
    .await
}

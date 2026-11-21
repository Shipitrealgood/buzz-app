use super::*;
use buzz_core::host_registry::{self as wire, Body, Query, Reply};
use buzz_paired_owner::{registry::RegistryBinding, Scope};
use futures_util::{SinkExt, StreamExt};
use nostr_registry::{Event, PublicKey, Timestamp};
use serde_json::{json, Value};

const COMMUNITY: &str = "https://example.org";
fn keys(byte: u8) -> Keys {
    Keys::new(SecretKey::from_slice(&[byte; 32]).unwrap())
}

#[test]
fn recovery_ipc_refuses_another_owner_before_opening_storage_or_network() {
    use tauri::test::{get_ipc_response, mock_builder, INVOKE_KEY};
    let app = mock_builder()
        .manage(IdentityHost::fixture())
        .manage(FederatedIdentity::new().unwrap())
        .invoke_handler(crate::commands())
        .build(crate::app_context())
        .unwrap();
    let view = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .unwrap();
    let other_owner = keys(2).public_key().to_hex();
    for (cmd, body, error) in [
        (
            "registered_host_recover",
            json!({ "community": COMMUNITY, "owner": other_owner }),
            UNAVAILABLE,
        ),
        (
            "registered_host_models",
            json!({ "input": { "community": COMMUNITY, "owner": other_owner,
            "host": keys(3).public_key().to_hex(), "generation": "aa".repeat(32), "runtimeId": "acp/claude",
            "catalogRevision": "bb".repeat(32) } }),
            "Active identity changed",
        ),
    ] {
        let response = get_ipc_response(
            &view,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: view.url().unwrap(),
                body: tauri::ipc::InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.into(),
            },
        );
        assert_eq!(response.unwrap_err(), json!(error));
    }
}

#[tokio::test]
async fn ready_identity_refuses_wrong_owner_closed_identity_and_noncanonical_origin() {
    let identity = IdentityHost::fixture();
    assert!(
        Session::capture(&identity, keys(2).public_key().to_hex(), COMMUNITY.into())
            .await
            .is_err()
    );
    for origin in [
        "http://example.org",
        "https://example.org/path",
        "https://example.org/",
    ] {
        assert!(
            Session::capture(&identity, keys(1).public_key().to_hex(), origin.into())
                .await
                .is_err()
        );
    }
    let session = Session::capture(&identity, keys(1).public_key().to_hex(), COMMUNITY.into())
        .await
        .unwrap();
    identity.close();
    assert!(session.keys().is_err());
    let root = tempfile::tempdir().unwrap();
    let journal = Journal::open(root.path()).unwrap();
    assert!(session
        .directory(journal.device.as_ref().unwrap(), "unavailable")
        .is_err());
}

#[test]
fn journal_is_private_exclusive_and_reopenable() {
    let root = tempfile::tempdir().unwrap();
    let journal = Journal::open(root.path()).unwrap();
    assert!(Journal::open(root.path()).is_err());
    plain(&root.path().join("registered-hosts-v1"), true).unwrap();
    plain(&root.path().join("registered-hosts-v1/owner.sqlite"), false).unwrap();
    drop(journal);
    drop(Journal::open(root.path()).unwrap());
}

#[cfg(unix)]
#[test]
fn journal_rejects_links_and_insecure_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let directory = root.path().join("registered-hosts-v1");
    symlink(outside.path(), &directory).unwrap();
    assert!(Journal::open(root.path()).is_err());
    fs::remove_file(&directory).unwrap();
    drop(Journal::open(root.path()).unwrap());
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Journal::open(root.path()).is_err());
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_file(directory.join("owner.sqlite")).unwrap();
    symlink(
        outside.path().join("must-not-create"),
        directory.join("owner.sqlite"),
    )
    .unwrap();
    assert!(Journal::open(root.path()).is_err());
    assert!(!outside.path().join("must-not-create").exists());
}

// This fixture uses the real shared NIP-42 connection, encrypted/signed registry
// replies and Device recovery. Only the remote relay is replaced by loopback.
#[tokio::test]
async fn signed_read_recovers_and_reopens_without_runtime_readiness_or_inventory_writes() {
    let identity = IdentityHost::fixture();
    let owner = keys(1);
    let host = keys(2);
    let relay = keys(3);
    let session = Session::capture(&identity, owner.public_key().to_hex(), COMMUNITY.into())
        .await
        .unwrap();
    let scope = Scope {
        community: COMMUNITY.into(),
        owner: owner.public_key(),
        host: host.public_key(),
        generation: "ab".repeat(32),
    };
    let now = Timestamp::now().as_secs();
    let claim = wire::seal(
        &owner,
        &scope,
        Body::PairClaim {
            proof: "cd".repeat(32),
        },
        "01".repeat(32),
        None,
        now,
        now + 600,
    )
    .unwrap();
    let ack = wire::seal(
        &host,
        &scope,
        Body::PairAck {
            claim_id: claim.id.to_hex(),
        },
        "02".repeat(32),
        Some(claim.id.to_hex()),
        now,
        now + 600,
    )
    .unwrap();
    let binding = RegistryBinding {
        generation: scope.generation.clone(),
        owner: scope.owner.to_hex(),
        host: scope.host.to_hex(),
        state: "active".into(),
        claim_id: claim.id.to_hex(),
        ack_id: ack.id.to_hex(),
        tombstone_id: None,
        latest_status_id: None,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let fixture_relay = relay.clone();
    let fixture_owner = owner.public_key();
    let fixture = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket
            .send(json!(["AUTH", "fixture-challenge"]).to_string().into())
            .await
            .unwrap();
        let auth: Value =
            serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap()).unwrap();
        assert_eq!(auth[0], "AUTH");
        let auth_event: Event = serde_json::from_value(auth[1].clone()).unwrap();
        auth_event.verify().unwrap();
        assert_eq!(auth_event.pubkey, fixture_owner);
        assert_eq!(auth_event.kind.as_u16(), 22242);
        assert!(auth_event
            .tags
            .iter()
            .any(|tag| tag.as_slice() == ["challenge", "fixture-challenge"]));
        socket
            .send(json!(["OK", auth_event.id, true, ""]).to_string().into())
            .await
            .unwrap();
        let mut views = Vec::new();
        while let Some(message) = socket.next().await {
            let message = message.unwrap();
            if message.is_close() {
                break;
            }
            let frame: Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
            // A host mutation would be EVENT. Recovery sends only REQ/CLOSE.
            if frame[0] == "CLOSE" {
                continue;
            }
            assert_eq!(frame[0], "REQ");
            let query: Query = serde_json::from_value(frame[2]["host_registry"].clone()).unwrap();
            views.push(query.view.clone());
            let data = match query.view.as_str() {
                "live" => json!({}),
                "hosts" => json!({"hosts": [binding]}),
                "binding" => json!(binding),
                "evidence" if query.event_id.as_deref() == Some(claim.id.to_hex().as_str()) => {
                    json!({"event": claim})
                }
                "evidence" => {
                    assert_eq!(query.event_id, Some(ack.id.to_hex()));
                    json!({"event": ack})
                }
                other => panic!("unexpected read {other}"),
            };
            let event = wire::reply(
                &fixture_relay,
                fixture_owner,
                Reply {
                    v: 2,
                    community: COMMUNITY.into(),
                    recipient: fixture_owner.to_hex(),
                    op: "registry-page".into(),
                    query,
                    high_water: "03".repeat(32),
                    data,
                    next_cursor: None,
                    complete: true,
                },
                Timestamp::now().as_secs(),
            )
            .unwrap();
            socket
                .send(json!(["EVENT", frame[1], event]).to_string().into())
                .await
                .unwrap();
            socket
                .send(json!(["EOSE", frame[1]]).to_string().into())
                .await
                .unwrap();
        }
        views
    });
    let mut socket = NostrWsConnection::connect_to_addr(
        COMMUNITY.replace("https://", "wss://").as_str(),
        &address.to_string(),
    )
    .await
    .unwrap();
    session.authenticate(&mut socket).await.unwrap();
    let mut registry =
        Registry::from_authenticated(COMMUNITY, owner.public_key(), relay.public_key(), socket)
            .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(root.path()).unwrap();
    session
        .recover_registry(&mut registry, journal.device.as_mut().unwrap())
        .await
        .unwrap();
    let result = session
        .directory(journal.device.as_ref().unwrap(), "available")
        .unwrap();
    assert_eq!(result.hosts.len(), 1);
    assert_eq!(result.hosts[0].registration, "registered");
    assert_eq!(result.hosts[0].availability, "unavailable");
    assert_eq!(result.hosts[0].generation, scope.generation);
    registry.disconnect().await.unwrap();
    assert_eq!(
        fixture.await.unwrap(),
        ["live", "hosts", "binding", "evidence", "evidence"]
    );
    drop(journal);
    let journal = Journal::open(root.path()).unwrap();
    let device = journal.device.as_ref().unwrap();
    let cached = session.directory(device, "unavailable").unwrap();
    assert_eq!(cached.hosts.len(), 1);
    assert_eq!(cached.registry, "unavailable");
    assert_eq!(cached.hosts[0].availability, "unavailable");
    assert!(device
        .registry_hosts(COMMUNITY, &keys(4))
        .unwrap()
        .is_empty());
    assert!(device
        .registry_hosts("https://other.example.org", &owner)
        .unwrap()
        .is_empty());
    let db =
        rusqlite::Connection::open(root.path().join("registered-hosts-v1/owner.sqlite")).unwrap();
    for table in ["requests", "known_identities", "delivery_cursor"] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "recovery wrote {table}");
    }
}

fn seed_paired_host(device: &mut Device, owner: &Keys, host: &Keys, community: &str) -> Scope {
    use buzz_core::paired_host::{build_ack, verify_claim, Offer};
    use buzz_core::paired_message::Route;
    let now_s = Timestamp::now().as_secs();
    let (offer, proof) = Offer::issue(community.into(), host.public_key(), now_s).unwrap();
    let inv = json!({
        "v": 1,
        "community": community,
        "generation": offer.generation,
        "host": offer.host.to_hex(),
        "expires_at": offer.expires_at,
        "proof": *proof.expose_hex(),
    })
    .to_string();
    let (scope, claim) = device.pair(community, owner, &inv).unwrap();
    let verified = verify_claim(
        claim.event.get().as_bytes(),
        host,
        &offer,
        Some(owner.public_key()),
        &proof.digest(),
        now_s,
    )
    .unwrap();
    let ack_ev = build_ack(host, &verified, now_s).unwrap();
    let ack = buzz_paired_owner::Packet {
        event: serde_json::value::to_raw_value(&ack_ev).unwrap(),
        route: Route {
            r: None,
            prev: Some(scope.generation.clone()),
            expires_at: offer.expires_at,
        },
    };
    let claim_id = serde_json::from_str::<Event>(claim.event.get())
        .unwrap()
        .id
        .to_hex();
    let mut fixed_ack = ack;
    fixed_ack.route.prev = Some(claim_id);
    let binding = buzz_paired_owner::Binding {
        generation: scope.generation.clone(),
        host: scope.host.to_hex(),
        owner: scope.owner.to_hex(),
        state: "active".into(),
        claim,
        ack: fixed_ack,
        tombstone: None,
        inventory: None,
        high_water: None,
    };
    device.restore(community, owner, &binding).unwrap();
    scope
}

#[tokio::test]
async fn provision_replays_same_identity_and_refuses_host_or_runtime_mismatch() {
    let identity = IdentityHost::fixture();
    let owner = keys(1);
    let host_a = keys(2);
    let host_b = keys(3);
    let session = Session::capture(&identity, owner.public_key().to_hex(), COMMUNITY.into())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(root.path()).unwrap();
    let device = journal.device.as_mut().unwrap();
    let scope_a = seed_paired_host(device, &owner, &host_a, COMMUNITY);
    let scope_b = seed_paired_host(device, &owner, &host_b, COMMUNITY);

    let first = provider::provision_inner(
        &session,
        device,
        provider::ProvisionInput {
            community: COMMUNITY.into(),
            owner: owner.public_key().to_hex(),
            host: scope_a.host.to_hex(),
            generation: scope_a.generation.clone(),
            runtime_id: "acp/claude".into(),
            name: "  Alpha  ".into(),
            picture: Some("https://example.test/a.png".into()),
            instructions: Some("initial".into()),
            effort: None,
        },
    )
    .unwrap();
    assert_eq!(first.picture.as_deref(), Some("https://example.test/a.png"));

    let replay = provider::provision_inner(
        &session,
        device,
        provider::ProvisionInput {
            community: COMMUNITY.into(),
            owner: owner.public_key().to_hex(),
            host: scope_a.host.to_hex(),
            generation: scope_a.generation.clone(),
            runtime_id: "acp/claude".into(),
            name: "Alpha".into(),
            picture: Some("https://example.test/other.png".into()),
            instructions: Some("updated".into()),
            effort: None,
        },
    )
    .unwrap();
    assert_eq!(replay.pubkey, first.pubkey);
    assert_eq!(replay.picture, first.picture);

    // Host mismatch refused
    assert!(provider::provision_inner(
        &session,
        device,
        provider::ProvisionInput {
            community: COMMUNITY.into(),
            owner: owner.public_key().to_hex(),
            host: scope_b.host.to_hex(),
            generation: scope_b.generation,
            runtime_id: "acp/claude".into(),
            name: "Alpha".into(),
            picture: None,
            instructions: None,
            effort: None,
        },
    )
    .is_err());

    // Runtime mismatch refused
    assert!(provider::provision_inner(
        &session,
        device,
        provider::ProvisionInput {
            community: COMMUNITY.into(),
            owner: owner.public_key().to_hex(),
            host: scope_a.host.to_hex(),
            generation: scope_a.generation,
            runtime_id: "acp/codex".into(),
            name: "Alpha".into(),
            picture: None,
            instructions: None,
            effort: None,
        },
    )
    .is_err());
}

#[tokio::test]
async fn agent_authenticated_kind0_publication_uses_agent_keys_and_nip_oa_auth_tag() {
    let owner = keys(1);
    let agent = keys(5);
    let identity = IdentityHost::fixture();
    let nip_oa = identity
        .authorize_agent(owner.public_key().to_hex(), agent.public_key().to_hex())
        .await
        .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let expected_agent = agent.public_key();
    let expected_oa = nip_oa.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        ws.send(json!(["AUTH", "ch1"]).to_string().into())
            .await
            .unwrap();
        let auth_raw = ws.next().await.unwrap().unwrap().into_text().unwrap();
        let auth_msg: Value = serde_json::from_str(&auth_raw).unwrap();
        assert_eq!(auth_msg[0], "AUTH");
        let auth_ev: Event = serde_json::from_value(auth_msg[1].clone()).unwrap();
        auth_ev.verify().unwrap();
        assert_eq!(auth_ev.pubkey, expected_agent);
        assert!(auth_ev
            .tags
            .iter()
            .any(|t| t.as_slice() == expected_oa.as_slice()));
        ws.send(json!(["OK", auth_ev.id, true, ""]).to_string().into())
            .await
            .unwrap();

        let ev_raw = ws.next().await.unwrap().unwrap().into_text().unwrap();
        let ev_msg: Value = serde_json::from_str(&ev_raw).unwrap();
        assert_eq!(ev_msg[0], "EVENT");
        let ev: Event = serde_json::from_value(ev_msg[1].clone()).unwrap();
        ev.verify().unwrap();
        assert_eq!(ev.kind.as_u16(), 0);
        assert_eq!(ev.pubkey, expected_agent);
        assert!(ev
            .tags
            .iter()
            .any(|t| t.as_slice() == expected_oa.as_slice()));
        let profile: Value = serde_json::from_str(&ev.content).unwrap();
        assert_eq!(profile["name"], "Alpha");
        assert_eq!(profile["display_name"], "Alpha");
        assert_eq!(profile["bot"], true);
        assert_eq!(profile["picture"], "https://example.test/a.png");
        ws.send(json!(["OK", ev.id, true, ""]).to_string().into())
            .await
            .unwrap();
    });

    let created = provider::publish_kind0_over_ws(
        &format!("ws://{addr}"),
        &agent,
        &nip_oa,
        "Alpha",
        Some("https://example.test/a.png"),
        100,
    )
    .await
    .unwrap();
    assert!(created >= 100);
    server.await.unwrap();
}

#[tokio::test]
async fn selected_model_activates_and_retries_without_redeploying() {
    hosted_lifecycle(false).await;
}

#[tokio::test]
async fn failed_first_launch_retries_on_the_original_identity() {
    hosted_lifecycle(true).await;
}

async fn hosted_lifecycle(requires_retry: bool) {
    use buzz_core::{
        agent_models::{AgentModelInfo, AgentModelsResponse},
        harness::{AcpAvailabilityStatus, Readiness, RuntimeFacts},
        host_deployment::{
            observation::{LoginState, Models, Observation, ResultBody},
            wire, Availability, Capability, DeploymentResult, Outcome, Status as HostStatus,
        },
    };
    let identity = IdentityHost::fixture();
    let owner = keys(1);
    let host = keys(2);
    let relay = keys(3);
    let session = Session::capture(&identity, owner.public_key().to_hex(), COMMUNITY.into())
        .await
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let mut journal = Journal::open(root.path()).unwrap();
    let device = journal.device.as_mut().unwrap();
    let scope = seed_paired_host(device, &owner, &host, COMMUNITY);

    let prov = provider::provision_inner(
        &session,
        device,
        provider::ProvisionInput {
            community: COMMUNITY.into(),
            owner: owner.public_key().to_hex(),
            host: scope.host.to_hex(),
            generation: scope.generation.clone(),
            runtime_id: "acp/claude".into(),
            name: "Netcup Agent".into(),
            picture: Some("https://example.test/a.png".into()),
            instructions: Some("You are helpful.".into()),
            effort: None,
        },
    )
    .unwrap();
    let agent_pk = PublicKey::from_hex(&prov.pubkey).unwrap();
    let nip_oa_1: [String; 4] = identity
        .authorize_agent(owner.public_key().to_hex(), prov.pubkey.clone())
        .await
        .unwrap()
        .try_into()
        .unwrap();

    // Kind 0 mock relay
    let kind0_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Event>::new()));
    let kind0_store = kind0_events.clone();
    let kind0_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let kind0_addr = kind0_listener.local_addr().unwrap();
    let kind0_srv = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = kind0_listener.accept().await else {
                break;
            };
            let store = kind0_store.clone();
            tokio::spawn(async move {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                ws.send(json!(["AUTH", "k0"]).to_string().into())
                    .await
                    .unwrap();
                while let Some(Ok(msg)) = ws.next().await {
                    if !msg.is_text() {
                        continue;
                    }
                    let Ok(txt) = msg.into_text() else { continue };
                    let frame: Value = serde_json::from_str(&txt).unwrap();
                    match frame[0].as_str().unwrap() {
                        "AUTH" => {
                            let ev: Event = serde_json::from_value(frame[1].clone()).unwrap();
                            ws.send(json!(["OK", ev.id, true, ""]).to_string().into())
                                .await
                                .unwrap();
                        }
                        "EVENT" => {
                            let ev: Event = serde_json::from_value(frame[1].clone()).unwrap();
                            let id = ev.id;
                            store.lock().unwrap().push(ev);
                            ws.send(json!(["OK", id, true, ""]).to_string().into())
                                .await
                                .unwrap();
                        }
                        _ => {}
                    }
                }
            });
        }
    });

    // 43104 mock relay
    let now_s = Timestamp::now().as_secs();
    let catalog_rev = "cd".repeat(32);
    let status = HostStatus {
        scope: scope.clone(),
        availability: Availability::Current,
        capability: Capability::Supported {
            catalog_revision: catalog_rev.clone(),
            runtimes: vec!["acp/claude".into()],
            harnesses: [("acp/claude".into(), "claude".into())].into(),
        },
        inventory: Some(
            [(
                "acp/claude".into(),
                RuntimeFacts {
                    availability: AcpAvailabilityStatus::Available,
                    version: Some("1.0.0".into()),
                    config_supported: true,
                },
            )]
            .into(),
        ),
        expires_at: now_s + 120,
    };
    let status_ev = wire::status(&host, &status, now_s).unwrap();
    let ops = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
        String,
        Value,
    >::new()));
    let ops_srv = ops.clone();
    let current_revision = std::sync::Arc::new(std::sync::Mutex::new(0u64));
    let rev_srv = current_revision.clone();
    let phase_mode = std::sync::Arc::new(std::sync::Mutex::new(
        if requires_retry {
            "needs-login"
        } else {
            "applied"
        }
        .to_string(),
    ));
    let phase_srv = phase_mode.clone();
    let owner_pk = owner.public_key();
    let relay_srv = relay.clone();
    let host_srv = host.clone();
    let scope_srv = scope.clone();
    let cat_rev_srv = catalog_rev.clone();
    let mgmt_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mgmt_addr = mgmt_listener.local_addr().unwrap();
    let mgmt_srv = tokio::spawn(async move {
        let (stream, _) = mgmt_listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        while let Some(Ok(msg)) = ws.next().await {
            if !msg.is_text() {
                continue;
            }
            let Ok(txt) = msg.into_text() else { continue };
            let frame: Value = serde_json::from_str(&txt).unwrap();
            match frame[0].as_str().unwrap() {
                "EVENT" => {
                    let ev: Event = serde_json::from_value(frame[1].clone()).unwrap();
                    let ev_str = serde_json::to_string(&ev).unwrap();
                    let (_, wire::Header::Command { authority }) =
                        wire::verify(ev_str.as_bytes(), COMMUNITY, owner_pk).unwrap()
                    else {
                        panic!("expected command");
                    };
                    if authority.action == buzz_core::host_deployment::Action::Deploy {
                        let buzz_core::host_deployment::Command::Deploy { config, .. } =
                            wire::open_command(&host_srv, &ev, &authority).unwrap()
                        else {
                            panic!("expected deployment")
                        };
                        assert_eq!(authority.agent, agent_pk);
                        assert_eq!(
                            config.model,
                            buzz_core::host_deployment::ModelSelection::Selected {
                                id: "claude-sonnet".into()
                            }
                        );
                    }
                    let mode = phase_srv.lock().unwrap().clone();
                    let t = Timestamp::now().as_secs();
                    let entry = match mode.as_str() {
                        "needs-login" => {
                            *rev_srv.lock().unwrap() = 1;
                            let body = ResultBody {
                                result: DeploymentResult {
                                    request: authority.clone(),
                                    outcome: Outcome::NeedsLogin,
                                    applied_revision: Some(1),
                                },
                                observation: Some(Observation {
                                    scope: scope_srv.clone(),
                                    agent: agent_pk,
                                    runtime_id: "acp/claude".into(),
                                    catalog_revision: cat_rev_srv.clone(),
                                    operation_id: authority.operation_id.clone(),
                                    revision: 1,
                                    action_id: "aa".repeat(32),
                                    observed_at: t,
                                    expires_at: t + 120,
                                    login: LoginState::NotObserved,
                                    login_at: None,
                                    readiness: Some(Readiness::LoggedIn),
                                    availability: Some(AcpAvailabilityStatus::Available),
                                    models: Models::Available {
                                        catalog: AgentModelsResponse {
                                            agent_name: "Claude".into(),
                                            agent_version: "1.0".into(),
                                            supports_switching: true,
                                            models: vec![AgentModelInfo {
                                                id: "claude-sonnet".into(),
                                                name: Some("Claude Sonnet".into()),
                                                description: None,
                                            }],
                                            agent_default_model: None,
                                            selected_model: None,
                                        },
                                    },
                                }),
                            };
                            let receipt = wire::result_body(&host_srv, &body, t).unwrap();
                            json!({
                                "event": ev_str,
                                "result": serde_json::to_string(&receipt).unwrap(),
                                "state": "needs-login",
                                "unit_id": "ef".repeat(32),
                                "revision": 1
                            })
                        }
                        "applied" => {
                            let revision = authority.expected_revision + 1;
                            *rev_srv.lock().unwrap() = revision;
                            let receipt = wire::result(
                                &host_srv,
                                &DeploymentResult {
                                    request: authority.clone(),
                                    outcome: Outcome::Applied,
                                    applied_revision: Some(revision),
                                },
                                t,
                            )
                            .unwrap();
                            json!({
                                "event": ev_str,
                                "result": serde_json::to_string(&receipt).unwrap(),
                                "state": "applied",
                                "unit_id": "ef".repeat(32),
                                "revision": revision
                            })
                        }
                        "cleaned" => {
                            let revision = authority.expected_revision + 1;
                            *rev_srv.lock().unwrap() = revision;
                            let receipt = wire::result(
                                &host_srv,
                                &DeploymentResult {
                                    request: authority.clone(),
                                    outcome: Outcome::Cleaned,
                                    applied_revision: Some(revision),
                                },
                                t,
                            )
                            .unwrap();
                            json!({
                                "event": ev_str,
                                "result": serde_json::to_string(&receipt).unwrap(),
                                "state": "cleaned",
                                "unit_id": "ef".repeat(32),
                                "revision": revision
                            })
                        }
                        other => panic!("unexpected mode {other}"),
                    };
                    assert!(
                        ops_srv
                            .lock()
                            .unwrap()
                            .insert(authority.operation_id.clone(), entry)
                            .is_none(),
                        "duplicate host effect"
                    );
                    ws.send(json!(["OK", ev.id, true, ""]).to_string().into())
                        .await
                        .unwrap();
                }
                "REQ" => {
                    let sub = frame[1].as_str().unwrap();
                    let q: wire::Query =
                        serde_json::from_value(frame[2]["host_deployment"].clone()).unwrap();
                    let data = match q.view.as_str() {
                        "status" => json!({ "event": serde_json::to_string(&status_ev).unwrap() }),
                        "operation" => {
                            let op = q.operation_id.as_deref().unwrap();
                            let latest_rev = *rev_srv.lock().unwrap();
                            ops_srv
                                .lock()
                                .unwrap()
                                .get(op)
                                .cloned()
                                .map(|mut entry| {
                                    entry["revision"] = json!(latest_rev);
                                    entry
                                })
                                .unwrap_or_else(|| json!({"state": "missing-or-superseded"}))
                        }
                        other => panic!("unexpected view {other}"),
                    };
                    let reply = wire::reply(
                        &relay_srv,
                        q.scope.owner,
                        &wire::Reply {
                            query: q,
                            expires_at: Timestamp::now().as_secs() + wire::PERMIT_SECONDS,
                            data,
                        },
                        Timestamp::now().as_secs(),
                    )
                    .unwrap();
                    ws.send(json!(["EVENT", sub, reply]).to_string().into())
                        .await
                        .unwrap();
                    ws.send(json!(["EOSE", sub]).to_string().into())
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }
    });

    let ws = NostrWsConnection::connect(&format!("ws://{mgmt_addr}"))
        .await
        .unwrap();
    let mut mgmt = buzz_paired_owner::deployment::Management::from_authenticated(
        true,
        COMMUNITY,
        owner.public_key(),
        relay.public_key(),
        ws,
    )
    .unwrap();
    let k0_url = format!("ws://{kind0_addr}");

    let input = |auth: [String; 4], model: Option<&str>| provider::ActivateInput {
        community: COMMUNITY.into(),
        owner: owner.public_key().to_hex(),
        pubkey: prov.pubkey.clone(),
        name: "Netcup Agent".into(),
        nip_oa: auth,
        host: Some(scope.host.to_hex()),
        generation: Some(scope.generation.clone()),
        runtime_id: Some("acp/claude".into()),
        model_id: model.map(str::to_owned),
        catalog_revision: Some(catalog_rev.clone()),
        effort: None,
    };
    // A pre-discovery saved request cannot publish a profile or start a model-free deployment.
    assert!(provider::activate_inner(
        &session,
        device,
        &mut mgmt,
        input(nip_oa_1.clone(), None),
        Some(&k0_url)
    )
    .await
    .is_err());
    assert!(kind0_events.lock().unwrap().is_empty());
    assert!(ops.lock().unwrap().is_empty());
    if requires_retry {
        let needs_login = provider::activate_inner(
            &session,
            device,
            &mut mgmt,
            input(nip_oa_1.clone(), Some("claude-sonnet")),
            Some(&k0_url),
        )
        .await
        .unwrap_err();
        assert!(needs_login.to_string().contains("needs-login"));
    }
    // Interrupt after the first command is retained (phase 0), or after its
    // continuation reaches phase 1. Reopen the real journal before retrying.
    let stopped_phase = u8::from(requires_retry);
    let fixture =
        rusqlite::Connection::open(root.path().join("registered-hosts-v1/owner.sqlite")).unwrap();
    fixture
        .execute_batch(&format!(
            "CREATE TRIGGER interrupt_phase BEFORE UPDATE OF data ON deployment_intents
        WHEN json_extract(NEW.data, '$.phase') = {} AND json_extract(OLD.data, '$.phase') = {}
        BEGIN SELECT RAISE(ABORT, 'fixture interrupted'); END;",
            stopped_phase + 1,
            stopped_phase
        ))
        .unwrap();
    let mut interrupted_input = input(nip_oa_1.clone(), Some("claude-sonnet"));
    if requires_retry {
        interrupted_input.effort = Some("high".into());
    }
    let interrupted = provider::activate_inner(
        &session,
        device,
        &mut mgmt,
        interrupted_input,
        Some(&k0_url),
    )
    .await
    .unwrap_err();
    assert!(
        interrupted.to_string().contains("fixture interrupted"),
        "{interrupted}"
    );
    fixture
        .execute_batch("DROP TRIGGER interrupt_phase")
        .unwrap();
    drop(fixture);
    let retained_id = device
        .reservation(COMMUNITY, &owner, &agent_pk)
        .unwrap()
        .unwrap()
        .operation_id
        .unwrap();
    let retained = device
        .deployment_intent(COMMUNITY, &owner, &retained_id)
        .unwrap()
        .unwrap();
    assert_eq!(retained.phase, stopped_phase);
    assert_eq!(
        retained.protected.authority.expected_revision,
        u64::from(requires_retry)
    );
    assert_eq!(ops.lock().unwrap().len(), usize::from(requires_retry));
    assert_eq!(kind0_events.lock().unwrap().len(), 1);
    drop(journal);
    let mut journal = Journal::open(root.path()).unwrap();
    let device = journal.device.as_mut().unwrap();
    let nip_oa_2: [String; 4] = identity
        .authorize_agent(owner.public_key().to_hex(), prov.pubkey.clone())
        .await
        .unwrap()
        .try_into()
        .unwrap();
    assert_ne!(nip_oa_2, nip_oa_1);
    *phase_mode.lock().unwrap() = "applied".into();
    provider::activate_inner(
        &session,
        device,
        &mut mgmt,
        input(nip_oa_2, Some("claude-sonnet")),
        Some(&k0_url),
    )
    .await
    .unwrap();
    assert_eq!(
        ops.lock().unwrap().len(),
        if requires_retry { 2 } else { 1 }
    );
    assert_eq!(kind0_events.lock().unwrap().len(), 1);
    assert_eq!(ops.lock().unwrap()[&retained_id]["event"], retained.event);
    assert_eq!(
        device.deployment_intents(COMMUNITY, &owner).unwrap().len(),
        if requires_retry { 2 } else { 1 }
    );
    assert_eq!(
        device
            .reservation(COMMUNITY, &owner, &agent_pk)
            .unwrap()
            .unwrap()
            .config
            .effort
            .as_deref(),
        requires_retry.then_some("high")
    );
    // 3. Profile rename publishes agent-signed kind 0 without redeploy
    provider::profile_inner(
        &session,
        device,
        provider::ProfileInput {
            community: COMMUNITY.into(),
            owner: owner.public_key().to_hex(),
            pubkey: prov.pubkey.clone(),
            name: "Renamed Netcup".into(),
            picture: Some("https://example.test/b.png".into()),
        },
        Some(&k0_url),
    )
    .await
    .unwrap();
    assert_eq!(kind0_events.lock().unwrap().len(), 2);

    // 4. Remove cleans up and retains retired tombstone
    *phase_mode.lock().unwrap() = "cleaned".into();
    provider::remove_inner(&session, device, None, Some(&mut mgmt), &prov.pubkey)
        .await
        .unwrap();
    let retired = device
        .reservation(COMMUNITY, &owner, &agent_pk)
        .unwrap()
        .unwrap();
    assert_eq!(
        retired.state,
        buzz_paired_owner::deployment::ReservationState::Retired
    );
    assert!(retired.agent_nsec.is_none());
    assert!(retired.nip_oa.is_none());

    // Subsequent activate/profile refused after retirement
    assert!(provider::profile_inner(
        &session,
        device,
        provider::ProfileInput {
            community: COMMUNITY.into(),
            owner: owner.public_key().to_hex(),
            pubkey: prov.pubkey.clone(),
            name: "Late".into(),
            picture: None,
        },
        Some(&k0_url),
    )
    .await
    .is_err());

    drop(mgmt);
    mgmt_srv.abort();
    kind0_srv.abort();
}

#[test]
fn provision_result_omits_none_picture_field() {
    let omitted = serde_json::to_value(provider::ProvisionResult {
        pubkey: "aa".repeat(32),
        picture: None,
    })
    .unwrap();
    assert!(omitted.get("picture").is_none());
    let present = serde_json::to_value(provider::ProvisionResult {
        pubkey: "aa".repeat(32),
        picture: Some("https://example.test/a.png".into()),
    })
    .unwrap();
    assert_eq!(present["picture"], "https://example.test/a.png");
}

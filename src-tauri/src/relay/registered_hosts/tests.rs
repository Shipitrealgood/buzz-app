use super::*;
use buzz_core::host_registry::{self as wire, Body, Query, Reply};
use buzz_paired_owner::{registry::RegistryBinding, Scope};
use futures_util::{SinkExt, StreamExt};
use nostr_registry::{Event, Timestamp};
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
    let response = get_ipc_response(
        &view,
        tauri::webview::InvokeRequest {
            cmd: "registered_host_recover".into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: view.url().unwrap(),
            body: tauri::ipc::InvokeBody::Json(
                json!({ "community": COMMUNITY, "owner": keys(2).public_key().to_hex() }),
            ),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    );
    assert_eq!(response.unwrap_err(), json!(UNAVAILABLE));
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

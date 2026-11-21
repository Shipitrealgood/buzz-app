//! Explicit registry recovery only. Registration is not runtime availability.
use super::{origin, relay_info, IdentityHost};
use crate::federated_identity::{assertion::HEADER, FederatedIdentity};
use anyhow::{anyhow, ensure, Result};
use buzz_paired_owner::{registry::Registry, Device, HostView};
use buzz_ws_client::{HeaderMap, NostrWsConnection};
use nostr_registry::{Keys, SecretKey};
use serde::Serialize;
use std::{fs, path::Path, time::Duration};
use tauri::Manager;

const UNAVAILABLE: &str = "Registered host recovery is unavailable";

/// Only public registration evidence crosses IPC. No runtime catalog, key,
/// provisioning request or local-runner record is exposed by this checkpoint.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegisteredHost {
    community: String,
    owner: String,
    host: String,
    generation: String,
    label: Option<String>,
    registration: String,
    availability: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Directory {
    community: String,
    owner: String,
    registry: &'static str,
    hosts: Vec<RegisteredHost>,
}

fn project(host: HostView) -> RegisteredHost {
    RegisteredHost {
        community: host.scope.community,
        owner: host.scope.owner.to_hex(),
        host: host.scope.host.to_hex(),
        generation: host.scope.generation,
        label: host.label,
        registration: if host.state == "paired" {
            "registered".into()
        } else {
            host.state
        },
        // This read never sends a host status challenge. Cached signed labels
        // cannot establish that a runtime is currently reachable or ready.
        availability: "unavailable",
    }
}

struct Session<'a> {
    identity: &'a IdentityHost,
    owner: String,
    community: String,
}

impl<'a> Session<'a> {
    async fn capture(identity: &'a IdentityHost, owner: String, community: String) -> Result<Self> {
        let canonical = origin(&community)
            .map_err(anyhow::Error::msg)?
            .origin()
            .ascii_serialization();
        ensure!(community == canonical, "Invalid community origin");
        ensure!(
            identity.viewer().await.map_err(anyhow::Error::msg)? == owner,
            "Active identity changed"
        );
        let session = Self {
            identity,
            owner,
            community,
        };
        session.keys()?;
        Ok(session)
    }

    fn keys(&self) -> Result<Keys> {
        self.with_keys(Ok)
    }

    fn with_keys<T>(&self, action: impl FnOnce(Keys) -> Result<T>) -> Result<T> {
        self.identity
            .with_ready_key(&self.owner, |secret| {
                // Borrow the app's zeroizing key bytes without a string bridge.
                // The shared SecretKey erases its scalar on drop; its owner
                // rechecks this guard after network waits.
                SecretKey::from_slice(secret)
                    .map(Keys::new)
                    .map_err(|_| UNAVAILABLE.to_owned())
                    .and_then(|keys| action(keys).map_err(|_| UNAVAILABLE.to_owned()))
            })
            .map_err(anyhow::Error::msg)
    }

    fn directory(&self, device: &Device, registry: &'static str) -> Result<Directory> {
        let keys = self.keys()?;
        let hosts = device
            .registry_hosts(&self.community, &keys)?
            .into_iter()
            .map(project)
            .collect();
        Ok(Directory {
            community: self.community.clone(),
            owner: self.owner.clone(),
            registry,
            hosts,
        })
    }

    async fn recover(&self, fi: &FederatedIdentity, device: &mut Device) -> Result<&'static str> {
        // Reuse bounded native HTTP discovery and FI authorization. Registry's
        // convenience connect method does not carry the app's FI assertion.
        let url = origin(&self.community).map_err(anyhow::Error::msg)?;
        let metadata = relay_info(&self.community)
            .await
            .map_err(anyhow::Error::msg)?;
        self.keys()?;
        if metadata["host_registry"].is_null() {
            return Ok("unsupported");
        }
        let relay = Registry::capability(&metadata)?;
        let authorization = fi
            .assertion_for_relay(self.identity, &url)
            .await
            .map_err(anyhow::Error::msg)?;
        self.keys()?;
        let operation = async {
            let mut headers = HeaderMap::new();
            if let Some(header) = authorization.header() {
                headers.insert(HEADER, header.parse()?);
            }
            let socket_url = self.community.replacen("https://", "wss://", 1);
            let mut socket = tokio::time::timeout(
                Duration::from_secs(10),
                NostrWsConnection::connect_with_headers(&socket_url, headers),
            )
            .await??;
            self.authenticate(&mut socket).await?;
            let mut registry = Registry::from_authenticated(
                &self.community,
                self.keys()?.public_key(),
                relay,
                socket,
            )?;
            // Never synchronize: that method also resends durable mutations.
            self.recover_registry(&mut registry, device).await?;
            Ok::<_, anyhow::Error>("available")
        };
        fi.during_authorization(self.identity, &authorization, operation)
            .await
            .map_err(anyhow::Error::msg)?
    }

    async fn recover_registry(&self, registry: &mut Registry, device: &mut Device) -> Result<()> {
        registry.recover_guarded(device, &|| self.keys()).await?;
        self.keys()?;
        Ok(())
    }

    async fn authenticate(&self, socket: &mut NostrWsConnection) -> Result<()> {
        socket
            .authenticate_with_signer(|challenge, url| {
                // Hold the existing identity lock through signing, so logout
                // cannot retire the key between admission and the signature.
                self.with_keys(|keys| {
                    Ok(buzz_ws_client::build_auth_event(
                        challenge, url, &keys, None,
                    )?)
                })
                .map_err(|_| {
                    buzz_ws_client::WsClientError::AuthFailed("Active identity changed".into())
                })
            })
            .await?;
        self.keys()?;
        Ok(())
    }
}

/// One OS file lock covers the entire recovery and its journal effects,
/// including another process. Dropping this owner closes SQLite before unlock.
struct Journal {
    device: Option<Device>,
    lock: fs::File,
}
impl Drop for Journal {
    fn drop(&mut self) {
        self.device.take();
        let _ = fs2::FileExt::unlock(&self.lock);
    }
}

fn plain(path: &Path, directory: bool) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink()
            && if directory {
                metadata.is_dir()
            } else {
                metadata.is_file()
            },
        "Invalid journal path"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.uid() == nix::unistd::Uid::effective().as_raw()
                && metadata.mode() & 0o077 == 0
                && (directory || metadata.nlink() == 1),
            "Journal is not private"
        );
    }
    Ok(())
}

impl Journal {
    fn open(base: &Path) -> Result<Self> {
        fs::create_dir_all(base)?;
        let base = dunce::canonicalize(base)?;
        let root = base.join("registered-hosts-v1");
        #[cfg(windows)]
        buzz_paired_owner::protect_windows_journal(&base, &root)?;
        #[cfg(not(windows))]
        {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&root) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        plain(&root, true)?;
        let open = |path: &Path| -> Result<fs::File> {
            let mut options = fs::OpenOptions::new();
            options.read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let file = match options.create_new(true).open(path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    plain(path, false)?;
                    options.create_new(false).open(path)?
                }
                Err(error) => return Err(error.into()),
            };
            plain(path, false)?;
            Ok(file)
        };
        // The shared Windows ACL owner intentionally permits only journal
        // files inside its directory. Keep this non-secret process lock beside
        // that directory so a later process can validate/reopen it unchanged.
        let lock = open(&base.join("registered-hosts-v1.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .map_err(|_| anyhow!("Registered host recovery is busy"))?;
        let mut journal = Self { device: None, lock };
        drop(open(&root.join("owner.sqlite"))?);
        journal.device = Some(Device::at(&root.join("owner.sqlite"))?);
        Ok(journal)
    }
}

#[tauri::command]
pub(crate) async fn registered_host_recover<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    identity: tauri::State<'_, IdentityHost>,
    fi: tauri::State<'_, FederatedIdentity>,
    community: String,
    owner: String,
) -> Result<Directory, String> {
    let operation = async {
        let session = Session::capture(&identity, owner, community).await?;
        let mut journal = Journal::open(&app.path().app_local_data_dir()?)?;
        let device = journal
            .device
            .as_mut()
            .ok_or_else(|| anyhow!(UNAVAILABLE))?;
        let registry = session.recover(&fi, device).await.unwrap_or("unavailable");
        // Even failure/cached results must be fenced to the native account.
        session.directory(device, registry)
    };
    operation.await.map_err(|_| UNAVAILABLE.into())
}

#[cfg(test)]
mod tests;

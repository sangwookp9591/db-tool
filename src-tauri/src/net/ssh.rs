use super::{
    model::*,
    proxy::{self, BoxStream},
    Event, State,
};
use dbcore::DbError;
use russh::{
    client,
    keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

#[derive(Debug)]
pub struct SshError(pub String);
impl From<russh::Error> for SshError {
    fn from(_: russh::Error) -> Self {
        Self("SSH_TRANSPORT_FAILED".into())
    }
}
impl std::fmt::Display for SshError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for SshError {}
struct Handler {
    hop: SshHop,
    known_hosts: PathBuf,
    state: Arc<State>,
}
impl client::Handler for Handler {
    type Error = SshError;
    async fn check_server_key(
        &mut self,
        server: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        if server.certificate().is_some() {
            return Err(SshError("SSH_CERTIFICATE_UNSUPPORTED".into()));
        }
        let key = server.public_key();
        let public = key
            .to_openssh()
            .map_err(|_| SshError("SSH_HOST_KEY_INVALID".into()))?;
        let host = if self.hop.port == 22 {
            self.hop.host.clone()
        } else {
            format!("[{}]:{}", self.hop.host, self.hop.port)
        };
        {
            let _guard = self.state.known_hosts.lock().await;
            if known(&self.known_hosts, &host, &public).map_err(|e| SshError(e.message))? {
                return Ok(true);
            }
        }
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        self.state
            .confirm(self.hop.host.clone(), self.hop.port, fingerprint)
            .await
            .map_err(|e| SshError(e.message))?;
        let _guard = self.state.known_hosts.lock().await;
        if !known(&self.known_hosts, &host, &public).map_err(|e| SshError(e.message))? {
            let mut data = std::fs::read_to_string(&self.known_hosts).unwrap_or_default();
            data.push_str(&format!("{host} {public}\n"));
            crate::storage::write_private(&self.known_hosts, data.as_bytes())
                .map_err(|_| SshError("SSH_KNOWN_HOSTS_WRITE_FAILED".into()))?;
        }
        Ok(true)
    }
}
fn known(path: &Path, host: &str, key: &str) -> Result<bool, DbError> {
    let text = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(error("SSH_KNOWN_HOSTS_UNREADABLE")),
    };
    for line in text.lines() {
        if let Some((h, k)) = line.split_once(' ')
            && h == host
        {
            let mut saved = k.split_whitespace();
            let mut supplied = key.split_whitespace();
            if saved.next() == supplied.next() && saved.next() == supplied.next() {
                return Ok(true);
            }
            return Err(error("SSH_HOST_KEY_CHANGED"));
        }
    }
    Ok(false)
}
pub struct Connection {
    handles: Vec<client::Handle<Handler>>,
}
impl Connection {
    pub fn closed(&self) -> bool {
        self.handles.iter().any(|h| h.is_closed())
    }
    pub async fn open(&self, host: &str, port: u16) -> Result<BoxStream, DbError> {
        let h = self
            .handles
            .last()
            .ok_or_else(|| error("SSH_TRANSPORT_FAILED"))?;
        let channel = h
            .channel_open_direct_tcpip(host, u32::from(port), "127.0.0.1", 0)
            .await
            .map_err(|_| error("SSH_FORWARD_REJECTED"))?;
        Ok(Box::new(channel.into_stream()))
    }
}
pub fn expand_path(path: &str) -> Result<PathBuf, DbError> {
    if let Some(relative) = path.strip_prefix("~/") {
        Ok(super::iam::home()?.join(relative))
    } else {
        Ok(path.into())
    }
}
fn private_key(path: &str, passphrase: Option<&str>) -> Result<russh::keys::PrivateKey, DbError> {
    let path = expand_path(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .map_err(|_| error("SSH_KEY_UNREADABLE"))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(error("SSH_KEY_PERMISSIONS"));
        }
    }
    russh::keys::load_secret_key(path, passphrase).map_err(|_| error("SSH_KEY_DECRYPT_FAILED"))
}
async fn authenticate(
    handle: &mut client::Handle<Handler>,
    hop: &SshHop,
    password: Option<&str>,
    passphrase: Option<&str>,
) -> Result<(), DbError> {
    let success = match hop.auth {
        SshAuth::Password => handle
            .authenticate_password(
                &hop.user,
                password.ok_or_else(|| error("SSH_PASSWORD_REQUIRED"))?,
            )
            .await
            .map_err(|_| error("SSH_AUTH_FAILED"))?
            .success(),
        SshAuth::Key => {
            let key = private_key(
                hop.key_file
                    .as_deref()
                    .ok_or_else(|| error("SSH_KEY_REQUIRED"))?,
                passphrase,
            )?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|_| error("SSH_AUTH_FAILED"))?
                .flatten();
            handle
                .authenticate_publickey(&hop.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map_err(|_| error("SSH_AUTH_FAILED"))?
                .success()
        }
        SshAuth::Agent => {
            #[cfg(any(unix, windows))]
            {
                #[cfg(unix)]
                let mut agent = russh::keys::agent::client::AgentClient::connect_env()
                    .await
                    .map_err(|_| error("SSH_AGENT_UNAVAILABLE"))?;
                #[cfg(windows)]
                let mut agent = russh::keys::agent::client::AgentClient::connect_named_pipe(
                    r"\\.\pipe\openssh-ssh-agent",
                )
                .await
                .map_err(|_| error("SSH_AGENT_UNAVAILABLE"))?;
                let identities = agent
                    .request_identities()
                    .await
                    .map_err(|_| error("SSH_AGENT_UNAVAILABLE"))?;
                let hash = handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|_| error("SSH_AUTH_FAILED"))?
                    .flatten();
                let mut success = false;
                for identity in identities {
                    if handle
                        .authenticate_publickey_with(
                            &hop.user,
                            identity.public_key().into_owned(),
                            hash,
                            &mut agent,
                        )
                        .await
                        .map_err(|_| error("SSH_AUTH_FAILED"))?
                        .success()
                    {
                        success = true;
                        break;
                    }
                }
                success
            }
            #[cfg(not(any(unix, windows)))]
            {
                return Err(error("SSH_AGENT_UNAVAILABLE"));
            }
        }
    };
    if success {
        Ok(())
    } else {
        Err(error("SSH_AUTH_REJECTED"))
    }
}
pub async fn connect(
    ssh: &Ssh,
    proxy: Option<&Proxy>,
    profile_id: &str,
    path: &Path,
    state: Arc<State>,
) -> Result<Connection, DbError> {
    let mut connection = Connection {
        handles: Vec::new(),
    };
    let hops = ssh.hops();
    let proxy_password = crate::storage::keychain_get(&format!("{profile_id}:proxy_password"))?;
    for (index, hop) in hops.iter().enumerate() {
        let stream = if index == 0 {
            tokio::time::timeout(
                Duration::from_secs(10),
                proxy::connect(proxy, proxy_password.as_deref(), &hop.host, hop.port),
            )
            .await
            .map_err(|_| error("SSH_TIMEOUT"))??
        } else {
            connection.open(&hop.host, hop.port).await?
        };
        let handler = Handler {
            hop: hop.clone(),
            known_hosts: path.into(),
            state: state.clone(),
        };
        let config = client::Config {
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            nodelay: true,
            ..Default::default()
        };
        let mut handle = tokio::time::timeout(
            Duration::from_secs(75),
            client::connect_stream(Arc::new(config), stream, handler),
        )
        .await
        .map_err(|_| error("SSH_TIMEOUT"))?
        .map_err(|e| error(&e.0))?;
        let prefix = if index == hops.len() - 1 {
            profile_id.to_owned()
        } else {
            format!("{profile_id}:jump:{index}")
        };
        let password = crate::storage::keychain_get(&format!("{prefix}:ssh_password"))?;
        let passphrase = crate::storage::keychain_get(&format!("{prefix}:key_passphrase"))?;
        tokio::time::timeout(
            Duration::from_secs(15),
            authenticate(&mut handle, hop, password.as_deref(), passphrase.as_deref()),
        )
        .await
        .map_err(|_| error("SSH_AUTH_TIMEOUT"))??;
        connection.handles.push(handle);
    }
    state.emit(Event::Network {
        profile_id: profile_id.into(),
        state: "connected".into(),
        attempt: 0,
        code: None,
    });
    Ok(connection)
}
#[cfg(test)]
impl Connection {
    pub async fn disconnect_for_test(&self) {
        for handle in &self.handles {
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "test", "en")
                .await;
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            while !self.closed() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins_host_keys_and_rejects_changes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("known_hosts");
        assert!(!known(&p, "[test]:22", "ssh-ed25519 first").unwrap());
        crate::storage::write_private(&p, b"[test]:22 ssh-ed25519 first\n").unwrap();
        assert!(known(&p, "[test]:22", "ssh-ed25519 first").unwrap());
        assert_eq!(
            known(&p, "[test]:22", "ssh-rsa changed")
                .unwrap_err()
                .message,
            "SSH_HOST_KEY_CHANGED"
        );
    }
    #[cfg(unix)]
    #[test]
    fn refuses_readable_private_key() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("key");
        std::fs::write(&p, "invalid").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            private_key(p.to_str().unwrap(), None)
                .err()
                .unwrap()
                .message,
            "SSH_KEY_PERMISSIONS"
        );
    }
}

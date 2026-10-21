use dbcore::{DbError, ErrorKind};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Network {
    pub ssh: Option<Ssh>,
    pub proxy: Option<Proxy>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ssh {
    pub host: String,
    #[serde(default = "ssh_port")]
    pub port: u16,
    pub user: String,
    pub auth: SshAuth,
    pub key_file: Option<String>,
    #[serde(default)]
    pub jump_hosts: Vec<SshHop>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SshHop {
    pub host: String,
    #[serde(default = "ssh_port")]
    pub port: u16,
    pub user: String,
    pub auth: SshAuth,
    pub key_file: Option<String>,
}
impl Ssh {
    pub fn hops(&self) -> Vec<SshHop> {
        let mut hops = self.jump_hosts.clone();
        hops.push(SshHop {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            auth: self.auth.clone(),
            key_file: self.key_file.clone(),
        });
        hops
    }
}
fn ssh_port() -> u16 {
    22
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuth {
    Password,
    Key,
    Agent,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proxy {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProxyKind {
    Socks5,
    HttpConnect,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Auth {
    #[default]
    Password,
    IamAws {
        profile: String,
        #[serde(default)]
        region: String,
    },
    IamGcp {
        adc_file: Option<String>,
    },
}
// This input is never serialized into a profile or an event.
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Secrets {
    pub ssh_password: Option<String>,
    pub key_passphrase: Option<String>,
    pub proxy_password: Option<String>,
    #[serde(default)]
    pub jumps: Vec<HopSecrets>,
}
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HopSecrets {
    pub ssh_password: Option<String>,
    pub key_passphrase: Option<String>,
}

pub fn error(code: &str) -> DbError {
    let mut e = DbError::new(ErrorKind::Connection, code);
    e.detail = Some(format!("connection.help.{}", code.to_ascii_lowercase()));
    e
}
pub fn valid_host(host: &str) -> Result<(), DbError> {
    if host.is_empty()
        || host.len() > 253
        || host
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '@' | '?' | '#'))
    {
        return Err(error("INVALID_HOST"));
    }
    Ok(())
}
impl Network {
    pub fn validate(&self) -> Result<(), DbError> {
        if let Some(ssh) = &self.ssh {
            if ssh.jump_hosts.len() > 8 {
                return Err(error("TOO_MANY_JUMPS"));
            }
            for hop in ssh.hops() {
                valid_host(&hop.host)?;
                if hop.port == 0 || hop.user.is_empty() || hop.user.chars().any(char::is_control) {
                    return Err(error("INVALID_SSH_CONFIG"));
                }
                if matches!(hop.auth, SshAuth::Key)
                    && hop.key_file.as_ref().is_none_or(|p| p.is_empty())
                {
                    return Err(error("SSH_KEY_REQUIRED"));
                }
            }
        }
        if let Some(proxy) = &self.proxy {
            valid_host(&proxy.host)?;
            if proxy.port == 0
                || proxy
                    .user
                    .as_ref()
                    .is_some_and(|u| u.chars().any(char::is_control))
            {
                return Err(error("INVALID_PROXY_CONFIG"));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_secrets_boundary() {
        let network: Network =
            serde_json::from_str(r#"{"ssh":{"host":"bastion","user":"dev","auth":"agent"}}"#)
                .unwrap();
        assert_eq!(network.ssh.as_ref().unwrap().port, 22);
        network.validate().unwrap();
        assert!(
            serde_json::from_str::<Network>(
                r#"{"ssh":{"host":"b","user":"u","auth":"password","password":"leak"}}"#
            )
            .is_err()
        );
        assert!(valid_host("host\r\nAuthorization: leak").is_err());
        assert!(
            serde_json::from_str::<Auth>(
                r#"{"kind":"iam_aws","profile":"test","region":"us-east-1","secret":"leak"}"#
            )
            .is_err()
        );
    }
}

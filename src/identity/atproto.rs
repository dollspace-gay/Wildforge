//! Optional AT Protocol account linking and public device-binding proof.
//!
//! OAuth is completed only by the client. A game server receives a DID and a
//! public repository-record key, resolves the DID's PDS, fetches that record,
//! and then relies on the ordinary Wildforge challenge to prove possession of
//! the bound device key. OAuth access/refresh tokens never enter game packets.

use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use jacquard::common::types::ident::AtIdentifier;
use jacquard::common::types::nsid::Nsid;
use jacquard::common::types::recordkey::RecordKey;
use jacquard::common::types::value::to_data;
use jacquard::common::xrpc::XrpcClient;
use jacquard::common::xrpc::atproto::{DeleteRecord, PutRecord};
use jacquard::oauth::atproto::AtprotoClientMetadata;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::{LoopbackConfig, LoopbackPort};
use jacquard::oauth::scopes::Scopes;
use jacquard::oauth::session::ClientData;
use jacquard::oauth::types::AuthorizeOptions;
use serde::{Deserialize, Serialize};

use super::{AtprotoDid, DeviceKeyId, atomic_write, encode_hex};
use crate::net::AtprotoClaim;

/// Owned by dollspace.gay. The corresponding Lexicon document is checked in
/// under `lexicons/`; PDS validation is disabled until it is published.
pub const BINDING_COLLECTION: &str = "gay.dollspace.wildforge.device";
const RECORD_TYPE: &str = "gay.dollspace.wildforge.device";
const MAX_IDENTITY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtprotoAccount {
    pub did: AtprotoDid,
    pub handle: Option<String>,
    pub binding: String,
    pub linked_at: u64,
    #[serde(default)]
    pub use_social_display_name: bool,
    #[serde(default)]
    pub use_social_avatar: bool,
    pub profile_display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl AtprotoAccount {
    pub fn load(root: &Path) -> io::Result<Option<Self>> {
        let path = root.join("atproto.toml");
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let account: Self = toml::from_str(&text).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid account link: {e}"),
                    )
                })?;
                // Re-parse to reject hand-edited values that bypass the newtype's constructor.
                AtprotoDid::parse(account.did.as_str())
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Some(account))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        atomic_write(&root.join("atproto.toml"), text.as_bytes(), false)
    }

    pub fn claim(&self) -> AtprotoClaim {
        AtprotoClaim {
            did: self.did.to_string(),
            binding: self.binding.clone(),
        }
    }

    pub fn unlink_local(root: &Path) -> io::Result<()> {
        match std::fs::remove_file(root.join("atproto.toml")) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceBinding<'a> {
    #[serde(rename = "$type")]
    record_type: &'static str,
    device_public_key: &'a str,
    created_at: u64,
    label: &'a str,
}

/// Complete one browser/loopback OAuth flow, write the device record, discard
/// the session, and persist only non-secret account metadata.
pub fn link_account(root: &Path, input: &str, public_key: [u8; 32]) -> io::Result<AtprotoAccount> {
    let input = input.trim();
    if input.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "enter an ATProto handle, DID, or PDS",
        ));
    }
    std::fs::create_dir_all(root)?;
    let token_path = root.join("atproto-oauth.json");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let store = jacquard::client::FileAuthStore::try_new(&token_path)
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        restrict_secret(&token_path)?;
        let scopes = Scopes::new(jacquard::SmolStr::new(format!(
            "atproto repo:{BINDING_COLLECTION}"
        )))
        .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        let oauth = OAuthClient::new(
            store,
            ClientData {
                keyset: None,
                config: AtprotoClientMetadata::new_localhost(None, Some(scopes)),
            },
            reqwest::Client::new(),
        );
        let session = oauth
            .login_with_local_server(
                input,
                AuthorizeOptions::default(),
                LoopbackConfig {
                    port: LoopbackPort::Ephemeral,
                    ..LoopbackConfig::default()
                },
            )
            .await
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        let (did, _) = session.session_info().await;
        let did_text = did.to_string();
        let parsed_did = AtprotoDid::parse(&did_text)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let device_id = DeviceKeyId::of_public_key(&public_key);
        let rkey = format!("device-{device_id}");
        let key_text = encode_hex(&public_key);
        let record = to_data(&DeviceBinding {
            record_type: RECORD_TYPE,
            device_public_key: &key_text,
            created_at: now(),
            label: "Wildforge device",
        })
        .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        let response = session
            .send(PutRecord {
                collection: Nsid::new_owned(BINDING_COLLECTION)
                    .map_err(|e| io::Error::other(e.to_string()))?,
                record,
                repo: AtIdentifier::Did(did),
                rkey: RecordKey::any_owned(&rkey).map_err(|e| io::Error::other(e.to_string()))?,
                swap_commit: None,
                swap_record: None,
                // The checked-in Lexicon may not be published yet. The record
                // remains ordinary public repo data and is validated locally.
                validate: Some(false),
            })
            .await
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        response
            .into_output()
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        let profile = fetch_public_profile(&did_text).await.ok();
        let _ = session.logout().await;
        Ok::<_, io::Error>(AtprotoAccount {
            did: parsed_did,
            handle: profile.as_ref().and_then(|value| value.handle.clone()),
            binding: rkey,
            linked_at: now(),
            use_social_display_name: false,
            use_social_avatar: false,
            profile_display_name: profile
                .as_ref()
                .and_then(|value| value.display_name.clone()),
            avatar_url: profile.and_then(|value| value.avatar),
        })
    });
    let _ = std::fs::remove_file(&token_path);
    let account = result?;
    account.save(root)?;
    Ok(account)
}

/// Re-authenticate, delete this device's repository record, and remove local
/// metadata. If the remote delete fails, local unlink is not reported as a
/// revocation; callers may explicitly choose `unlink_local` instead.
pub fn revoke_account(root: &Path, input: &str, account: &AtprotoAccount) -> io::Result<()> {
    let token_path = root.join("atproto-oauth.json");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let store = jacquard::client::FileAuthStore::try_new(&token_path)
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        restrict_secret(&token_path)?;
        let scopes = Scopes::new(jacquard::SmolStr::new(format!(
            "atproto repo:{BINDING_COLLECTION}"
        )))
        .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        let oauth = OAuthClient::new(
            store,
            ClientData {
                keyset: None,
                config: AtprotoClientMetadata::new_localhost(None, Some(scopes)),
            },
            reqwest::Client::new(),
        );
        let session = oauth
            .login_with_local_server(
                input,
                AuthorizeOptions::default(),
                LoopbackConfig {
                    port: LoopbackPort::Ephemeral,
                    ..LoopbackConfig::default()
                },
            )
            .await
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        let (did, _) = session.session_info().await;
        if did.to_string() != account.did.to_string() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the approved account does not match the linked DID",
            ));
        }
        let response = session
            .send(DeleteRecord {
                collection: Nsid::new_owned(BINDING_COLLECTION)
                    .map_err(|e| io::Error::other(e.to_string()))?,
                repo: AtIdentifier::Did(did),
                rkey: RecordKey::any_owned(&account.binding)
                    .map_err(|e| io::Error::other(e.to_string()))?,
                swap_commit: None,
                swap_record: None,
            })
            .await
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        response
            .into_output()
            .map_err(|e| io::Error::other(redact(&e.to_string())))?;
        let _ = session.logout().await;
        Ok::<_, io::Error>(())
    });
    let _ = std::fs::remove_file(token_path);
    result?;
    AtprotoAccount::unlink_local(root)
}

#[derive(Clone, Debug)]
pub struct VerifiedBinding {
    pub did: AtprotoDid,
    pub cached: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedBinding {
    did: String,
    binding: String,
    public_key: String,
    verified_at: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    #[serde(default)]
    proof: Vec<CachedBinding>,
}

pub struct ProofCache {
    path: PathBuf,
    grace_secs: u64,
    entries: Mutex<Vec<CachedBinding>>,
}

impl ProofCache {
    pub fn load(path: PathBuf, grace_secs: u64) -> io::Result<Self> {
        let cache = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str::<CacheFile>(&text).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid proof cache: {e}"),
                )
            })?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => CacheFile::default(),
            Err(e) => return Err(e),
        };
        if cache.version > 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported ATProto proof cache version",
            ));
        }
        Ok(Self {
            path,
            grace_secs,
            entries: Mutex::new(cache.proof),
        })
    }

    pub async fn verify(
        &self,
        claim: &AtprotoClaim,
        public_key: &[u8; 32],
    ) -> Result<VerifiedBinding, String> {
        let did = AtprotoDid::parse(&claim.did).map_err(|e| e.to_string())?;
        let key = encode_hex(public_key);
        match verify_live(&did, &claim.binding, &key).await {
            Ok(()) => {
                let mut entries = self.entries.lock().map_err(|_| "proof cache poisoned")?;
                entries.retain(|entry| {
                    !(entry.did == did.as_str()
                        && entry.binding == claim.binding
                        && entry.public_key == key)
                });
                entries.push(CachedBinding {
                    did: did.to_string(),
                    binding: claim.binding.clone(),
                    public_key: key,
                    verified_at: now(),
                });
                entries.retain(|entry| now().saturating_sub(entry.verified_at) <= self.grace_secs);
                if entries.len() > 512 {
                    let remove = entries.len() - 512;
                    entries.drain(..remove);
                }
                save_cache(&self.path, &entries).map_err(|e| e.to_string())?;
                Ok(VerifiedBinding { did, cached: false })
            }
            Err(live_error) => {
                let entries = self.entries.lock().map_err(|_| "proof cache poisoned")?;
                if has_cached_proof(&entries, &did, &claim.binding, &key, self.grace_secs, now()) {
                    Ok(VerifiedBinding { did, cached: true })
                } else {
                    Err(live_error)
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BindingValue {
    #[serde(rename = "$type")]
    record_type: String,
    device_public_key: String,
}

#[derive(Deserialize)]
struct GetRecordEnvelope {
    value: BindingValue,
}

async fn verify_live(did: &AtprotoDid, binding: &str, key: &str) -> Result<(), String> {
    RecordKey::<jacquard::common::types::recordkey::Rkey>::any_owned(binding)
        .map_err(|_| "invalid ATProto binding record key")?;
    let document_url = did_document_url(did)?;
    let document: serde_json::Value = get_public_limited_json(document_url).await?;
    if document.get("id").and_then(|value| value.as_str()) != Some(did.as_str()) {
        return Err("resolved DID document has the wrong id".into());
    }
    let endpoint = pds_endpoint(&document).ok_or("DID document has no ATProto PDS service")?;
    let mut endpoint = reqwest::Url::parse(&endpoint).map_err(|_| "invalid PDS endpoint")?;
    endpoint.set_path("/xrpc/com.atproto.repo.getRecord");
    endpoint.set_query(None);
    endpoint
        .query_pairs_mut()
        .append_pair("repo", did.as_str())
        .append_pair("collection", BINDING_COLLECTION)
        .append_pair("rkey", binding);
    let envelope: GetRecordEnvelope = get_public_limited_json(endpoint).await?;
    if !binding_matches(&envelope, key) {
        return Err("ATProto binding record does not match this device key".into());
    }
    Ok(())
}

fn binding_matches(envelope: &GetRecordEnvelope, key: &str) -> bool {
    envelope.value.record_type == RECORD_TYPE
        && envelope.value.device_public_key.eq_ignore_ascii_case(key)
}

fn has_cached_proof(
    entries: &[CachedBinding],
    did: &AtprotoDid,
    binding: &str,
    key: &str,
    grace_secs: u64,
    at: u64,
) -> bool {
    grace_secs > 0
        && entries.iter().any(|entry| {
            entry.did == did.as_str()
                && entry.binding == binding
                && entry.public_key == key
                && at.saturating_sub(entry.verified_at) <= grace_secs
        })
}

fn did_document_url(did: &AtprotoDid) -> Result<reqwest::Url, String> {
    if did.as_str().starts_with("did:plc:") {
        return reqwest::Url::parse(&format!("https://plc.directory/{did}"))
            .map_err(|_| "invalid did:plc URL".into());
    }
    let method = did
        .as_str()
        .strip_prefix("did:web:")
        .ok_or("unsupported DID method")?;
    if method.is_empty()
        || method.contains('/')
        || method.contains('\\')
        || method.split(':').any(|part| part.is_empty())
    {
        return Err("invalid did:web identifier".into());
    }
    let mut parts = method.split(':');
    let host = parts.next().unwrap();
    let path: Vec<&str> = parts.collect();
    let url = if path.is_empty() {
        format!("https://{host}/.well-known/did.json")
    } else {
        format!("https://{host}/{}/did.json", path.join("/"))
    };
    reqwest::Url::parse(&url).map_err(|_| "invalid did:web URL".into())
}

fn pds_endpoint(document: &serde_json::Value) -> Option<String> {
    document
        .get("service")?
        .as_array()?
        .iter()
        .find(|service| {
            service.get("id").and_then(|value| value.as_str()) == Some("#atproto_pds")
                || service.get("type").and_then(|value| value.as_str())
                    == Some("AtprotoPersonalDataServer")
        })?
        .get("serviceEndpoint")?
        .as_str()
        .map(str::to_owned)
}

async fn public_addresses(url: &reqwest::Url) -> Result<Vec<std::net::SocketAddr>, String> {
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() {
        return Err("identity endpoints must be public HTTPS URLs".into());
    }
    let host = url.host_str().ok_or("identity endpoint has no host")?;
    let port = url
        .port_or_known_default()
        .ok_or("identity endpoint has no port")?;
    let addresses: Vec<_> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "identity endpoint DNS lookup failed")?
        .collect();
    if addresses.is_empty() {
        return Err("identity endpoint DNS lookup returned no addresses".into());
    }
    for address in &addresses {
        if !public_ip(address.ip()) {
            return Err("identity endpoint resolves to a private address".into());
        }
    }
    Ok(addresses)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_multicast()
                || ip.is_unspecified()
                || octets[0] == 0
                || octets[0] >= 240
                || octets[0] == 100 && (64..=127).contains(&octets[1])
                || octets[0] == 198 && matches!(octets[1], 18 | 19))
        }
        IpAddr::V6(ip) => {
            if let Some(ipv4) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(ipv4));
            }
            let segments = ip.segments();
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                || segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}

async fn get_limited_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: reqwest::Url,
) -> Result<T, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| redact(&e.to_string()))?;
    if !response.status().is_success() {
        return Err(format!("identity endpoint returned {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IDENTITY_BYTES as u64)
    {
        return Err("identity response is too large".into());
    }
    let bytes = response.bytes().await.map_err(|e| redact(&e.to_string()))?;
    if bytes.len() > MAX_IDENTITY_BYTES {
        return Err("identity response is too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "identity endpoint returned invalid JSON".into())
}

async fn get_public_limited_json<T: for<'de> Deserialize<'de>>(
    url: reqwest::Url,
) -> Result<T, String> {
    let host = url
        .host_str()
        .ok_or("identity endpoint has no host")?
        .to_owned();
    let addresses = public_addresses(&url).await?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(4))
        .timeout(Duration::from_secs(8))
        // Redirects need a fresh address-policy check. Reject them instead of
        // allowing a public endpoint to bounce the verifier into a private
        // network.
        .redirect(reqwest::redirect::Policy::none())
        // Pin this request to the addresses we just screened, preventing a
        // second DNS answer from rebinding the hostname to a private service.
        .resolve_to_addrs(&host, &addresses)
        .user_agent("Wildforge/0.1 ATProto verifier")
        .build()
        .map_err(|error| error.to_string())?;
    get_limited_json(&client, url).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicProfile {
    handle: Option<String>,
    display_name: Option<String>,
    avatar: Option<String>,
}

async fn fetch_public_profile(did: &str) -> Result<PublicProfile, String> {
    let mut url = reqwest::Url::parse("https://public.api.bsky.app/xrpc/app.bsky.actor.getProfile")
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut().append_pair("actor", did);
    get_public_limited_json(url).await
}

fn save_cache(path: &Path, entries: &[CachedBinding]) -> io::Result<()> {
    let text = toml::to_string_pretty(&CacheFile {
        version: 1,
        proof: entries.to_vec(),
    })
    .map_err(io::Error::other)?;
    atomic_write(path, text.as_bytes(), false)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn redact(message: &str) -> String {
    // Library errors may include full callback/query URLs. Keep diagnostics
    // useful without risking authorization codes or tokens in logs/UI.
    message
        .split_whitespace()
        .map(|part| {
            if part.contains("token=")
                || part.contains("code=")
                || part.contains("access_token")
                || part.contains("refresh_token")
            {
                "[redacted]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn restrict_secret(path: &Path) -> io::Result<()> {
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_requires_exact_record_type_and_device_key() {
        let key = "01".repeat(32);
        let good = GetRecordEnvelope {
            value: BindingValue {
                record_type: RECORD_TYPE.into(),
                device_public_key: key.to_uppercase(),
            },
        };
        assert!(binding_matches(&good, &key));
        assert!(!binding_matches(&good, &"02".repeat(32)));
        let wrong_type = GetRecordEnvelope {
            value: BindingValue {
                record_type: "app.bsky.feed.post".into(),
                device_public_key: key.clone(),
            },
        };
        assert!(!binding_matches(&wrong_type, &key));
    }

    #[test]
    fn revoked_or_expired_cached_proof_does_not_downgrade_silently() {
        let did = AtprotoDid::parse("did:plc:cachetest").unwrap();
        let entries = vec![CachedBinding {
            did: did.to_string(),
            binding: "device-a".into(),
            public_key: "03".repeat(32),
            verified_at: 100,
        }];
        assert!(has_cached_proof(
            &entries,
            &did,
            "device-a",
            &"03".repeat(32),
            60,
            160,
        ));
        assert!(!has_cached_proof(
            &entries,
            &did,
            "device-a",
            &"03".repeat(32),
            60,
            161,
        ));
        assert!(!has_cached_proof(
            &entries,
            &did,
            "revoked-record",
            &"03".repeat(32),
            60,
            120,
        ));
    }

    #[test]
    fn did_resolution_tracks_current_pds_and_rejects_private_networks() {
        let first: serde_json::Value = serde_json::json!({
            "service": [{"id":"#atproto_pds","type":"AtprotoPersonalDataServer","serviceEndpoint":"https://old.example"}]
        });
        let migrated: serde_json::Value = serde_json::json!({
            "service": [{"id":"#atproto_pds","type":"AtprotoPersonalDataServer","serviceEndpoint":"https://new.example"}]
        });
        assert_eq!(pds_endpoint(&first).as_deref(), Some("https://old.example"));
        assert_eq!(
            pds_endpoint(&migrated).as_deref(),
            Some("https://new.example")
        );
        assert!(!public_ip("127.0.0.1".parse().unwrap()));
        assert!(!public_ip("169.254.1.2".parse().unwrap()));
        assert!(!public_ip("100.64.1.2".parse().unwrap()));
        assert!(!public_ip("224.0.0.1".parse().unwrap()));
        assert!(!public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!public_ip("2001:db8::1".parse().unwrap()));
        assert!(public_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn oauth_errors_redact_codes_and_tokens() {
        let message = redact("failed code=secret access_token=secret harmless");
        assert_eq!(message, "failed [redacted] [redacted] harmless");
    }
}

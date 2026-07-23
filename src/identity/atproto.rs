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

use jacquard_common::types::handle::Handle;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::nsid::Nsid;
use jacquard_common::types::recordkey::RecordKey;
use jacquard_common::types::value::to_data;
use jacquard_common::xrpc::XrpcClient;
use jacquard_common::xrpc::atproto::{DeleteRecord, PutRecord};
use jacquard_identity::JacquardResolver;
use jacquard_identity::resolver::{HandleStep, IdentityResolver, ResolverOptions};
use jacquard_oauth::atproto::AtprotoClientMetadata;
use jacquard_oauth::authstore::MemoryAuthStore;
use jacquard_oauth::client::OAuthClient;
use jacquard_oauth::loopback::{LoopbackConfig, LoopbackPort};
use jacquard_oauth::resolver::OAuthResolver;
use jacquard_oauth::scopes::Scopes;
use jacquard_oauth::session::ClientData;
use jacquard_oauth::types::AuthorizeOptions;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

use super::{AtprotoDid, DeviceKeyId, atomic_write, encode_hex};
use crate::net::AtprotoClaim;

/// Owned by dollspace.gay. The corresponding Lexicon document is checked in
/// under `lexicons/`; PDS validation is disabled until it is published.
pub const BINDING_COLLECTION: &str = "gay.dollspace.wildforge.device";
const RECORD_TYPE: &str = "gay.dollspace.wildforge.device";
const MAX_IDENTITY_BYTES: usize = 64 * 1024;
const MAX_CACHED_PROOFS: usize = 512;
const LIVE_VERIFY_TIMEOUT: Duration = Duration::from_secs(8);
const HANDLE_VERIFY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum PublicJsonError {
    NotFound,
    Other(String),
}

impl std::fmt::Display for PublicJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("identity record was not found"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for PublicJsonError {}

#[derive(Debug)]
enum ProofError {
    BindingAbsent,
    Other(String),
}

impl std::fmt::Display for ProofError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BindingAbsent => f.write_str("ATProto binding record was not found"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ProofError {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtprotoAccount {
    pub did: AtprotoDid,
    pub handle: Option<String>,
    pub binding: String,
    /// Public (not secret) key written into this account's binding record.
    #[serde(default)]
    pub device_public_key: String,
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
                if !account.device_public_key.is_empty()
                    && super::decode_hex::<32>(&account.device_public_key).is_none()
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid linked device public key",
                    ));
                }
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

fn oauth_client_data() -> io::Result<ClientData<SmolStr>> {
    let scopes = Scopes::new(SmolStr::new(format!("atproto repo:{BINDING_COLLECTION}")))
        .map_err(|e| io::Error::other(redact(&e.to_string())))?;
    Ok(ClientData {
        keyset: None,
        config: AtprotoClientMetadata::new_localhost(None, Some(scopes)),
    })
}

fn validate_oauth_subject(expected: Option<&str>, subject: &str) -> io::Result<AtprotoDid> {
    let subject = AtprotoDid::parse(subject)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if expected.is_some_and(|expected| expected != subject.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "OAuth token subject does not match the identity resolved before authorization",
        ));
    }
    Ok(subject)
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
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let store = MemoryAuthStore::new();
        let oauth = OAuthClient::new(store, oauth_client_data()?, reqwest::Client::new());
        // Resolve an identity hint before opening the browser. Jacquard checks
        // issuer ownership of the returned `sub`; Wildforge additionally
        // binds that subject to the handle/DID the player selected. A bare
        // PDS/entryway URL intentionally has no preselected account.
        let expected_did = oauth
            .client
            .resolve_oauth(input)
            .await
            .map_err(|error| io::Error::other(redact(&error.to_string())))?
            .1
            .map(|document| document.id.to_string());
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
        let parsed_did = validate_oauth_subject(expected_did.as_deref(), &did_text)?;
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
        // Do not report a successful link until the public repository path a
        // game server will use can read back this exact device record.
        let verified_handle = verify_live_bounded(&parsed_did, &rkey, &key_text)
            .await
            .map_err(io::Error::other)?;
        let profile = fetch_public_profile(&did_text).await.ok();
        let _ = session.logout().await;
        Ok::<_, io::Error>(AtprotoAccount {
            did: parsed_did,
            handle: profile
                .as_ref()
                .and_then(|value| value.handle.clone())
                .or(verified_handle),
            binding: rkey,
            device_public_key: key_text,
            linked_at: now(),
            use_social_display_name: false,
            use_social_avatar: false,
            profile_display_name: profile
                .as_ref()
                .and_then(|value| value.display_name.clone()),
            avatar_url: profile.and_then(|value| value.avatar),
        })
    });
    let account = result?;
    account.save(root)?;
    Ok(account)
}

/// Re-authenticate, delete this device's repository record, and remove local
/// metadata. If the remote delete fails, local unlink is not reported as a
/// revocation; callers may explicitly choose `unlink_local` instead.
pub fn revoke_account(root: &Path, input: &str, account: &AtprotoAccount) -> io::Result<()> {
    if account.device_public_key.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "this legacy account link has no device key; relink before revoking",
        ));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let store = MemoryAuthStore::new();
        let oauth = OAuthClient::new(store, oauth_client_data()?, reqwest::Client::new());
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
        // Re-fetch through the same public path used by game servers. A
        // successful verification here means revocation has not taken effect
        // yet and local metadata must remain linked.
        confirm_binding_deleted(&account.did, &account.binding, &account.device_public_key).await?;
        let _ = session.logout().await;
        Ok::<_, io::Error>(())
    });
    result?;
    AtprotoAccount::unlink_local(root)
}

#[derive(Clone, Debug)]
pub struct VerifiedBinding {
    pub did: AtprotoDid,
    pub cached: bool,
    pub handle: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedBinding {
    did: String,
    binding: String,
    public_key: String,
    verified_at: u64,
    #[serde(default)]
    handle: Option<String>,
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
        let mut cache = match std::fs::read_to_string(&path) {
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
        let changed = normalize_cache(&mut cache.proof, grace_secs, now());
        if changed {
            save_cache(&path, &cache.proof)?;
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
        match verify_live_bounded(&did, &claim.binding, &key).await {
            Ok(handle) => {
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
                    handle: handle.clone(),
                });
                entries.retain(|entry| now().saturating_sub(entry.verified_at) <= self.grace_secs);
                normalize_cache(&mut entries, self.grace_secs, now());
                save_cache(&self.path, &entries).map_err(|e| e.to_string())?;
                Ok(VerifiedBinding {
                    did,
                    cached: false,
                    handle,
                })
            }
            Err(live_error) => {
                let entries = self.entries.lock().map_err(|_| "proof cache poisoned")?;
                if let Some(handle) =
                    cached_proof(&entries, &did, &claim.binding, &key, self.grace_secs, now())
                {
                    Ok(VerifiedBinding {
                        did,
                        cached: true,
                        handle,
                    })
                } else {
                    Err(live_error.to_string())
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

async fn verify_live(
    did: &AtprotoDid,
    binding: &str,
    key: &str,
) -> Result<Option<String>, ProofError> {
    RecordKey::<jacquard_common::types::recordkey::Rkey>::any_owned(binding)
        .map_err(|_| ProofError::Other("invalid ATProto binding record key".into()))?;
    let document_url = did_document_url(did).map_err(ProofError::Other)?;
    let document: serde_json::Value = get_public_limited_json(document_url)
        .await
        .map_err(|error| ProofError::Other(error.to_string()))?;
    if document.get("id").and_then(|value| value.as_str()) != Some(did.as_str()) {
        return Err(ProofError::Other(
            "resolved DID document has the wrong id".into(),
        ));
    }
    let handle_candidate = handle_from_document(&document);
    let endpoint = pds_endpoint(&document)
        .ok_or_else(|| ProofError::Other("DID document has no ATProto PDS service".into()))?;
    let mut endpoint = reqwest::Url::parse(&endpoint)
        .map_err(|_| ProofError::Other("invalid PDS endpoint".into()))?;
    endpoint.set_path("/xrpc/com.atproto.repo.getRecord");
    endpoint.set_query(None);
    endpoint
        .query_pairs_mut()
        .append_pair("repo", did.as_str())
        .append_pair("collection", BINDING_COLLECTION)
        .append_pair("rkey", binding);
    let handle_check = async {
        let handle = handle_candidate?;
        tokio::time::timeout(HANDLE_VERIFY_TIMEOUT, handle_matches_did(&handle, did))
            .await
            .ok()
            .filter(|matches| *matches)
            .map(|_| handle)
    };
    let (envelope, handle) = tokio::join!(get_public_limited_json(endpoint), handle_check);
    let envelope: GetRecordEnvelope = match envelope {
        Ok(envelope) => envelope,
        Err(PublicJsonError::NotFound) => return Err(ProofError::BindingAbsent),
        Err(PublicJsonError::Other(error)) => return Err(ProofError::Other(error)),
    };
    if !binding_matches(&envelope, key) {
        return Err(ProofError::Other(
            "ATProto binding record does not match this device key".into(),
        ));
    }
    Ok(handle)
}

async fn verify_live_bounded(
    did: &AtprotoDid,
    binding: &str,
    key: &str,
) -> Result<Option<String>, ProofError> {
    tokio::time::timeout(LIVE_VERIFY_TIMEOUT, verify_live(did, binding, key))
        .await
        .unwrap_or_else(|_| Err(ProofError::Other("ATProto verification timed out".into())))
}

async fn confirm_binding_deleted(did: &AtprotoDid, binding: &str, key: &str) -> io::Result<()> {
    for delay in [0, 200, 600] {
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        match verify_live_bounded(did, binding, key).await {
            Err(ProofError::BindingAbsent) => return Ok(()),
            Err(ProofError::Other(error)) => {
                return Err(io::Error::other(format!(
                    "could not confirm device-binding deletion: {error}"
                )));
            }
            Ok(_) => {}
        }
    }
    Err(io::Error::other(
        "device binding is still publicly verifiable after deletion",
    ))
}

fn binding_matches(envelope: &GetRecordEnvelope, key: &str) -> bool {
    envelope.value.record_type == RECORD_TYPE
        && envelope.value.device_public_key.eq_ignore_ascii_case(key)
}

fn cached_proof(
    entries: &[CachedBinding],
    did: &AtprotoDid,
    binding: &str,
    key: &str,
    grace_secs: u64,
    at: u64,
) -> Option<Option<String>> {
    (grace_secs > 0)
        .then(|| {
            entries.iter().find(|entry| {
                entry.did == did.as_str()
                    && entry.binding == binding
                    && entry.public_key == key
                    && at.saturating_sub(entry.verified_at) <= grace_secs
            })
        })
        .flatten()
        .map(|entry| entry.handle.clone())
}

fn normalize_cache(entries: &mut Vec<CachedBinding>, grace_secs: u64, at: u64) -> bool {
    let before = entries.len();
    let before_order: Vec<u64> = entries.iter().map(|entry| entry.verified_at).collect();
    if grace_secs == 0 {
        entries.clear();
        return before != 0;
    }
    entries.retain(|entry| at.saturating_sub(entry.verified_at) <= grace_secs);
    entries.sort_by_key(|entry| entry.verified_at);
    if entries.len() > MAX_CACHED_PROOFS {
        entries.drain(..entries.len() - MAX_CACHED_PROOFS);
    }
    entries.len() != before
        || entries
            .iter()
            .map(|entry| entry.verified_at)
            .ne(before_order)
}

fn handle_from_document(document: &serde_json::Value) -> Option<String> {
    document
        .get("alsoKnownAs")?
        .as_array()?
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter_map(|value| value.strip_prefix("at://"))
        .find_map(|value| Handle::new(value).ok())
        .map(|handle| handle.to_string())
}

async fn handle_matches_did(handle: &str, did: &AtprotoDid) -> bool {
    let Ok(handle) = Handle::<SmolStr>::new_owned(handle) else {
        return false;
    };
    let options = ResolverOptions {
        handle_order: vec![HandleStep::DnsTxt],
        public_fallback_for_handle: false,
        ..ResolverOptions::default()
    };
    let resolver = JacquardResolver::new_dns(reqwest::Client::new(), options);
    if let Ok(resolved) = resolver.resolve_handle(&handle).await {
        return resolved.as_str() == did.as_str();
    }

    let Ok(url) = reqwest::Url::parse(&format!("https://{handle}/.well-known/atproto-did")) else {
        return false;
    };
    get_public_limited_text(url)
        .await
        .is_ok_and(|resolved| resolved.trim() == did.as_str())
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
) -> Result<T, PublicJsonError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| PublicJsonError::Other(redact(&e.to_string())))?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IDENTITY_BYTES as u64)
    {
        return Err(PublicJsonError::Other(
            "identity response is too large".into(),
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| PublicJsonError::Other(redact(&e.to_string())))?;
    if bytes.len() > MAX_IDENTITY_BYTES {
        return Err(PublicJsonError::Other(
            "identity response is too large".into(),
        ));
    }
    if !status.is_success() {
        if response_is_not_found(status, &bytes) {
            return Err(PublicJsonError::NotFound);
        }
        return Err(PublicJsonError::Other(format!(
            "identity endpoint returned {status}"
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| PublicJsonError::Other("identity endpoint returned invalid JSON".into()))
}

fn response_is_not_found(status: reqwest::StatusCode, bytes: &[u8]) -> bool {
    status == reqwest::StatusCode::NOT_FOUND
        || serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .and_then(|value| value.get("error")?.as_str().map(str::to_owned))
            .is_some_and(|error| error == "RecordNotFound")
}

async fn get_public_limited_json<T: for<'de> Deserialize<'de>>(
    url: reqwest::Url,
) -> Result<T, PublicJsonError> {
    let client = public_client(&url).await?;
    get_limited_json(&client, url).await
}

async fn public_client(url: &reqwest::Url) -> Result<reqwest::Client, PublicJsonError> {
    let host = url
        .host_str()
        .ok_or_else(|| PublicJsonError::Other("identity endpoint has no host".into()))?
        .to_owned();
    let addresses = public_addresses(url)
        .await
        .map_err(PublicJsonError::Other)?;
    reqwest::Client::builder()
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
        .map_err(|error| PublicJsonError::Other(error.to_string()))
}

async fn get_public_limited_text(url: reqwest::Url) -> Result<String, PublicJsonError> {
    let client = public_client(&url).await?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| PublicJsonError::Other(redact(&error.to_string())))?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_IDENTITY_BYTES as u64)
    {
        return Err(PublicJsonError::Other(
            "identity response is too large".into(),
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| PublicJsonError::Other(redact(&error.to_string())))?;
    if !status.is_success() {
        return Err(PublicJsonError::Other(format!(
            "identity endpoint returned {status}"
        )));
    }
    if bytes.len() > MAX_IDENTITY_BYTES {
        return Err(PublicJsonError::Other(
            "identity response is too large".into(),
        ));
    }
    String::from_utf8(bytes.to_vec())
        .map_err(|_| PublicJsonError::Other("identity endpoint returned invalid UTF-8".into()))
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
    get_public_limited_json(url)
        .await
        .map_err(|error| error.to_string())
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
        assert!(response_is_not_found(
            reqwest::StatusCode::BAD_REQUEST,
            br#"{"error":"RecordNotFound"}"#
        ));
        assert!(response_is_not_found(reqwest::StatusCode::NOT_FOUND, b""));
        assert!(!response_is_not_found(
            reqwest::StatusCode::BAD_GATEWAY,
            b""
        ));
    }

    #[test]
    fn revoked_or_expired_cached_proof_does_not_downgrade_silently() {
        let did = AtprotoDid::parse("did:plc:cachetest").unwrap();
        let entries = vec![CachedBinding {
            did: did.to_string(),
            binding: "device-a".into(),
            public_key: "03".repeat(32),
            verified_at: 100,
            handle: Some("moss.example".into()),
        }];
        assert_eq!(
            cached_proof(&entries, &did, "device-a", &"03".repeat(32), 60, 160,),
            Some(Some("moss.example".into()))
        );
        assert_eq!(
            cached_proof(&entries, &did, "device-a", &"03".repeat(32), 60, 161,),
            None
        );
        assert_eq!(
            cached_proof(&entries, &did, "revoked-record", &"03".repeat(32), 60, 120,),
            None
        );
        assert_eq!(
            cached_proof(&entries, &did, "device-a", &"03".repeat(32), 0, 100,),
            None
        );
    }

    #[test]
    fn proof_cache_is_pruned_on_load_and_persisted_at_its_hard_limit() {
        let root =
            std::env::temp_dir().join(format!("wildforge-proof-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("proofs.toml");
        let at = now();
        let proof: Vec<CachedBinding> = (0..(MAX_CACHED_PROOFS + 4))
            .map(|index| CachedBinding {
                did: format!("did:plc:cache{index}"),
                binding: format!("device-{index}"),
                public_key: format!("{index:064x}"),
                verified_at: at.saturating_sub((MAX_CACHED_PROOFS + 4 - index) as u64),
                handle: None,
            })
            .collect();
        save_cache(&path, &proof).unwrap();

        let loaded = ProofCache::load(path.clone(), 3_600).unwrap();
        let entries = loaded.entries.lock().unwrap();
        assert_eq!(entries.len(), MAX_CACHED_PROOFS);
        assert!(
            entries
                .windows(2)
                .all(|pair| { pair[0].verified_at <= pair[1].verified_at })
        );
        drop(entries);
        let persisted: CacheFile =
            toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(persisted.version, 1);
        assert_eq!(persisted.proof.len(), MAX_CACHED_PROOFS);

        let disabled = ProofCache::load(path.clone(), 0).unwrap();
        assert!(disabled.entries.lock().unwrap().is_empty());
        let persisted: CacheFile = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(persisted.proof.is_empty());
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
    fn did_document_handle_is_validated() {
        let document = serde_json::json!({
            "alsoKnownAs": [
                "https://example.invalid/profile",
                "at://moss.garden"
            ]
        });
        assert_eq!(
            handle_from_document(&document).as_deref(),
            Some("moss.garden")
        );
        let invalid = serde_json::json!({"alsoKnownAs": ["at://not a handle"]});
        assert_eq!(handle_from_document(&invalid), None);
    }

    #[test]
    fn oauth_errors_redact_codes_and_tokens() {
        let message = redact("failed code=secret access_token=secret harmless");
        assert_eq!(message, "failed [redacted] [redacted] harmless");
    }

    #[test]
    fn oauth_metadata_requests_only_the_binding_scope_and_dpop() {
        let client = oauth_client_data().unwrap();
        let wire = jacquard_oauth::atproto::atproto_client_metadata(&client.config, &None).unwrap();
        assert_eq!(
            wire.scope.as_ref().map(AsRef::as_ref),
            Some("atproto repo:gay.dollspace.wildforge.device")
        );
        assert_eq!(wire.dpop_bound_access_tokens, Some(true));
        assert_eq!(
            wire.token_endpoint_auth_method.as_ref().map(AsRef::as_ref),
            Some("none")
        );

        let hosted: serde_json::Value = serde_json::from_str(include_str!(
            "../../docs/atproto-oauth-client-metadata.json"
        ))
        .unwrap();
        assert_eq!(
            hosted.get("scope").and_then(serde_json::Value::as_str),
            Some("atproto repo:gay.dollspace.wildforge.device")
        );
        assert_eq!(
            hosted
                .get("dpop_bound_access_tokens")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn persisted_account_metadata_contains_no_oauth_credentials() {
        let root =
            std::env::temp_dir().join(format!("wildforge-atproto-account-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let account = AtprotoAccount {
            did: AtprotoDid::parse("did:plc:account").unwrap(),
            handle: Some("moss.example".into()),
            binding: "device-a".into(),
            device_public_key: "ab".repeat(32),
            linked_at: 7,
            use_social_display_name: false,
            use_social_avatar: false,
            profile_display_name: Some("Moss".into()),
            avatar_url: None,
        };
        account.save(&root).unwrap();
        let path = root.join("atproto.toml");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("access_token"));
        assert!(!text.contains("refresh_token"));
        assert!(!text.contains("dpop"));
        assert_eq!(
            AtprotoAccount::load(&root).unwrap().unwrap().did,
            account.did
        );

        std::fs::write(&path, text.replace(&"ab".repeat(32), "not-a-public-key")).unwrap();
        assert_eq!(
            AtprotoAccount::load(&root).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );

        let mut legacy = account;
        legacy.device_public_key.clear();
        assert_eq!(
            revoke_account(&root, "moss.example", &legacy)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}

#[cfg(test)]
#[path = "oauth_conformance.rs"]
mod oauth_conformance;

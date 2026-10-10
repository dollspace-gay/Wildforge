//! DNS-first handle resolution using the native operating system's nameservers.

use hickory_resolver::TokioResolver;
use hickory_resolver::net::NetError;
use hickory_resolver::proto::rr::RData;
use jacquard_common::types::did::Did;
use jacquard_common::types::handle::Handle;
use smol_str::SmolStr;

use crate::identity::AtprotoDid;

#[derive(Debug, thiserror::Error)]
pub(super) enum DnsError {
    #[error("DNS resolver configuration task failed")]
    ConfigurationTask(#[from] tokio::task::JoinError),
    #[error("DNS handle resolution failed")]
    Resolution(#[from] NetError),
}

pub(super) async fn resolve_did(handle: &Handle<SmolStr>) -> Result<Option<Did>, DnsError> {
    let resolver = native_resolver().await?;
    Ok(resolve_did_with(&resolver, handle).await?)
}

async fn native_resolver() -> Result<TokioResolver, DnsError> {
    // Native configuration reads a file on Unix and adapter state on Windows.
    // Keep those synchronous operations outside the async identity worker.
    Ok(tokio::task::spawn_blocking(|| TokioResolver::builder_tokio()?.build()).await??)
}

pub(super) async fn matches_did_with_fallback(
    expected: &AtprotoDid,
    lookup: impl Future<Output = Result<Option<Did>, DnsError>>,
    fallback: impl Future<Output = bool>,
) -> bool {
    if let Ok(Some(resolved)) = lookup.await {
        return resolved.as_str() == expected.as_str();
    }
    fallback.await
}

async fn resolve_did_with(
    resolver: &TokioResolver,
    handle: &Handle<SmolStr>,
) -> Result<Option<Did>, NetError> {
    let response = resolver.txt_lookup(format!("_atproto.{handle}.")).await?;
    for record in response.answers() {
        let RData::TXT(txt) = &record.data else {
            continue;
        };
        // Preserve Jacquard's first-valid character-string selection, including
        // its treatment of segmented TXT records and conflicting valid values.
        for data in &txt.txt_data {
            let value = String::from_utf8_lossy(data);
            if let Some(value) = value.strip_prefix("did=")
                && let Ok(did) = Did::new_owned(value)
            {
                return Ok(Some(did));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "handle_dns_tests.rs"]
mod tests;

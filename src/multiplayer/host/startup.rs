//! Startup for the authoritative host session.

use super::{
    AdmissionPolicy, DisplayName, HashMap, HashSet, HostSession, IdentityPolicy, ServerSettings,
    chunk_jobs, net,
};

impl HostSession {
    pub fn start(world_name: String) -> std::io::Result<HostSession> {
        Self::start_configured(world_name, None)
    }

    pub(super) fn start_configured(
        world_name: String,
        host_name: Option<DisplayName>,
    ) -> std::io::Result<HostSession> {
        let settings =
            ServerSettings::load_or_create(&std::path::PathBuf::from("saves").join(&world_name))?;
        Self::start_on_with_settings(
            world_name,
            settings.port,
            host_name,
            settings.identity,
            settings.admission,
            settings.verification_grace_secs,
        )
    }

    pub fn start_windowed(
        world_name: String,
        host_name: DisplayName,
    ) -> std::io::Result<HostSession> {
        Self::start_configured(world_name, Some(host_name))
    }

    /// Tests and second-hosts bind an OS-assigned port with 0.
    #[cfg(test)]
    pub fn start_on(world_name: String, port: u16) -> std::io::Result<HostSession> {
        Self::start_on_with_policy(
            world_name,
            port,
            None,
            IdentityPolicy::Local,
            AdmissionPolicy::Open,
        )
    }

    #[cfg(test)]
    pub fn start_on_with_policy(
        world_name: String,
        port: u16,
        host_name: Option<DisplayName>,
        identity_policy: IdentityPolicy,
        admission_policy: AdmissionPolicy,
    ) -> std::io::Result<HostSession> {
        Self::start_on_with_settings(
            world_name,
            port,
            host_name,
            identity_policy,
            admission_policy,
            3_600,
        )
    }

    pub(super) fn start_on_with_settings(
        world_name: String,
        port: u16,
        host_name: Option<DisplayName>,
        identity_policy: IdentityPolicy,
        admission_policy: AdmissionPolicy,
        verification_grace_secs: u64,
    ) -> std::io::Result<HostSession> {
        Ok(HostSession {
            net: net::Host::start(
                world_name.clone(),
                port,
                identity_policy,
                admission_policy,
                verification_grace_secs,
            )?,
            guests: HashMap::new(),
            content_hash: net::content_hash(std::path::Path::new("mods")),
            world_name,
            identity_policy,
            admission_policy,
            host_name: host_name.map(|name| name.to_string()),
            profiles: None,
            pending_guests: HashMap::new(),
            moderation: None,
            banned: HashSet::new(),
            fresh_spawn: None,
            chunk_jobs: chunk_jobs::HostChunkState::default(),
            initial_view_dist: 5,
            snapshot_timer: 0.0,
            snapshot_seq: 0,
            state_timer: 0.0,
            container_timer: 0.0,
            perish_timer: 0.0,
            sleep_settle: 0.0,
        })
    }
}

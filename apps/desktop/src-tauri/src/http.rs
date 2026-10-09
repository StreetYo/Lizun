//! The HTTP client shared by everything that downloads: widget installs and calendar feeds.

use std::time::Duration;

/// An agent that checks certificates against the system store and gives up after `timeout`.
pub fn agent(timeout: Duration) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(timeout))
        .user_agent(concat!("Lizun/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

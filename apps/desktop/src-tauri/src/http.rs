//! The HTTP client shared by everything that downloads: widget installs, calendar feeds and
//! widgets' own requests.

use std::time::Duration;

/// An agent that checks certificates against the system store and gives up after `timeout`.
pub fn agent(timeout: Duration) -> ureq::Agent {
    config(timeout).build().into()
}

/// The configuration of [`agent`], for callers that change some of it.
pub fn config(timeout: Duration) -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(timeout))
        .user_agent(concat!("Lizun/", env!("CARGO_PKG_VERSION")))
}

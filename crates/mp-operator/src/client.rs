//! Cluster access, behind a trait.
//!
//! The shipped implementation talks to a local `kubectl proxy`, which avoids
//! reimplementing TLS and service-account auth inside a security component.
//! `deploy/` runs the proxy as a sidecar. This is the same arrangement the
//! daemon uses for TLS termination, and for the same reason.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ClusterError {
    NotFound(String),
    Transport(String),
    Decode(String),
}

impl std::fmt::Display for ClusterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClusterError::NotFound(s) => write!(f, "not found: {s}"),
            ClusterError::Transport(s) => write!(f, "transport: {s}"),
            ClusterError::Decode(s) => write!(f, "decode: {s}"),
        }
    }
}

impl std::error::Error for ClusterError {}

/// What reconcile needs from a cluster. Nothing more, so the operator's
/// required RBAC stays visibly small.
pub trait ClusterClient {
    /// Read a ConfigMap's data.
    fn get_config_map(
        &self,
        ns: &str,
        name: &str,
    ) -> Result<BTreeMap<String, String>, ClusterError>;
    /// Count objects matching a label selector, for symmetry class sizing.
    fn count_matching(
        &self,
        ns: &str,
        selector: &BTreeMap<String, String>,
    ) -> Result<usize, ClusterError>;
    /// Write back a policy's status subresource.
    fn patch_status(&self, ns: &str, name: &str, status_json: &str) -> Result<(), ClusterError>;
}

/// Client that speaks plain HTTP to a `kubectl proxy` on loopback.
pub struct ProxyClient {
    pub base: String,
}

impl ProxyClient {
    pub fn new(base: impl Into<String>) -> Self {
        ProxyClient { base: base.into() }
    }
}

impl ClusterClient for ProxyClient {
    fn get_config_map(
        &self,
        ns: &str,
        name: &str,
    ) -> Result<BTreeMap<String, String>, ClusterError> {
        Err(ClusterError::Transport(format!(
            "ProxyClient is a transport stub: GET {}/api/v1/namespaces/{ns}/configmaps/{name}. \
             Wire it to a real HTTP client before deploying; reconcile logic is transport-free \
             and tested independently.",
            self.base
        )))
    }

    fn count_matching(
        &self,
        _ns: &str,
        _selector: &BTreeMap<String, String>,
    ) -> Result<usize, ClusterError> {
        Err(ClusterError::Transport(
            "ProxyClient is a transport stub".into(),
        ))
    }

    fn patch_status(&self, _ns: &str, _name: &str, _status_json: &str) -> Result<(), ClusterError> {
        Err(ClusterError::Transport(
            "ProxyClient is a transport stub".into(),
        ))
    }
}

/// In-memory client for tests and dry runs.
#[derive(Debug, Default)]
pub struct FakeClient {
    pub config_maps: BTreeMap<(String, String), BTreeMap<String, String>>,
    pub counts: BTreeMap<String, usize>,
    pub patches: std::cell::RefCell<Vec<String>>,
}

impl FakeClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config_map(mut self, ns: &str, name: &str, data: BTreeMap<String, String>) -> Self {
        self.config_maps
            .insert((ns.to_string(), name.to_string()), data);
        self
    }

    pub fn with_count(mut self, class: &str, n: usize) -> Self {
        self.counts.insert(class.to_string(), n);
        self
    }
}

impl ClusterClient for FakeClient {
    fn get_config_map(
        &self,
        ns: &str,
        name: &str,
    ) -> Result<BTreeMap<String, String>, ClusterError> {
        self.config_maps
            .get(&(ns.to_string(), name.to_string()))
            .cloned()
            .ok_or_else(|| ClusterError::NotFound(format!("configmap {ns}/{name}")))
    }

    fn count_matching(
        &self,
        _ns: &str,
        selector: &BTreeMap<String, String>,
    ) -> Result<usize, ClusterError> {
        let key = selector.values().cloned().collect::<Vec<_>>().join(",");
        Ok(self.counts.get(&key).copied().unwrap_or(0))
    }

    fn patch_status(&self, ns: &str, name: &str, status_json: &str) -> Result<(), ClusterError> {
        self.patches
            .borrow_mut()
            .push(format!("{ns}/{name}: {status_json}"));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_client_serves_what_it_was_given() {
        let c = FakeClient::new().with_config_map(
            "mp",
            "cal",
            [("budget".to_string(), "512".to_string())]
                .into_iter()
                .collect(),
        );
        assert_eq!(c.get_config_map("mp", "cal").unwrap()["budget"], "512");
        assert!(c.get_config_map("mp", "missing").is_err());
    }

    #[test]
    fn the_proxy_client_fails_loudly_rather_than_silently_succeeding() {
        // A stub that returned Ok(empty) would let the operator write a
        // "calibrated" status from data it never fetched.
        let c = ProxyClient::new("http://127.0.0.1:8001");
        assert!(matches!(
            c.get_config_map("a", "b"),
            Err(ClusterError::Transport(_))
        ));
    }
}

//! Kubernetes operator for manifold-plane.
//!
//! Reconciles `ManifoldPolicy` custom resources into a running configuration:
//! symmetry classes, calibrated budgets, and half-lives.
//!
//! The reconcile logic is pure and the cluster access sits behind a trait. That
//! is not a testability flourish — an operator that can change an admission
//! controller's budget is itself a privileged component, and the part that
//! decides *what* to change should be verifiable without a cluster in the loop.

pub mod client;
pub mod policy;
pub mod reconcile;

pub use client::{ClusterClient, ClusterError, ProxyClient};
pub use policy::{ManifoldPolicy, PolicySpec, PolicyStatus, SymmetryClassSpec};
pub use reconcile::{reconcile, Action, ReconcileReport};

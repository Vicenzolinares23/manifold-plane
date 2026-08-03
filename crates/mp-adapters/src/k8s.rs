//! Kubernetes admission adapter.
//!
//! Maps an AdmissionReview into a displacement. The escalation chains this is
//! aimed at are the well-known ones — bind a role, mount a token, exec into a
//! pod, create a workload that runs as root — each of which passes RBAC on its
//! own because RBAC evaluates one request against one policy with no memory of
//! the previous four.

use crate::{amplify_by_reach, irreversibility_bits, opacity_bits, Adapter, Displacement};
use mp_core::axis::Axis;
use mp_core::linalg::Vec6;

/// The subset of an AdmissionReview the displacement depends on.
///
/// Deliberately not the full object. `docs/02` S3 requires resources to enter
/// only through their class and reachability, never their names, so the adapter
/// must not be able to see a name even by accident.
#[derive(Debug, Clone, PartialEq)]
pub struct AdmissionRequest {
    pub verb: Verb,
    pub resource: ResourceClass,
    /// Namespaces the request's effect can span. Cluster-scoped resources
    /// report the cluster's namespace count.
    pub namespace_span: u32,
    /// True when the object grants permissions to a subject: RoleBinding,
    /// ClusterRoleBinding, or a ServiceAccount token request.
    pub grants_permissions: bool,
    /// Distinct verbs the granted role confers, if this is a grant.
    pub granted_verb_count: u32,
    /// True when the request escapes the audit trail: `pods/exec`,
    /// `pods/attach`, `pods/portforward`, or a mutation of audit configuration.
    pub evades_audit: bool,
    /// True when the resource has no undo: deletion without a backup, a
    /// finalizer removal, a PVC delete with `Delete` reclaim policy.
    pub destroys_state: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    Get,
    List,
    Watch,
    Create,
    Update,
    Patch,
    Delete,
    DeleteCollection,
    Exec,
    Impersonate,
}

impl Verb {
    /// Preimage count of the verb's effect on cluster state (`docs/03` A3).
    ///
    /// Reads are bijective: the world after is the world before. `delete` maps
    /// every possible prior object content to the same absent state, so its
    /// preimage is the object's content entropy — floored here at a
    /// conservative 2^10, since the true value needs a sample of real objects
    /// and `docs/03` says to cap at a measured entropy rather than guess high.
    fn preimages(self) -> f64 {
        match self {
            Verb::Get | Verb::List | Verb::Watch => 1.0,
            Verb::Create => 2.0,
            Verb::Update | Verb::Patch => 64.0,
            Verb::Delete => 1024.0,
            Verb::DeleteCollection => 65536.0,
            Verb::Exec | Verb::Impersonate => 256.0,
        }
    }

    fn is_read(self) -> bool {
        matches!(self, Verb::Get | Verb::List | Verb::Watch)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceClass {
    Pod,
    Secret,
    ConfigMap,
    ServiceAccount,
    Role,
    ClusterRole,
    RoleBinding,
    ClusterRoleBinding,
    Workload,
    Node,
    PersistentVolume,
    WebhookConfig,
    Other,
}

impl ResourceClass {
    /// Reachability multiplier: how much of the cluster this resource class
    /// exposes. Bits, not a score — `log2` of the reachable-set expansion.
    fn reach_bits(self) -> f64 {
        match self {
            ResourceClass::ConfigMap | ResourceClass::Other => 0.0,
            ResourceClass::Pod | ResourceClass::Workload => 1.0,
            ResourceClass::ServiceAccount => 2.0,
            ResourceClass::Secret => 3.0,
            ResourceClass::Role | ResourceClass::RoleBinding => 3.0,
            ResourceClass::PersistentVolume => 2.0,
            // The bridge resources from `docs/04`: one permission here, many
            // bits of reach. This is precisely where authority and reach
            // decorrelate, which is the signal collapsing them would destroy.
            ResourceClass::Node => 6.0,
            ResourceClass::ClusterRole | ResourceClass::ClusterRoleBinding => 7.0,
            ResourceClass::WebhookConfig => 8.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct K8sAdapter {
    /// Cluster namespace count, for normalizing span.
    pub total_namespaces: u32,
}

impl K8sAdapter {
    pub fn new(total_namespaces: u32) -> Self {
        K8sAdapter {
            total_namespaces: total_namespaces.max(1),
        }
    }
}

impl Adapter for K8sAdapter {
    type Request = AdmissionRequest;

    fn name(&self) -> &'static str {
        "kubernetes"
    }

    fn displacement(&self, r: &AdmissionRequest, current: &Vec6) -> Displacement {
        let mut d = Displacement::zero();

        // Authority: bits of new operation surface conferred by a grant.
        if r.grants_permissions {
            d = d.with(Axis::Authority, (1.0 + r.granted_verb_count as f64).log2());
        } else if !r.verb.is_read() {
            d = d.with(Axis::Authority, 0.25);
        }

        // Reach: resource class expansion, scaled by namespace span.
        let span =
            (r.namespace_span.max(1) as f64 / self.total_namespaces.max(1) as f64).clamp(0.0, 1.0);
        d = d.with(Axis::Reach, r.resource.reach_bits() * (0.5 + 0.5 * span));

        // Irreversibility: verb preimages, plus the floor for destroyed state.
        let mut preimages = r.verb.preimages();
        if r.destroys_state {
            preimages = preimages.max(4096.0);
        }
        d = d.with(Axis::Irreversibility, irreversibility_bits(preimages));

        // Opacity: exec and friends leave the audit trail behind.
        if r.evades_audit {
            d = d.with(Axis::Opacity, opacity_bits(0.05));
        }

        // Tempo is measured by the daemon from arrival times, not from the
        // request body — a request cannot be trusted to describe its own rate.

        amplify_by_reach(d, current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp_core::linalg::N;

    fn req(verb: Verb, resource: ResourceClass) -> AdmissionRequest {
        AdmissionRequest {
            verb,
            resource,
            namespace_span: 1,
            grants_permissions: false,
            granted_verb_count: 0,
            evades_audit: false,
            destroys_state: false,
        }
    }

    fn adapter() -> K8sAdapter {
        K8sAdapter::new(50)
    }

    #[test]
    fn reading_a_configmap_is_nearly_free() {
        let d = adapter().displacement(&req(Verb::Get, ResourceClass::ConfigMap), &[0.0; N]);
        assert_eq!(d.get(Axis::Irreversibility), 0.0);
        assert_eq!(d.get(Axis::Authority), 0.0);
        assert!(d.get(Axis::Reach) < 0.01);
    }

    #[test]
    fn a_cluster_role_binding_costs_far_more_reach_than_a_configmap() {
        let a = adapter().displacement(&req(Verb::Create, ResourceClass::ConfigMap), &[0.0; N]);
        let b = adapter().displacement(
            &req(Verb::Create, ResourceClass::ClusterRoleBinding),
            &[0.0; N],
        );
        assert!(b.get(Axis::Reach) > a.get(Axis::Reach) + 3.0);
    }

    #[test]
    fn exec_registers_as_opacity_not_just_authority() {
        let mut r = req(Verb::Exec, ResourceClass::Pod);
        r.evades_audit = true;
        let d = adapter().displacement(&r, &[0.0; N]);
        assert!(d.get(Axis::Opacity) > 4.0, "exec should be strongly opaque");
    }

    #[test]
    fn delete_collection_is_far_more_irreversible_than_a_single_delete() {
        let one = adapter().displacement(&req(Verb::Delete, ResourceClass::Pod), &[0.0; N]);
        let many =
            adapter().displacement(&req(Verb::DeleteCollection, ResourceClass::Pod), &[0.0; N]);
        assert!(many.get(Axis::Irreversibility) > one.get(Axis::Irreversibility) + 5.0);
    }

    #[test]
    fn a_broad_grant_costs_authority_proportional_to_verbs_conferred() {
        let mut narrow = req(Verb::Create, ResourceClass::RoleBinding);
        narrow.grants_permissions = true;
        narrow.granted_verb_count = 1;

        let mut broad = narrow.clone();
        broad.granted_verb_count = 255;

        let a = adapter().displacement(&narrow, &[0.0; N]);
        let b = adapter().displacement(&broad, &[0.0; N]);
        assert!((a.get(Axis::Authority) - 1.0).abs() < 1e-9);
        assert!((b.get(Axis::Authority) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn the_bridge_resource_decorrelates_authority_and_reach() {
        // docs/04's counterexample, concretely. A node-level grant buys one
        // bit of authority and six of reach. Collapsing the two axes into one
        // score would erase exactly this, and this is the escalation step.
        let mut r = req(Verb::Patch, ResourceClass::Node);
        r.grants_permissions = true;
        r.granted_verb_count = 1;
        let d = adapter().displacement(&r, &[0.0; N]);
        assert!(
            d.get(Axis::Reach) > 3.0 * d.get(Axis::Authority),
            "reach {} should dominate authority {}",
            d.get(Axis::Reach),
            d.get(Axis::Authority)
        );
    }

    #[test]
    fn the_same_request_costs_more_from_a_position_of_broad_reach() {
        let r = req(Verb::Create, ResourceClass::Secret);
        let from_baseline = adapter().displacement(&r, &[0.0; N]);
        let mut broad = [0.0; N];
        broad[Axis::Reach.index()] = 7.0;
        let from_broad = adapter().displacement(&r, &broad);
        assert!(from_broad.get(Axis::Reach) > from_baseline.get(Axis::Reach));
    }

    #[test]
    fn cluster_wide_span_costs_more_reach_than_a_single_namespace() {
        let mut one = req(Verb::Create, ResourceClass::Role);
        one.namespace_span = 1;
        let mut all = one.clone();
        all.namespace_span = 50;
        let a = adapter().displacement(&one, &[0.0; N]);
        let b = adapter().displacement(&all, &[0.0; N]);
        assert!(b.get(Axis::Reach) > a.get(Axis::Reach));
    }
}

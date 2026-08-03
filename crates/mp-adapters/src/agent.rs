//! LLM agent tool-call adapter.
//!
//! The newest of the three domains and the one with the least existing defence.
//! An agent's tool calls are individually approvable and collectively
//! catastrophic: read a file, summarize it, call an HTTP tool, and private data
//! has left the building through three steps nobody would have refused.
//!
//! This is `docs/01` I6 in its purest form, which is why it makes a good test
//! of whether the state space generalizes. It also has a property the other two
//! lack — the classic exfiltration chain moves *reach* then *irreversibility*
//! in sequence, so it is visible as a trajectory shape rather than as any
//! single step.

use crate::{amplify_by_reach, irreversibility_bits, opacity_bits, Adapter, Displacement};
use mp_core::axis::Axis;
use mp_core::linalg::Vec6;

/// What a tool does, independent of what it is called.
///
/// Named by effect rather than by tool name: `docs/02` S3 forbids the model
/// from depending on identifiers, and a tool named `safe_helper` that makes
/// outbound requests must price as an outbound request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    /// Reads data already inside the trust boundary.
    ReadLocal,
    /// Reads data from outside — web fetch, third-party API. Brings untrusted
    /// content in, which is the prompt-injection entry point.
    ReadExternal,
    /// Writes inside the trust boundary.
    WriteLocal,
    /// Sends data outward: HTTP POST, email, webhook, message to a third party.
    /// The one that cannot be retracted.
    SendExternal,
    /// Executes code or shell commands.
    Execute,
    /// Changes the agent's own configuration, permissions, or memory.
    SelfModify,
    /// Spawns or delegates to another agent.
    Delegate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub kind: ToolKind,
    /// Bytes of data the call moves. Used for the disclosure entropy on
    /// outbound calls.
    pub payload_bytes: u64,
    /// Distinct recipients the payload reaches. One outbound call to a
    /// thousand recipients is not one outbound call.
    pub recipients: u32,
    /// True when the call's arguments were derived from content the agent read
    /// from an external source during this session. This is the taint bit, and
    /// it is what makes prompt injection visible: the payload of an outbound
    /// call being derived from untrusted input is the signature of the attack.
    pub argument_tainted: bool,
    /// True when the tool's effect is not recorded in the session transcript.
    pub off_transcript: bool,
    /// Sensitivity of the data source, in bits of disclosure per byte.
    /// Measured from a classifier or from labels; defaults conservative.
    pub source_sensitivity: f64,
}

impl Default for ToolCall {
    fn default() -> Self {
        ToolCall {
            kind: ToolKind::ReadLocal,
            payload_bytes: 0,
            recipients: 0,
            argument_tainted: false,
            off_transcript: false,
            source_sensitivity: 0.01,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AgentAdapter {
    /// Cap on disclosure bits charged for a single outbound call, so one large
    /// transfer does not saturate the budget on its own and mask everything
    /// after it.
    pub max_disclosure_bits: f64,
}

impl AgentAdapter {
    pub fn new() -> Self {
        AgentAdapter { max_disclosure_bits: 20.0 }
    }
}

impl Adapter for AgentAdapter {
    type Request = ToolCall;

    fn name(&self) -> &'static str {
        "agent"
    }

    fn displacement(&self, c: &ToolCall, current: &Vec6) -> Displacement {
        let mut d = Displacement::zero();
        let cap = if self.max_disclosure_bits > 0.0 { self.max_disclosure_bits } else { 20.0 };

        match c.kind {
            ToolKind::ReadLocal => {
                d = d.with(Axis::Reach, 0.5);
            }
            ToolKind::ReadExternal => {
                // Brings untrusted content across the boundary. Cheap in
                // capability, but it is the step that arms everything after it.
                d = d.with(Axis::Reach, 1.0).with(Axis::Authority, 0.1);
            }
            ToolKind::WriteLocal => {
                d = d.with(Axis::Authority, 0.5).with(Axis::Irreversibility, 6.0);
            }
            ToolKind::SendExternal => {
                // Disclosure is irreversible: the recipient's knowledge cannot
                // be retracted. `docs/03` A3 gives the form —
                // log2(recipients × payload entropy).
                let payload_bits = (c.payload_bytes as f64 * c.source_sensitivity).max(1.0);
                let disclosure =
                    irreversibility_bits(payload_bits * c.recipients.max(1) as f64).min(cap);
                d = d.with(Axis::Irreversibility, disclosure).with(Axis::Reach, 2.0);
            }
            ToolKind::Execute => {
                d = d
                    .with(Axis::Authority, 3.0)
                    .with(Axis::Reach, 3.0)
                    .with(Axis::Irreversibility, 8.0);
            }
            ToolKind::SelfModify => {
                // An agent changing its own permissions is the escalation step.
                d = d.with(Axis::Authority, 5.0).with(Axis::Reach, 2.0);
            }
            ToolKind::Delegate => {
                // Delegation is the agent-domain coalition problem: capability
                // handed to another asker. Charged as coupling so the coalition
                // machinery in `docs/05` §5.7 picks it up.
                d = d.with(Axis::Coupling, 2.0).with(Axis::Reach, 1.5);
            }
        }

        // Taint. An outbound call whose arguments came from externally-read
        // content is the prompt-injection signature, and it is the single most
        // informative bit this adapter has.
        if c.argument_tainted {
            let multiplier = match c.kind {
                ToolKind::SendExternal | ToolKind::Execute | ToolKind::SelfModify => 3.0,
                _ => 1.0,
            };
            d = d
                .with(Axis::Authority, 1.0 * multiplier)
                .with(Axis::Irreversibility, 2.0 * multiplier);
        }

        if c.off_transcript {
            d = d.with(Axis::Opacity, opacity_bits(0.1));
        }

        amplify_by_reach(d, current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp_core::linalg::N;

    fn call(kind: ToolKind) -> ToolCall {
        ToolCall { kind, ..Default::default() }
    }

    #[test]
    fn a_local_read_is_cheap() {
        let d = AgentAdapter::new().displacement(&call(ToolKind::ReadLocal), &[0.0; N]);
        assert_eq!(d.get(Axis::Irreversibility), 0.0);
        assert!(d.get(Axis::Reach) <= 0.5);
    }

    #[test]
    fn sending_data_outward_is_irreversible_in_proportion_to_what_leaves() {
        let a = AgentAdapter::new();
        let small = ToolCall {
            kind: ToolKind::SendExternal,
            payload_bytes: 100,
            recipients: 1,
            source_sensitivity: 1.0,
            ..Default::default()
        };
        let large = ToolCall { payload_bytes: 10_000_000, ..small.clone() };
        assert!(
            a.displacement(&large, &[0.0; N]).get(Axis::Irreversibility)
                > a.displacement(&small, &[0.0; N]).get(Axis::Irreversibility)
        );
    }

    #[test]
    fn many_recipients_cost_more_than_one() {
        let a = AgentAdapter::new();
        let one = ToolCall {
            kind: ToolKind::SendExternal,
            payload_bytes: 1000,
            recipients: 1,
            source_sensitivity: 1.0,
            ..Default::default()
        };
        let many = ToolCall { recipients: 5000, ..one.clone() };
        assert!(
            a.displacement(&many, &[0.0; N]).get(Axis::Irreversibility)
                > a.displacement(&one, &[0.0; N]).get(Axis::Irreversibility)
        );
    }

    #[test]
    fn tainted_outbound_calls_cost_far_more_than_clean_ones() {
        // The prompt-injection signature: an outbound call whose arguments were
        // derived from externally-read content.
        let a = AgentAdapter::new();
        let clean = ToolCall {
            kind: ToolKind::SendExternal,
            payload_bytes: 1000,
            recipients: 1,
            ..Default::default()
        };
        let tainted = ToolCall { argument_tainted: true, ..clean.clone() };
        let c = a.displacement(&clean, &[0.0; N]);
        let t = a.displacement(&tainted, &[0.0; N]);
        assert!(t.get(Axis::Authority) > c.get(Axis::Authority) + 2.0);
        assert!(t.get(Axis::Irreversibility) > c.get(Axis::Irreversibility) + 5.0);
    }

    #[test]
    fn taint_matters_less_on_a_call_that_cannot_leak() {
        // A tainted local read is not the same event as a tainted outbound
        // send. The multiplier distinguishes them.
        let a = AgentAdapter::new();
        let read = ToolCall { kind: ToolKind::ReadLocal, argument_tainted: true, ..Default::default() };
        let send = ToolCall {
            kind: ToolKind::SendExternal,
            argument_tainted: true,
            payload_bytes: 10,
            recipients: 1,
            ..Default::default()
        };
        assert!(
            a.displacement(&send, &[0.0; N]).get(Axis::Authority)
                > a.displacement(&read, &[0.0; N]).get(Axis::Authority)
        );
    }

    #[test]
    fn the_exfiltration_chain_accumulates_across_individually_fine_steps() {
        // Read external content, read a local secret, send it out. Every step
        // is one an operator would approve on its own. The point is the sum.
        let a = AgentAdapter::new();
        let mut z = [0.0; N];
        let chain = [
            call(ToolKind::ReadExternal),
            call(ToolKind::ReadLocal),
            ToolCall {
                kind: ToolKind::SendExternal,
                payload_bytes: 50_000,
                recipients: 1,
                argument_tainted: true,
                source_sensitivity: 1.0,
                ..Default::default()
            },
        ];
        for c in &chain {
            let d = a.displacement(c, &z);
            for i in 0..N {
                z[i] += d.as_vec()[i];
            }
        }
        assert!(z[Axis::Irreversibility.index()] > 10.0);
        assert!(z[Axis::Reach.index()] > 2.0);
    }

    #[test]
    fn delegation_registers_as_coupling_so_coalitions_pick_it_up() {
        let d = AgentAdapter::new().displacement(&call(ToolKind::Delegate), &[0.0; N]);
        assert!(d.get(Axis::Coupling) > 0.0);
    }

    #[test]
    fn self_modification_is_the_most_authority_expensive_step() {
        let a = AgentAdapter::new();
        let sm = a.displacement(&call(ToolKind::SelfModify), &[0.0; N]);
        for k in [ToolKind::ReadLocal, ToolKind::ReadExternal, ToolKind::WriteLocal] {
            assert!(sm.get(Axis::Authority) > a.displacement(&call(k), &[0.0; N]).get(Axis::Authority));
        }
    }

    #[test]
    fn off_transcript_calls_register_as_opacity() {
        let a = AgentAdapter::new();
        let c = ToolCall { off_transcript: true, ..call(ToolKind::Execute) };
        assert!(a.displacement(&c, &[0.0; N]).get(Axis::Opacity) > 3.0);
    }
}

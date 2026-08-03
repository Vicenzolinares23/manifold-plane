//! Industrial control systems adapter — Modbus and DNP3.
//!
//! The literal controllers. This is the domain where `docs/01` I3
//! (irreversibility) stops being an abstraction: a setpoint write moves fluid,
//! heats a vessel, or opens a breaker, and there is no undo. Closing the valve
//! again does not un-move the fluid.
//!
//! It is also the domain where `docs/04`'s continuous-time limit matters.
//! Setpoint streams are genuinely continuous, not discrete events, so the
//! sampling-artifact warning from C1 in `docs/01` is live here rather than
//! theoretical.

use crate::{amplify_by_reach, irreversibility_bits, Adapter, Displacement};
use mp_core::axis::Axis;
use mp_core::linalg::Vec6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionCode {
    ReadCoils,
    ReadDiscreteInputs,
    ReadHoldingRegisters,
    ReadInputRegisters,
    WriteSingleCoil,
    WriteSingleRegister,
    WriteMultipleCoils,
    WriteMultipleRegisters,
    /// DNP3 direct operate — actuation without a select step.
    DirectOperate,
    /// DNP3 select-before-operate: the select half.
    Select,
    /// Diagnostic and device-management codes. These are the ones that can
    /// disable logging or reset a device's state, which is why they are opacity
    /// rather than authority.
    Diagnostic,
    /// Firmware or configuration download.
    ConfigWrite,
}

impl FunctionCode {
    pub fn is_write(self) -> bool {
        !matches!(
            self,
            FunctionCode::ReadCoils
                | FunctionCode::ReadDiscreteInputs
                | FunctionCode::ReadHoldingRegisters
                | FunctionCode::ReadInputRegisters
                | FunctionCode::Select
        )
    }
}

/// How much of the physical process a point governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointCriticality {
    /// Indication only; no actuation.
    Telemetry,
    /// Local actuation with a mechanical or interlock backstop.
    Interlocked,
    /// Local actuation, no backstop.
    Direct,
    /// Safety instrumented function — the layer that exists to prevent harm.
    /// A write here is not an operation, it is the removal of a safeguard.
    SafetyFunction,
}

impl PointCriticality {
    /// Entropy of the physical state the write destroys (`docs/03` A3).
    ///
    /// Capped at the measured process entropy rather than treated as infinite,
    /// as `docs/03` requires. Values here are the conservative defaults a
    /// deployment overrides from its own process model — an unconfigured
    /// installation should be too strict, never too permissive.
    fn process_entropy(self) -> f64 {
        match self {
            PointCriticality::Telemetry => 1.0,
            PointCriticality::Interlocked => 16.0,
            PointCriticality::Direct => 4096.0,
            PointCriticality::SafetyFunction => 1_048_576.0,
        }
    }

    fn reach_bits(self) -> f64 {
        match self {
            PointCriticality::Telemetry => 0.0,
            PointCriticality::Interlocked => 1.0,
            PointCriticality::Direct => 3.0,
            PointCriticality::SafetyFunction => 6.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IcsRequest {
    pub function: FunctionCode,
    pub criticality: PointCriticality,
    /// Number of registers or coils the request spans.
    pub point_count: u32,
    /// Magnitude of the commanded change as a fraction of the point's full
    /// operating range, in `[0,1]`. Zero for reads.
    pub excursion_fraction: f64,
    /// True when the write is outside the range the process has historically
    /// operated in — the ICS analogue of the orbit residual, available here
    /// because the physical process supplies its own baseline.
    pub outside_historical_envelope: bool,
    /// True when the command bypasses select-before-operate.
    pub bypasses_sbo: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IcsAdapter {
    /// Total points on the segment, for normalizing span.
    pub segment_points: u32,
}

impl IcsAdapter {
    pub fn new(segment_points: u32) -> Self {
        IcsAdapter { segment_points: segment_points.max(1) }
    }
}

impl Adapter for IcsAdapter {
    type Request = IcsRequest;

    fn name(&self) -> &'static str {
        "ics"
    }

    fn displacement(&self, r: &IcsRequest, current: &Vec6) -> Displacement {
        let mut d = Displacement::zero();

        if !r.function.is_write() {
            // Reads still carry reach — knowing the process state is real
            // reconnaissance — but no irreversibility and no authority.
            d = d.with(Axis::Reach, r.criticality.reach_bits() * 0.1);
            return amplify_by_reach(d, current);
        }

        // Authority: writing at all, scaled by how many points are spanned.
        let span = (1.0 + r.point_count as f64).log2();
        d = d.with(Axis::Authority, 0.5 + 0.5 * span);

        // Reach: criticality, scaled by the fraction of the segment touched.
        let seg_frac =
            (r.point_count as f64 / self.segment_points as f64).clamp(0.0, 1.0);
        d = d.with(Axis::Reach, r.criticality.reach_bits() * (0.5 + 0.5 * seg_frac));

        // Irreversibility: the physical process entropy, scaled by how far the
        // command actually moves the process. A write that changes nothing
        // destroys nothing.
        let excursion = r.excursion_fraction.clamp(0.0, 1.0);
        let entropy = r.criticality.process_entropy().powf(excursion.max(1e-3));
        d = d.with(Axis::Irreversibility, irreversibility_bits(entropy));

        // Opacity: diagnostics can silence a device; SBO bypass removes the
        // operator-confirmation record that would otherwise exist.
        if matches!(r.function, FunctionCode::Diagnostic | FunctionCode::ConfigWrite) {
            d = d.with(Axis::Opacity, 3.0);
        }
        if r.bypasses_sbo {
            d = d.with(Axis::Opacity, 2.0);
        }

        // Operating outside the historical envelope is the physical process
        // telling us the symmetry it normally exhibits has broken.
        if r.outside_historical_envelope {
            d = d.with(Axis::Reach, 2.0).with(Axis::Irreversibility, 2.0);
        }

        amplify_by_reach(d, current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mp_core::linalg::N;

    fn adapter() -> IcsAdapter {
        IcsAdapter::new(256)
    }

    fn read() -> IcsRequest {
        IcsRequest {
            function: FunctionCode::ReadHoldingRegisters,
            criticality: PointCriticality::Telemetry,
            point_count: 4,
            excursion_fraction: 0.0,
            outside_historical_envelope: false,
            bypasses_sbo: false,
        }
    }

    fn write(crit: PointCriticality, excursion: f64) -> IcsRequest {
        IcsRequest {
            function: FunctionCode::WriteSingleRegister,
            criticality: crit,
            point_count: 1,
            excursion_fraction: excursion,
            outside_historical_envelope: false,
            bypasses_sbo: false,
        }
    }

    #[test]
    fn a_read_destroys_nothing() {
        let d = adapter().displacement(&read(), &[0.0; N]);
        assert_eq!(d.get(Axis::Irreversibility), 0.0);
        assert_eq!(d.get(Axis::Authority), 0.0);
    }

    #[test]
    fn a_read_still_costs_a_little_reach() {
        // Reconnaissance is not free even when it changes nothing.
        let mut r = read();
        r.criticality = PointCriticality::Direct;
        let d = adapter().displacement(&r, &[0.0; N]);
        assert!(d.get(Axis::Reach) > 0.0);
    }

    #[test]
    fn writing_a_safety_function_dominates_writing_telemetry() {
        let t = adapter().displacement(&write(PointCriticality::Telemetry, 1.0), &[0.0; N]);
        let s = adapter().displacement(&write(PointCriticality::SafetyFunction, 1.0), &[0.0; N]);
        assert!(s.get(Axis::Irreversibility) > 15.0);
        assert!(s.get(Axis::Irreversibility) > t.get(Axis::Irreversibility) * 10.0);
    }

    #[test]
    fn a_write_that_barely_moves_the_process_barely_costs_anything() {
        // The excursion scaling. Nudging a setpoint 0.1% is not the same event
        // as slamming it to a rail, and a model that priced them identically
        // would be unusable in a real plant.
        let small = adapter().displacement(&write(PointCriticality::Direct, 0.001), &[0.0; N]);
        let full = adapter().displacement(&write(PointCriticality::Direct, 1.0), &[0.0; N]);
        assert!(small.get(Axis::Irreversibility) < 0.5);
        assert!(full.get(Axis::Irreversibility) > 11.0);
    }

    #[test]
    fn the_slow_setpoint_walk_accumulates() {
        // The attack this adapter exists for: a hundred writes each moving the
        // process 1% of range. Every one is inside limits; the sum is not.
        // Summing displacements is legitimate because the axes are in bits.
        let a = adapter();
        let mut total = 0.0;
        for _ in 0..100 {
            total += a
                .displacement(&write(PointCriticality::Direct, 0.01), &[0.0; N])
                .get(Axis::Irreversibility);
        }
        assert!(total > 5.0, "a hundred 1% steps accumulated only {total} bits");
    }

    #[test]
    fn bypassing_select_before_operate_registers_as_opacity() {
        let mut r = write(PointCriticality::Direct, 0.5);
        r.bypasses_sbo = true;
        let d = adapter().displacement(&r, &[0.0; N]);
        assert!(d.get(Axis::Opacity) >= 2.0);
    }

    #[test]
    fn leaving_the_historical_envelope_costs_extra() {
        let inside = adapter().displacement(&write(PointCriticality::Direct, 0.5), &[0.0; N]);
        let mut r = write(PointCriticality::Direct, 0.5);
        r.outside_historical_envelope = true;
        let outside = adapter().displacement(&r, &[0.0; N]);
        assert!(outside.get(Axis::Reach) > inside.get(Axis::Reach) + 1.0);
    }

    #[test]
    fn a_multi_point_write_spans_more_than_a_single_point() {
        let a = adapter();
        let mut one = write(PointCriticality::Direct, 0.5);
        one.point_count = 1;
        let mut many = one.clone();
        many.point_count = 200;
        assert!(
            a.displacement(&many, &[0.0; N]).get(Axis::Authority)
                > a.displacement(&one, &[0.0; N]).get(Axis::Authority)
        );
    }
}

//! Core state space and geometry for manifold-plane.
//!
//! See `docs/` for the derivation. Nothing in this crate is a design choice that
//! was not forced by an invariant, a symmetry, or a dimensional argument
//! recorded there.

// Fixed-size 6x6 arithmetic. Indexed loops mirror the index notation in
// docs/ line for line, which matters more here than iterator idiom: these
// routines are meant to be checked against the mathematics by hand.
#![allow(clippy::needless_range_loop)]

pub mod axis;
pub mod linalg;
pub mod metric;
pub mod state;

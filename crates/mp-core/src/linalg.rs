//! Fixed-size 6x6 symmetric linear algebra.
//!
//! Hand-rolled rather than pulled from a crate. The dimension is fixed at 6 by
//! `docs/04-state-variables.md`, everything is small enough to unroll, and the
//! safety argument in `docs/06-proofs.md` is only as trustworthy as the code
//! underneath it. Zero dependencies means the audit surface is this file.

/// State-space dimension. Fixed by the axis derivation in `docs/04`, not chosen.
pub const N: usize = 6;

/// A vector in the 6-dimensional capability space. Components are in bits.
pub type Vec6 = [f64; N];

/// A 6x6 matrix, row-major.
pub type Mat6 = [[f64; N]; N];

pub const ZERO_V: Vec6 = [0.0; N];

pub fn identity() -> Mat6 {
    let mut m = [[0.0; N]; N];
    for i in 0..N {
        m[i][i] = 1.0;
    }
    m
}

pub fn diag(d: &Vec6) -> Mat6 {
    let mut m = [[0.0; N]; N];
    for i in 0..N {
        m[i][i] = d[i];
    }
    m
}

pub fn sub(a: &Vec6, b: &Vec6) -> Vec6 {
    let mut r = ZERO_V;
    for i in 0..N {
        r[i] = a[i] - b[i];
    }
    r
}

pub fn add(a: &Vec6, b: &Vec6) -> Vec6 {
    let mut r = ZERO_V;
    for i in 0..N {
        r[i] = a[i] + b[i];
    }
    r
}

pub fn scale(a: &Vec6, s: f64) -> Vec6 {
    let mut r = ZERO_V;
    for i in 0..N {
        r[i] = a[i] * s;
    }
    r
}

pub fn dot(a: &Vec6, b: &Vec6) -> f64 {
    let mut s = 0.0;
    for i in 0..N {
        s += a[i] * b[i];
    }
    s
}

pub fn matvec(m: &Mat6, v: &Vec6) -> Vec6 {
    let mut r = ZERO_V;
    for i in 0..N {
        let mut s = 0.0;
        for j in 0..N {
            s += m[i][j] * v[j];
        }
        r[i] = s;
    }
    r
}

pub fn matmul(a: &Mat6, b: &Mat6) -> Mat6 {
    let mut r = [[0.0; N]; N];
    for i in 0..N {
        for k in 0..N {
            let aik = a[i][k];
            if aik == 0.0 {
                continue;
            }
            for j in 0..N {
                r[i][j] += aik * b[k][j];
            }
        }
    }
    r
}

pub fn transpose(a: &Mat6) -> Mat6 {
    let mut r = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            r[i][j] = a[j][i];
        }
    }
    r
}

pub fn mat_add(a: &Mat6, b: &Mat6) -> Mat6 {
    let mut r = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            r[i][j] = a[i][j] + b[i][j];
        }
    }
    r
}

pub fn mat_sub(a: &Mat6, b: &Mat6) -> Mat6 {
    let mut r = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            r[i][j] = a[i][j] - b[i][j];
        }
    }
    r
}

pub fn mat_scale(a: &Mat6, s: f64) -> Mat6 {
    let mut r = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            r[i][j] = a[i][j] * s;
        }
    }
    r
}

pub fn trace(a: &Mat6) -> f64 {
    (0..N).map(|i| a[i][i]).sum()
}

pub fn frobenius(a: &Mat6) -> f64 {
    let mut s = 0.0;
    for i in 0..N {
        for j in 0..N {
            s += a[i][j] * a[i][j];
        }
    }
    s.sqrt()
}

/// Force exact symmetry. Repeated arithmetic drifts, and every downstream
/// routine here assumes `A == Aᵀ` exactly.
pub fn symmetrize(a: &Mat6) -> Mat6 {
    let mut r = [[0.0; N]; N];
    for i in 0..N {
        for j in 0..N {
            r[i][j] = 0.5 * (a[i][j] + a[j][i]);
        }
    }
    r
}

/// Quadratic form `vᵀ A v`.
pub fn quad(a: &Mat6, v: &Vec6) -> f64 {
    dot(v, &matvec(a, v))
}

/// Symmetric eigendecomposition by the cyclic Jacobi method.
///
/// Returns `(eigenvalues, eigenvectors)` where column `k` of the eigenvector
/// matrix corresponds to eigenvalue `k`. Jacobi is chosen over anything
/// fancier because at n=6 it converges in a handful of sweeps and is short
/// enough to read and verify by hand.
pub fn eigh(a: &Mat6) -> (Vec6, Mat6) {
    let mut m = symmetrize(a);
    let mut v = identity();

    for _sweep in 0..64 {
        // Off-diagonal magnitude; stop once it is at rounding level.
        let mut off = 0.0;
        for i in 0..N {
            for j in (i + 1)..N {
                off += m[i][j] * m[i][j];
            }
        }
        if off <= 1e-30 {
            break;
        }

        for p in 0..N {
            for q in (p + 1)..N {
                if m[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (m[q][q] - m[p][p]) / (2.0 * m[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;

                for k in 0..N {
                    let mkp = m[k][p];
                    let mkq = m[k][q];
                    m[k][p] = c * mkp - s * mkq;
                    m[k][q] = s * mkp + c * mkq;
                }
                for k in 0..N {
                    let mpk = m[p][k];
                    let mqk = m[q][k];
                    m[p][k] = c * mpk - s * mqk;
                    m[q][k] = s * mpk + c * mqk;
                }
                for k in 0..N {
                    let vkp = v[k][p];
                    let vkq = v[k][q];
                    v[k][p] = c * vkp - s * vkq;
                    v[k][q] = s * vkp + c * vkq;
                }
            }
        }
    }

    let mut vals = ZERO_V;
    for i in 0..N {
        vals[i] = m[i][i];
    }
    (vals, v)
}

/// Rebuild a symmetric matrix from an eigenbasis with the eigenvalues passed
/// through `f`. This is how every spectral projection in `metric.rs` is done.
pub fn spectral_map<F: Fn(f64) -> f64>(a: &Mat6, f: F) -> Mat6 {
    let (vals, vecs) = eigh(a);
    let mut d = [[0.0; N]; N];
    for i in 0..N {
        d[i][i] = f(vals[i]);
    }
    let vt = transpose(&vecs);
    symmetrize(&matmul(&matmul(&vecs, &d), &vt))
}

/// Smallest eigenvalue. Positive-definiteness checks route through here.
pub fn min_eigenvalue(a: &Mat6) -> f64 {
    let (vals, _) = eigh(a);
    vals.iter().cloned().fold(f64::INFINITY, f64::min)
}

pub fn max_eigenvalue(a: &Mat6) -> f64 {
    let (vals, _) = eigh(a);
    vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
}

/// Inverse of a symmetric positive-definite matrix via its spectrum.
/// Returns `None` if any eigenvalue is at or below `tol` — a singular metric
/// is a calibration failure, not something to paper over with a pseudoinverse.
pub fn inv_spd(a: &Mat6, tol: f64) -> Option<Mat6> {
    let (vals, vecs) = eigh(a);
    if vals.iter().any(|&l| l <= tol) {
        return None;
    }
    let mut d = [[0.0; N]; N];
    for i in 0..N {
        d[i][i] = 1.0 / vals[i];
    }
    let vt = transpose(&vecs);
    Some(symmetrize(&matmul(&matmul(&vecs, &d), &vt)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn eigh_recovers_a_known_diagonal_matrix() {
        let d = [3.0, 1.0, 4.0, 1.0, 5.0, 9.0];
        let (vals, _) = eigh(&diag(&d));
        let mut got: Vec<f64> = vals.to_vec();
        let mut want = d.to_vec();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (g, w) in got.iter().zip(want.iter()) {
            assert!(approx(*g, *w, 1e-9), "{g} vs {w}");
        }
    }

    #[test]
    fn eigh_reconstructs_the_original_matrix() {
        let mut a = identity();
        a[0][1] = 0.4;
        a[1][0] = 0.4;
        a[2][5] = -0.3;
        a[5][2] = -0.3;
        a[3][3] = 2.5;
        let rebuilt = spectral_map(&a, |l| l);
        for i in 0..N {
            for j in 0..N {
                assert!(approx(rebuilt[i][j], a[i][j], 1e-9));
            }
        }
    }

    #[test]
    fn inv_spd_round_trips() {
        let mut a = identity();
        a[0][0] = 4.0;
        a[1][1] = 0.5;
        a[0][1] = 0.2;
        a[1][0] = 0.2;
        let inv = inv_spd(&a, 1e-12).expect("spd");
        let prod = matmul(&a, &inv);
        for i in 0..N {
            for j in 0..N {
                let want = if i == j { 1.0 } else { 0.0 };
                assert!(approx(prod[i][j], want, 1e-9));
            }
        }
    }

    #[test]
    fn inv_spd_refuses_a_singular_matrix() {
        let mut a = identity();
        a[3][3] = 0.0;
        assert!(inv_spd(&a, 1e-12).is_none());
    }

    #[test]
    fn quadratic_form_matches_hand_computation() {
        let a = diag(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let v = [1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        assert!(approx(quad(&a, &v), 21.0, 1e-12));
    }
}

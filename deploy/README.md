# deploy

## TLS

The daemon speaks plain HTTP. Kubernetes admission webhooks require HTTPS, so
terminate TLS in a sidecar and forward to the daemon over loopback. The same
arrangement covers the operator's API-server access.

This is a deliberate boundary, not an omission. A half-built TLS stack inside a
security component is worse than none, and the sidecar pattern is well
understood. `crates/mp-daemon/src/http.rs` says the same thing at the source.

## Order of operations

1. `kubectl apply -f crd.yaml`
2. Run the daemon in **observe** mode against real traffic and collect
   displacement vectors.
3. Calibrate `M` and `c` from that corpus. Until this happens, `/readyz` is 503
   and the operator reports `Degraded`, both on purpose.
4. Publish the result as the ConfigMap named in `calibrationConfigMap`.
5. `kubectl apply -f policy-example.yaml`, adjusted for your workloads.
6. Register the webhook, starting with `failurePolicy: Ignore`, and watch the
   `manifold_plane_decisions_total` metric before switching to `Fail`.

Step 3 is not optional. An uncalibrated deployment either denies everything or
means nothing, depending on the budget you guessed.

## Sizing symmetry classes

Below five members a class yields no orbit residual (`mp_barrier::orbit::MIN_PEERS`)
and its members fall back entirely on their own baselines — the weakest
configuration available, per `docs/07` F2. The operator reports undersized
classes rather than silently degrading.

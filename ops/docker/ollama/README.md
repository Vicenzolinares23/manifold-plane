# Ollama container

Serves a local 8B model so `examples/agent_harness.py` can drive a *real*
agent against the daemon instead of replaying synthetic tool calls.

This matters for validating the agent adapter. Synthetic traces are written by
someone who already knows what the detector looks for, which makes them
worthless as evidence. A real model deciding its own tool sequence produces
displacement trajectories nobody designed, and those are the ones worth
measuring against `docs/07`.

## Build

```bash
DOCKER_BUILDKIT=1 docker compose build ollama
```

`llama3:8b` is pulled during the build and baked into the image.

Different model:

```bash
docker compose build --build-arg MODELS="mistral:7b llama3:8b" ollama
```

Skip the pre-pull (models fetch on first request, slower but a much smaller
image):

```bash
docker compose build --build-arg PRE_PULL_MODELS=false ollama
```

## Note on size

An 8B model at 4-bit quantization is roughly 4.7 GB, and it lands in the image
layer rather than in a volume. That is the deliberate trade — reproducibility
over image size. If image size matters more in your environment, set
`PRE_PULL_MODELS=false` and let the named volume carry the weights.

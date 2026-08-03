from manifold_agent.scripts.seed_corpus import seed_rows
from manifold_agent.training.dataset import counts_by_split, samples_from_seed


def test_seed_has_exfil_chain():
    kinds = [r["label"]["kind"] for r in seed_rows()]
    assert "ReadExternal" in kinds
    assert "ReadLocal" in kinds
    assert "SendExternal" in kinds


def test_dataset_splits():
    samples = samples_from_seed(seed_rows())
    counts = counts_by_split(samples)
    assert sum(counts.values()) == len(samples)
    assert counts["train"] >= 1

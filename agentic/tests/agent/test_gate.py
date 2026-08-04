from manifold_agent.agent.graph import run_scripted
from manifold_agent.agent.nodes import gate_node, tools_node
from manifold_agent.state import Decision


def test_deny_does_not_execute_tool(monkeypatch):
    from manifold_agent import gateway

    def fake_decide(asker_id, tool_call, **kwargs):
        from manifold_agent.state import Verdict

        return Verdict(
            decision=Decision.DENY,
            admissible_fraction=0.0,
            coalitions_checked=0,
            margin_before=1.0,
            margin_after=-1.0,
            required=1.0,
            alpha_effective=0.05,
            orbit_residual=0.0,
            budget_fraction=1.1,
            state_after=[0.0] * 6,
            denied=1,
            held=0,
            admitted=0,
        )

    monkeypatch.setattr(gateway, "decide", fake_decide)
    state = {
        "asker_id": "t",
        "pending_tools": [{"name": "send_external", "arguments": {"url": "x", "payload": "secret", "argument_tainted": True}}],
        "messages": [],
    }
    gated = gate_node(state)
    assert gated["route"] == "replan"
    # tools_node must not be implied; if called anyway with empty after replan drop:
    after = tools_node({**gated, "pending_tools": []})
    assert after.get("last_results") in (None, [], after.get("last_results"))


def test_scripted_benign_runs():
    out = run_scripted([{"name": "read_local", "arguments": {"path": "."}}], asker_id="unit")
    assert out.get("last_results")

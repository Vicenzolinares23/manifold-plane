from manifold_agent.tools.registry import load_all_tools, tool_names


def test_registry_loads_tool_kinds():
    load_all_tools()
    names = set(tool_names())
    for expected in (
        "read_local",
        "read_external",
        "write_local",
        "send_external",
        "execute",
        "self_modify",
        "delegate",
        "remember",
        "recall",
        "forget",
    ):
        assert expected in names

"""Compile the Stage 8 LangGraph agent."""

from __future__ import annotations

from typing import Any

from langgraph.graph import END, START, StateGraph

from manifold_agent.agent.nodes import (
    AgentState,
    agent_node,
    gate_node,
    hold_node,
    memory_node,
    replan_node,
    route_after_agent,
    route_from_gate,
    tools_node,
)
from manifold_agent.memory.session import build_checkpointer


def build_graph(*, checkpointer: Any | None = None, database_url: str | None = None):
    g = StateGraph(AgentState)
    g.add_node("agent", agent_node)
    g.add_node("gate", gate_node)
    g.add_node("tools", tools_node)
    g.add_node("memory", memory_node)
    g.add_node("replan", replan_node)
    g.add_node("hold", hold_node)

    g.add_edge(START, "agent")
    g.add_conditional_edges(
        "agent",
        route_after_agent,
        {"gate": "gate", "end": END},
    )
    g.add_conditional_edges(
        "gate",
        route_from_gate,
        {"tools": "tools", "replan": "replan", "hold": "hold", "end": END},
    )
    g.add_edge("tools", "memory")
    g.add_edge("memory", "agent")
    g.add_edge("replan", "agent")
    g.add_edge("hold", END)

    saver = checkpointer if checkpointer is not None else build_checkpointer(database_url)
    return g.compile(checkpointer=saver)


def run_scripted(
    pending_tools: list[dict[str, Any]],
    *,
    asker_id: str = "demo-agent",
    symmetry_class: str = "default",
) -> AgentState:
    """Drive the graph without an LLM — used by demos and tests."""
    graph = build_graph()
    state: AgentState = {
        "asker_id": asker_id,
        "symmetry_class": symmetry_class,
        "messages": [{"role": "user", "content": "scripted run"}],
        "pending_tools": pending_tools,
        "last_results": [],
    }
    # LangGraph needs a thread_id when a checkpointer is present.
    return graph.invoke(state, config={"configurable": {"thread_id": f"script-{asker_id}"}})

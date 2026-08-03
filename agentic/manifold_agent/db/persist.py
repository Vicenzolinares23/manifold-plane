"""Persist Stage 8 audit rows: tool_events, guardrail_events, memory."""

from __future__ import annotations

import uuid

from manifold_agent.state import GuardrailReport, MemoryEntry, ToolCall, Verdict


def ensure_asker_and_session(
    *,
    asker_key: str,
    thread_id: str,
    symmetry_class: str = "default",
) -> tuple[uuid.UUID, uuid.UUID] | None:
    """Return (asker.id, session.id) or None if Postgres is unavailable."""
    try:
        from sqlalchemy import select

        from manifold_agent.db.models import Asker, SessionRow
        from manifold_agent.db.session import session_scope
    except Exception:
        return None

    try:
        with session_scope() as db:
            asker = db.scalar(select(Asker).where(Asker.asker_id == asker_key))
            if asker is None:
                asker = Asker(asker_id=asker_key, symmetry_class=symmetry_class)
                db.add(asker)
                db.flush()
            session = db.scalar(select(SessionRow).where(SessionRow.thread_id == thread_id))
            if session is None:
                session = SessionRow(asker_id=asker.id, thread_id=thread_id)
                db.add(session)
                db.flush()
            return asker.id, session.id
    except Exception:
        return None


def record_tool_event(
    *,
    asker_key: str,
    thread_id: str,
    tool_call: ToolCall,
    verdict: Verdict | None,
    result: str | None = None,
    symmetry_class: str = "default",
) -> None:
    ids = ensure_asker_and_session(
        asker_key=asker_key, thread_id=thread_id, symmetry_class=symmetry_class
    )
    if ids is None or verdict is None:
        return
    asker_id, session_id = ids
    try:
        from manifold_agent.db.models import ToolEvent
        from manifold_agent.db.session import session_scope

        with session_scope() as db:
            db.add(
                ToolEvent(
                    session_id=session_id,
                    asker_id=asker_id,
                    tool_name=tool_call.name,
                    kind=tool_call.kind.value,
                    arguments=dict(tool_call.arguments),
                    payload_bytes=tool_call.payload_bytes,
                    recipients=tool_call.recipients,
                    argument_tainted=tool_call.argument_tainted,
                    off_transcript=tool_call.off_transcript,
                    source_sensitivity=tool_call.source_sensitivity,
                    decision=verdict.decision.value,
                    margin_before=verdict.margin_before,
                    margin_after=verdict.margin_after,
                    required=verdict.required,
                    alpha_effective=verdict.alpha_effective,
                    orbit_residual=verdict.orbit_residual,
                    budget_fraction=verdict.budget_fraction,
                    admissible_fraction=verdict.admissible_fraction,
                    blocked_by_coalition=verdict.blocked_by_coalition,
                    z_after=verdict.state_after,
                    result=result,
                )
            )
    except Exception:
        return


def record_guardrail_event(
    *,
    asker_key: str,
    thread_id: str,
    stage: str,
    report: GuardrailReport,
    symmetry_class: str = "default",
) -> None:
    ids = ensure_asker_and_session(
        asker_key=asker_key, thread_id=thread_id, symmetry_class=symmetry_class
    )
    if ids is None:
        return
    asker_id, session_id = ids
    try:
        from manifold_agent.db.models import GuardrailEvent
        from manifold_agent.db.session import session_scope

        with session_scope() as db:
            db.add(
                GuardrailEvent(
                    session_id=session_id,
                    asker_id=asker_id,
                    stage=stage,
                    allowed=report.allowed,
                    risk=report.risk,
                    reason=report.reason,
                    model=report.model,
                    details=dict(report.details),
                )
            )
    except Exception:
        return


def record_memory(
    *,
    asker_key: str,
    thread_id: str,
    entry: MemoryEntry,
    symmetry_class: str = "default",
) -> None:
    ids = ensure_asker_and_session(
        asker_key=asker_key, thread_id=thread_id, symmetry_class=symmetry_class
    )
    if ids is None:
        return
    asker_id, session_id = ids
    try:
        from manifold_agent.db.models import MemoryEntryRow
        from manifold_agent.db.session import session_scope

        with session_scope() as db:
            db.add(
                MemoryEntryRow(
                    asker_id=asker_id,
                    session_id=session_id,
                    kind=entry.kind.value,
                    content=entry.content,
                    importance=entry.importance,
                    scope=entry.scope,
                    ttl_secs=entry.ttl_secs,
                    metadata_=dict(entry.metadata) | {"key": entry.key},
                )
            )
    except Exception:
        return

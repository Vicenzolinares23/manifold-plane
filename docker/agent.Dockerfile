# Agent image for the LangGraph layer.
FROM python:3.12-slim
WORKDIR /app
COPY agentic /app
RUN pip install --no-cache-dir -e ".[test]"
ENV MP_ENGINE_URL=http://engine:8787
ENV MP_DATABASE_URL=postgresql+psycopg://manifold:manifold@postgres:5432/manifold_plane
CMD ["python", "-m", "manifold_agent.examples.demo_agent"]

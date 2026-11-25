-- Phase 2：AI 请求审计记录（不保存密钥与原始 TLE）。
CREATE TABLE IF NOT EXISTS ai_requests (
    id UUID PRIMARY KEY,
    org_id UUID REFERENCES organizations (id) ON DELETE SET NULL,
    created_by UUID REFERENCES users (id) ON DELETE SET NULL,
    kind TEXT NOT NULL,
    preset TEXT NOT NULL,
    job_id UUID REFERENCES computation_jobs (id) ON DELETE SET NULL,
    scenario_id UUID REFERENCES scenarios (id) ON DELETE SET NULL,
    model TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    error TEXT,
    latency_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_ai_requests_org ON ai_requests (org_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_requests_job ON ai_requests (job_id);

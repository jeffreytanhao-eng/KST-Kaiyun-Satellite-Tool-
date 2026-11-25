-- Phase 1：场景（对象以 JSONB 存储）与异步计算任务。

CREATE TABLE IF NOT EXISTS scenarios (
    id UUID PRIMARY KEY,
    org_id UUID REFERENCES organizations (id) ON DELETE SET NULL,
    created_by UUID REFERENCES users (id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    data JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_scenarios_org ON scenarios (org_id);
CREATE INDEX IF NOT EXISTS idx_scenarios_created_by ON scenarios (created_by);

CREATE TABLE IF NOT EXISTS computation_jobs (
    id UUID PRIMARY KEY,
    org_id UUID REFERENCES organizations (id) ON DELETE SET NULL,
    created_by UUID REFERENCES users (id) ON DELETE SET NULL,
    scenario_id UUID NOT NULL REFERENCES scenarios (id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    params JSONB NOT NULL DEFAULT '{}'::jsonb,
    result JSONB,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_jobs_scenario ON computation_jobs (scenario_id, created_at DESC);

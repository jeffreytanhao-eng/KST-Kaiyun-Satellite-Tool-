-- Phase 5：组织角色、场景共享与评论、任务增强、资源配额、审计与 AI 解读。

-- 组织角色（admin / analyst / viewer）与用户级配额覆盖。
ALTER TABLE users ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'analyst';
ALTER TABLE users ADD COLUMN IF NOT EXISTS max_concurrent_jobs INTEGER;
ALTER TABLE users ADD COLUMN IF NOT EXISTS max_daily_jobs INTEGER;
ALTER TABLE users ADD COLUMN IF NOT EXISTS max_ai_daily INTEGER;

DO $$ BEGIN
    ALTER TABLE users
        ADD CONSTRAINT users_role_check CHECK (role IN ('admin', 'analyst', 'viewer'));
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

-- 兼容既有部署：首个创建的用户提升为管理员（仅在尚无管理员时执行）。
UPDATE users SET role = 'admin'
WHERE id = (SELECT id FROM users ORDER BY created_at ASC LIMIT 1)
  AND NOT EXISTS (SELECT 1 FROM users WHERE role = 'admin');

-- 组织级配额。
ALTER TABLE organizations ADD COLUMN IF NOT EXISTS max_concurrent_jobs INTEGER NOT NULL DEFAULT 5;
ALTER TABLE organizations ADD COLUMN IF NOT EXISTS max_daily_jobs INTEGER NOT NULL DEFAULT 500;
ALTER TABLE organizations ADD COLUMN IF NOT EXISTS max_ai_daily INTEGER NOT NULL DEFAULT 200;

-- 任务增强：进度、缓存、取消标记。
ALTER TABLE computation_jobs ADD COLUMN IF NOT EXISTS progress INTEGER NOT NULL DEFAULT 0;
ALTER TABLE computation_jobs ADD COLUMN IF NOT EXISTS cache_key TEXT;
ALTER TABLE computation_jobs ADD COLUMN IF NOT EXISTS cache_source_id UUID;
ALTER TABLE computation_jobs ADD COLUMN IF NOT EXISTS cached BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE computation_jobs ADD COLUMN IF NOT EXISTS cancel_requested BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE computation_jobs ADD COLUMN IF NOT EXISTS cancelled_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_jobs_cache ON computation_jobs (org_id, cache_key, status, finished_at DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_user ON computation_jobs (created_by, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_jobs_active ON computation_jobs (status) WHERE status IN ('queued', 'running');
CREATE INDEX IF NOT EXISTS idx_jobs_finished ON computation_jobs (finished_at);

-- 场景共享（read 只读 / write 可编辑）。
CREATE TABLE IF NOT EXISTS scenario_shares (
    id UUID PRIMARY KEY,
    scenario_id UUID NOT NULL REFERENCES scenarios (id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    permission TEXT NOT NULL DEFAULT 'read' CHECK (permission IN ('read', 'write')),
    created_by UUID REFERENCES users (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (scenario_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_scenario_shares_user ON scenario_shares (user_id);
CREATE INDEX IF NOT EXISTS idx_scenario_shares_scenario ON scenario_shares (scenario_id);

-- 场景评论（协作基础）。
CREATE TABLE IF NOT EXISTS scenario_comments (
    id UUID PRIMARY KEY,
    scenario_id UUID NOT NULL REFERENCES scenarios (id) ON DELETE CASCADE,
    user_id UUID REFERENCES users (id) ON DELETE SET NULL,
    body TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_scenario_comments_scenario ON scenario_comments (scenario_id, created_at DESC);

-- 审计日志（共享、权限、配额、取消等敏感操作）。
CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY,
    org_id UUID REFERENCES organizations (id) ON DELETE SET NULL,
    user_id UUID REFERENCES users (id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id UUID,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_audit_org ON audit_logs (org_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_resource ON audit_logs (resource_type, resource_id);

-- AI 解读结果（关联原分析任务，便于复用与审计）。
CREATE TABLE IF NOT EXISTS ai_interpretations (
    id UUID PRIMARY KEY,
    org_id UUID REFERENCES organizations (id) ON DELETE SET NULL,
    created_by UUID REFERENCES users (id) ON DELETE SET NULL,
    job_id UUID REFERENCES computation_jobs (id) ON DELETE SET NULL,
    scenario_id UUID REFERENCES scenarios (id) ON DELETE SET NULL,
    preset TEXT NOT NULL,
    model TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ai_interp_job ON ai_interpretations (job_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_interp_org ON ai_interpretations (org_id, created_at DESC);

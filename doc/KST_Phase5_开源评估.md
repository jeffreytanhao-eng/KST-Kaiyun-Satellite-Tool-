# KST Phase 5 开源项目与方案评估

**版本**：v1.0  
**日期**：2026-11-26  
**依据**：`doc/Openspace_Satellite_Tool_Phase5_开发计划书_v1.0.md` 第 4 节（强制评估）

本文档记录 Phase 5 推荐方案与开源项目的评估结论、采用方式与不采用理由。结论与 `backend/src/ai.rs`、`backend/src/policy.rs`、`backend/src/routes/{ai,jobs,orgs,scenarios}.rs`、`backend/src/db.rs`、`backend/src/rate_limit.rs`、`backend/migrations/0004_phase5.sql`、`frontend/src/components/{TaskPanel,CollabPanel,OrgPanel,AiPanel}.vue` 的实现保持一致。

---

## 1. 评估结论总览

| 类别 | 项目 / 方案 | 结论 | 采用方式 |
|------|-------------|------|----------|
| AI | 现有 `AiClient` 扩展 | 采用 | 在既有 OpenAI 兼容适配器上增加 JSON 模式与结构化建议，不引入厂商 SDK |
| 任务队列 | 现有 `computation_jobs` 表扩展 | 采用 | 增加进度、缓存键、取消标记与 `cancelled` 状态，不引入独立消息队列 |
| 权限 | 基于现有 JWT + 组织模型扩展 | 采用 | JWT 增加角色声明并在每次请求回查数据库；新增 `scenario_shares` 显式共享 |
| 缓存 | Redis 或数据库缓存 | 数据库缓存采用，Redis 不采用 | 以 `cache_key` + 有效期 + 同组织约束在 PostgreSQL 内复用结果；Redis 列为 P2 |
| 协作 | 简单评论表或版本字段 | 评论表采用，版本字段不采用 | 新增 `scenario_comments`；修改留痕由 `audit_logs` 承担，版本回滚列为 P2 |
| 限流 | 现有 `rate_limit.rs` 扩展 | 采用 | 保留固定窗口接口限流，新增按用户/组织的业务配额（并发、每日任务、每日 AI） |

---

## 2. 逐项评估

### 2.1 现有 `AiClient` 扩展（采用）

- **用途**：统一 AI 调用，支撑“参数建议”和“结果解读”。
- **评估**：
  - 项目已有 `AiClient`，直接对接 OpenAI Chat Completions 协议，兼容火山方舟、DeepSeek、通义等主流服务，无锁定风险；
  - 引入 `async-openai`、`llm` 等第三方 SDK 会额外增加依赖与版本绑定，而当前只需 HTTP + JSON；
  - Phase 5 新增能力集中在两点：`response_format: json_object`（结构化建议）与预设扩展（链路、雷达解读）。
- **结论**：**采用扩展，不引入 SDK**。
- **落地**：
  - `AiClient::complete` 保持纯文本；新增 `complete_json` 携带 `response_format: {"type":"json_object"}`；
  - 预设从 4 个扩展到 6 个（新增 `link_interpret`、`radar_interpret`），覆盖 Access、覆盖、环境、链路、雷达与轨道参数推荐；
  - `recommend` 解析 JSON（容忍 ```json 围栏与前后说明文字），失败时降级为纯文本返回，避免模型不遵循格式导致功能不可用；
  - 结构建议 schema：`summary`、`orbit{altitude_km,inclination_deg,eccentricity}`、`sensor{shape,half_angle_deg,max_range_km,pointing_mode}`、`constellation{constellation_type,planes,total_satellites,phasing}`、`risks[]`；
  - 所有 AI 请求经后端转发，调用记录写入 `ai_requests`，前端不持有 API Key；
  - `KST_AI_ENABLED=false` 或缺少 Key/Model 时，`/ai/*` 统一返回明确错误，核心计算与页面不受影响。

### 2.2 现有 `computation_jobs` 表扩展（采用）

- **用途**：任务列表可视化、进度反馈、取消、结果缓存与保留清理。
- **评估**：
  - 项目已用 `computation_jobs` + `spawn_blocking` 承载全部异步计算，状态机简单（queued → running → succeeded/failed）；
  - 引入 Celery、BullMQ、RabbitMQ 等需要额外中间件与运维成本，且任务产物仍需落 PostgreSQL；
  - Phase 5 所需能力（进度、取消、缓存）均可通过表字段与状态机扩展完成。
- **结论**：**采用表扩展，不引入消息队列**。
- **落地**（`0004_phase5.sql`）：
  - 新增 `progress`（0/10/25/85/100）、`cached`、`cancel_requested`、`cancelled_at`、`cache_key`、`cache_source_id`；
  - 状态新增 `cancelled`；`queued` 任务取消立即置为 `cancelled`，`running` 任务置 `cancel_requested`，计算完成后丢弃结果并置 `cancelled`；
  - 新增索引 `idx_jobs_cache (org_id, cache_key, created_at)`、`idx_jobs_user`、`idx_jobs_active`、`idx_jobs_finished`；
  - `db::spawn_job_cleanup` 按 `KST_JOB_RETENTION_HOURS`（默认 168 小时）周期清理终态任务；
  - `GET /jobs` 仅返回任务元数据，不再携带 `params/result` 大字段，避免 3 秒轮询反复传输兆级 JSON（实测修复后轮询开销从秒级降至毫秒级）。

### 2.3 基于现有 JWT + 组织模型扩展（采用）

- **用途**：组织内角色（admin/analyst/viewer）与场景只读/编辑共享。
- **评估**：
  - 项目已有 JWT、`users.org_id` 与组织表，扩展成本最低；
  - 引入 Casbin、OPA 等策略引擎对当前“组织 + 场景 + 角色”二维模型过重，且难以与 SQL 可见性过滤结合；
  - 角色变更需要即时生效，纯 JWT 声明会滞后至 Token 过期。
- **结论**：**采用 JWT 声明 + 每请求数据库回查**。
- **落地**：
  - `Claims` 增加 `role`；`AuthUser` 在 `FromRequestParts` 中按 `sub` 查询最新 `org_id/role`，角色调整无需重新登录；
  - `users.role` 取值 `admin | analyst | viewer`（CHECK 约束），首个用户自动提升为 admin；
  - `policy::scenario_access` 统一计算权限：创建者 → owner；同组织管理员 → admin；显式共享 → read/write；同组织其他成员 → read；不可见统一返回 404，避免泄露资源存在性；
  - 写入校验：创建/更新场景、创建任务、保存 AI 解读要求 `require_writer`（admin/analyst），共享管理与删除要求 owner 或 admin；
  - 所有校验在 Axum 处理函数与 SQL 层执行，前端仅根据 `permission/can_edit` 做展示（隐藏按钮），不作为安全边界。

### 2.4 Redis 或数据库缓存（数据库缓存采用，Redis 不采用）

- **用途**：任务结果缓存与复用。
- **评估**：
  - 计算结果是结构化 JSON（可达数 MB），需要持久化、按组织隔离、可审计，并可被任务列表/CSV/报告直接复用，天然适合与任务行同库；
  - `deploy/docker-compose.yml` 虽已提供 Redis 7，但当前没有会话共享、分布式限流或跨实例广播的硬需求；
  - 若用 Redis 仅存结果，仍需保留数据库任务行与缓存键，形成双写与失效一致性问题。
- **结论**：**本期采用 PostgreSQL 缓存，Redis 不接入**。
- **落地**：
  - `cache_key = sha256(kind | 规范化 params | 规范化场景 data)`，严格绑定任务类型、参数与场景内容，任一变化即不命中；
  - 命中条件：同组织、`status='succeeded'`、`finished_at` 在 `KST_JOB_CACHE_HOURS`（默认 24 小时）内；
  - 命中时新建任务行并复制结果、置 `cached=TRUE` 与 `cache_source_id`，保证归属与审计清晰，不跨用户共享行；
  - **后续**：多实例部署或高频命中场景可将 `cache_key → result` 放入 Redis 并设置 TTL，数据库仍作为事实来源（P2）。

### 2.5 简单评论表或版本字段（评论表采用，版本字段不采用）

- **用途**：多用户协作基础。
- **评估**：
  - 版本字段/快照表需要 diff、回滚与存储策略，属于完整版本管理范畴，成本明显高于 Phase 5 目标；
  - 评论表结构简单、价值直接，配合审计日志即可满足“协作基础”；
  - 版本管理可作为 P2，在现有 `scenarios.updated_at` 与审计记录基础上扩展。
- **结论**：**采用评论表 + 审计留痕，不实现版本回滚**。
- **落地**：
  - `scenario_comments (scenario_id, created_by, body, created_at)`，正文上限 2000 字；
  - 可读用户即可评论（包含 viewer），写入需登录；
  - 共享变更、场景删除、评论、任务取消、角色/配额调整、AI 解读保存均写入 `audit_logs`（`org_id, user_id, action, resource_type, resource_id, details`）；
  - 审计查询仅组织管理员可见（`GET /org/audit`）。

### 2.6 现有 `rate_limit.rs` 扩展（采用）

- **用途**：接口限流与按用户/组织的资源配额。
- **评估**：
  - 现有固定窗口限流按 Authorization 头计数，适合防御异常客户端，但无法表达“并发任务/每日任务/AI 次数”等业务配额；
  - 引入 Redis 令牌桶可支持多实例共享计数，但当前为单实例部署，数据库计数已足够且可审计；
  - 业务配额必须与资源创建在同一事务边界前校验，放在中间件反而难以获取场景/组织上下文。
- **结论**：**保留接口限流，新增数据库业务配额**。
- **落地**：
  - `rate_limit.rs` 继续对 `/api/v1` 计算类路由做 240 次/60 秒固定窗口限制；
  - `policy::effective_limits` 计算用户有效配额：用户覆盖 > 组织配额 > 默认（并发 5、每日 500、AI 每日 200）；
  - `enforce_job_quota` 在创建任务前检查“运行中任务数”和“今日任务数”；`enforce_ai_quota` 在 AI 调用前检查“今日 AI 次数”；
  - 超限统一返回 `429`，`error.code = quota_exceeded`，消息包含具体上限与处理建议（等待完成、取消任务或明日再试）。

---

## 3. P2 可选项评估

| 可选项 | 结论 | 说明 |
|--------|------|------|
| Redis 结果缓存 / 分布式限流 | 暂不实现 | 单实例与数据一致性优先，多实例部署时再引入 |
| 场景版本历史与回滚 | 暂不实现 | 当前由 `updated_at` + 审计日志留痕，完整版本需快照与 diff |
| 实时协作（WebSocket/OT） | 暂不实现 | 评论与共享已满足协作基础，实时协同编辑复杂度高 |
| 消息通知（邮件/飞书） | 暂不实现 | 与 Phase 5 验收无直接关系 |
| AI 调用配额按 Token 计量 | 暂不实现 | 当前按次数计量，Token 计量需解析 usage 并制定计价策略 |
| 组织层级（部门/项目组） | 暂不实现 | 现有单层组织已满足角色与配额控制 |

---

## 4. 架构约束落实情况

- **计算位置**：轨道、覆盖、链路、雷达、干扰、报告全部仍在 Rust 后端；AI 仅生成建议与解读文本，不参与数值计算，也不直接写入场景，必须由用户在前端确认后调用场景保存接口。
- **安全**：
  - 场景共享、可见性、角色、任务可见性、AI 解读读取全部由 `policy.rs` 与 SQL 片段强制校验；
  - `JOB_READ_SCOPE` 固定使用 `$1 = 用户 ID`、`$2 = 组织 ID`，任务列表、任务详情、CSV/报告下载、AI 汇总与解读保存共用同一可见性规则；
  - AI 请求只经后端发出，API Key 不下发前端；`ai_requests` 记录调用类型、用户与耗时，不保存敏感原文。
- **性能**：
  - 任务列表不再返回 `params/result`，避免轮询大字段；
  - 缓存命中直接复用结果，跳过重复计算；
  - 大规模计算仍走异步任务与 `spawn_blocking`，不阻塞请求线程。
- **数据模型**：`0004_phase5.sql` 完成用户角色/配额、组织配额、任务进度/缓存/取消、场景共享、评论、审计与 AI 解读表；迁移已在 PostgreSQL 16 上顺序执行成功（version 1–4 全部 `success = t`）。
- **可关闭性**：
  - `KST_AI_ENABLED=false` 或缺少 `KST_AI_API_KEY/KST_AI_MODEL` 时 AI 接口返回明确错误，前端展示“AI 未启用”，轨道/覆盖/链路/雷达/报告等核心功能不受影响；
  - 协作能力按需使用，不改变场景与任务主流程；共享表为空时行为与 Phase 4 一致。

---

## 5. 验收对照

| 验收标准 | 结论 | 实现位置 |
|----------|------|----------|
| 自然语言获取轨道/传感器/星座参数建议并可应用 | 通过 | `POST /ai/recommend` + `AiPanel.onApplySuggestion` |
| 对主要分析结果生成 AI 解读摘要 | 通过 | `POST /ai/interpret`，覆盖 Access/覆盖/链路/雷达/星座/干扰 |
| 场景只读/编辑共享且权限生效 | 通过 | `scenario_shares` + `policy::scenario_access` |
| 任务列表状态/进度/取消/结果缓存 | 通过 | `GET /jobs`、`POST /jobs/{id}/cancel`、`cache_key` |
| 组织内角色权限控制 | 通过 | `users.role` + `require_writer/require_admin` |
| 资源配额与限流生效且有明确提示 | 通过 | `policy::enforce_*` + `rate_limit.rs` + 429 `quota_exceeded` |
| AI 可完全关闭且不影响核心计算 | 通过 | `KST_AI_ENABLED` + `guard_ai` |
| 所有权限与共享校验在后端执行 | 通过 | `policy.rs`、`routes/*.rs`、SQL 可见性过滤 |
| 推荐方案有评估记录 | 通过 | 本文档 |
| 视觉与核心功能无回退（Phase 1–4） | 通过 | 前端全量构建 + 浏览器回归 |

---

## 6. 验证记录

### 6.1 迁移与启动

- `0004_phase5.sql` 随服务启动自动执行，`_sqlx_migrations` 中 version 1–4 全部 `success = t`。
- PostgreSQL 中已确认 `users.role/配额列`、`organizations` 配额列、`computation_jobs.progress/cache_key/cached/cancel_requested`、`scenario_shares`、`scenario_comments`、`audit_logs`、`ai_interpretations` 存在。
- `GET /api/v1/health` 返回 `{"service":"kst-server","status":"ok","version":"0.1.0"}`。

### 6.2 接口验收（44/44 通过）

使用临时账号 `p5tester`（analyst）与管理员 `admin` 执行端到端脚本，覆盖：

- 登录与 `/auth/me` 返回 `role`；组织用量、成员列表、审计日志；
- 场景复制创建、同组织默认只读、只读更新返回 403、写共享后可编辑、评论创建与读取；
- 任务创建、进度、`cached=true` 命中、`GET /jobs` 不返回 `result`、详情返回完整结果；
- 每日配额设为 1 后创建任务返回 `429 quota_exceeded`，重置后恢复；
- 取消任务返回 `cancel_requested=true`，计算完成后终态为 `cancelled`；
- viewer 角色创建任务/更新场景返回 403，评论可用；
- AI 配置、结果解读、参数建议（含 `suggestion.orbit` 等结构化参数）、保存与读取解读记录；
- 脚本执行结束后清理测试场景、任务、评论、解读与临时账号。

验收过程中发现并修复：

1. `policy::effective_limits` 将 `INTEGER` 配额列读入 `i64` 导致 500，改为 SQL 侧 `::bigint`；
2. `GET /jobs` 返回 `params/result` 导致轮询反复传输大 JSON（单个传播结果可达数 MB），改为仅返回任务摘要字段，列表轮询恢复毫秒级；
3. 推理模型只返回 `reasoning_content` 时被判定“响应为空”，增加兜底并提高 `max_tokens`；
4. 历史任务 `progress` 默认 0，列表对 `succeeded` 统一显示 100%。

### 6.3 前端构建与浏览器回归

- `npm run typecheck` 通过（`NODE_OPTIONS=--max-old-space-size=4096`）。
- `npm run build` 通过（`vue-tsc --noEmit && vite build`，产物输出到 `frontend/dist`）。
- 浏览器实测：登录后右侧面板出现“任务 / 协作 / 组织”三个新页签；
  - 任务页签显示任务列表、状态、进度、耗时与 CSV/报告操作；
  - 组织页签显示配额（并发 0/5、今日任务 89/500、今日 AI 13/200）、成员角色、组织默认配额与审计日志；
  - 协作页签显示共享状态与评论区，未共享时提示明确；
  - 原有属性、Access、覆盖、传播、链路、星座、雷达、报告、环境、AI 页签均正常，页面无报错、无崩溃。

### 6.4 AI 完全关闭验证

- 以 `KST_AI_ENABLED=false` 启动：日志输出“AI 功能未启用（缺少 KST_AI_API_KEY / KST_AI_MODEL 或显式关闭）”；
- `GET /api/v1/ai/config` 返回 `enabled=false`；
- `POST /api/v1/ai/recommend` 被直接拒绝（400），不会发起外部模型请求；
- 同一进程下场景、任务、覆盖/链路等核心接口正常，恢复 `KST_AI_ENABLED=true` 后 `enabled=true`。

# KST（Kaiyun Satellite Tool）开发设计书（Web 服务器版）

**版本**: v2.0  
**日期**: 2026-11-25  
**定位**: 基于 Web 的服务器端空间任务分析与仿真平台，对标 STK 核心能力，并引入云端 AI 增强功能  
**交付对象**: TRAE 直接开发，Grok 作为 Supervisor 进行架构把控与关键节点评审  

**参考仓库映射**（必须按此引用）：

| 仓库 | 用途 | 关键路径 |
|------|------|----------|
| `spacelibres/STKCraft-time` | 时间基座 | 直接依赖 |
| `spacelibres/STKCraft-Geocalc` | 几何 / SPK / 帧 / Access 原子能力 | 直接依赖 |
| `spacelibres/STKCraft-SpaceEnv` | 环境模型 | 按需调用 |
| `jeffreytanhao-eng/Openspace-Safety-Platform-II` | **UI 风格 + 三维态势核心参考** | `web/src/views/situation/` |
| `jeffreytanhao-eng/SSA-Watch-Planning` | 前端工作台、Cesium 仿真交互参考 | `frontend/components/`、`public/cesium/` |
| `jeffreytanhao-eng/openspace-os-core` | 可选平台能力（认证、事件总线等）参考 | 按需借鉴，不强制依赖 |

---

## 1. 产品定位与目标

### 1.1 定位
KST 是一款**浏览器访问的服务器端空间分析平台**。用户无需安装客户端，所有重计算在服务器完成，前端负责交互与三维可视化。

与传统 STK 的核心差异：
- 无需本地安装
- 天然支持多用户与权限
- 可结合云端 AI 提供智能辅助
- 数据集中管理与共享

### 1.2 MVP 能力目标
- 场景管理（创建、保存、共享）
- SGP4 传播
- 卫星 + 地面站 Access 计算
- 三维地球 + 轨道 + 时间回放
- 基础报告
- 用户登录与基础权限

### 1.3 明确不在 Phase 1 实现
- HPOP / 高精度数值积分
- 大规模星座覆盖网格
- 实时多人协同编辑（后期可加）

---

## 2. 总体架构

```
浏览器（Vue 3.5 + Element Plus + Cesium）
        │ HTTPS / WebSocket
        ▼
API 网关 / Nginx
        │
        ▼
后端服务（Rust + Axum）
  ├── 认证与权限
  ├── 场景与对象管理
  ├── 异步计算任务调度
  ├── Access / 传播引擎（调用 stkcraft-*）
  └── AI 服务适配层
        │
        ▼
PostgreSQL + Redis（任务队列 / 缓存）
        │
        ▼
对象存储（星历文件、报告、内核等，可选 MinIO / S3）
```

**硬性约束**：
- 所有轨道计算、帧转换、Access 搜索必须在 Rust 后端完成。
- 前端只负责展示与交互，禁止实现核心数值计算。

---

## 3. 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 前端 | Vue 3.5 + TypeScript + Vite 6 + Element Plus + Pinia | 风格参考 Openspace-Safety-Platform-II |
| 三维 | CesiumJS + Three.js 叠加层 | 必须参考 `situation/` 实现 |
| 后端 | Rust + Axum + Tokio | 高性能，直接调用 stkcraft crates |
| 数据库 | **PostgreSQL 16** | 主存储 |
| 缓存/队列 | Redis 7 | 任务队列、计算结果缓存、会话 |
| 认证 | JWT + 可选 OAuth | 参考 openspace-os-core 思路 |
| AI 能力 | 云端 LLM API（兼容 OpenAI 协议） | 通过适配层调用 |
| 部署 | Docker Compose 起步 | 后期可上 K8s |

---

## 4. 数据库设计（PostgreSQL）

### 4.1 核心表（Phase 1 必须）

```sql
-- 用户与组织
CREATE TABLE users (
  id UUID PRIMARY KEY,
  username TEXT UNIQUE NOT NULL,
  email TEXT UNIQUE,
  password_hash TEXT,
  created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE organizations (
  id UUID PRIMARY KEY,
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ DEFAULT now()
);

-- 场景
CREATE TABLE scenarios (
  id UUID PRIMARY KEY,
  org_id UUID REFERENCES organizations(id),
  owner_id UUID REFERENCES users(id),
  name TEXT NOT NULL,
  description TEXT,
  start_epoch TIMESTAMPTZ,
  stop_epoch TIMESTAMPTZ,
  config_json JSONB,          -- 对象列表、参数等
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);

-- 计算任务
CREATE TABLE computation_jobs (
  id UUID PRIMARY KEY,
  scenario_id UUID REFERENCES scenarios(id),
  job_type TEXT NOT NULL,     -- propagate / access / coverage
  status TEXT NOT NULL,       -- pending / running / done / failed
  params_json JSONB,
  result_json JSONB,
  progress REAL DEFAULT 0,
  created_at TIMESTAMPTZ DEFAULT now(),
  finished_at TIMESTAMPTZ
);

-- 报告
CREATE TABLE reports (
  id UUID PRIMARY KEY,
  job_id UUID REFERENCES computation_jobs(id),
  name TEXT,
  content_json JSONB,
  created_at TIMESTAMPTZ DEFAULT now()
);
```

### 4.2 设计原则
- 场景主体用 `config_json`（JSONB）存储对象与参数，同时支持导出为标准 JSON 文件。
- 大结果（长时间传播点列）可存对象存储，数据库只存引用与摘要。
- 所有表预留 `org_id` 支持多租户。

---

## 5. 数值核心层

与桌面版一致，直接依赖：
- `stkcraft-time`
- `stkcraft-geocalc`
- `stkcraft-spaceenv`（按需）

新增模块（Rust）：
- `Propagator` trait + `Sgp4Propagator`
- `AccessEngine`（内部调用 geocalc）

所有计算通过异步任务执行，不阻塞 API 请求。

---

## 6. API 设计要点

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/login` | 登录 |
| GET/POST | `/api/v1/scenarios` | 场景列表 / 创建 |
| POST | `/api/v1/jobs/propagate` | 提交传播任务 |
| POST | `/api/v1/jobs/access` | 提交 Access 任务 |
| GET | `/api/v1/jobs/{id}` | 查询任务状态与结果 |
| WS | `/api/v1/ws/jobs/{id}` | 进度推送（可选） |
| POST | `/api/v1/ai/suggest` | AI 辅助接口 |

---

## 7. 前端设计

### 7.1 风格与布局
必须参考 `Openspace-Safety-Platform-II`：
- 深色航天风格
- 左侧对象树
- 中部三维视口（`web/src/views/situation/`）
- 底部时间线
- 右侧属性 / 报告面板

### 7.2 三维实现要求
- Cesium 初始化、离线底图、相机控制优先复用 Safety Platform 的 `situation` 模块思路。
- 轨道与对象由后端返回的状态序列驱动。
- 禁止前端使用 `satellite.js` 做主传播计算。

---

## 8. 云端 AI 增强功能（建议纳入设计）

以下功能可在 Phase 1 预留接口，Phase 2 逐步实现：

| 功能 | 说明 | 实现方式 |
|------|------|----------|
| 自然语言创建场景 | “创建一个 500km SSO 卫星，北京站 Access” | LLM 解析 → 生成场景 JSON |
| 智能参数推荐 | 根据任务目标推荐轨道高度、倾角、传感器参数 | LLM + 规则引擎 |
| Access 结果解读 | 自动生成中文分析摘要 | LLM 总结计算报告 |
| 异常检测提示 | 对传播结果或 Access 窗口做简单异常标注 | 规则 + LLM 解释 |
| 报告自动生成 | 一键生成带图表说明的分析报告 | LLM + 模板 |
| 知识问答 | “这个场景的最大 Access 时长是多少？” | RAG 或直接查询结果后 LLM 回答 |

**实现建议**：
- 后端增加 `AIAdapter`，统一调用云端 LLM（兼容 OpenAI 协议）。
- 敏感计算数据不直接送 LLM，只送摘要或用户明确授权的内容。
- AI 功能全部可选，关闭后不影响核心计算。

---

## 9. 开发阶段与 stkcraft 能力映射

### 9.1 stkcraft 能力总览与阶段归属

| 能力 | 所属仓库 | Phase 1 | Phase 2 | 说明 |
|------|----------|---------|---------|------|
| 高保真时间系统（UTC/TAI/TT/TDB/ET/GPS/UT1/SCLK） | STKCraft-time | ✅ 必须 | - | 全阶段基础 |
| SPK 状态计算、参考系转换、Aberration | STKCraft-Geocalc | ✅ 必须 | - | Access 与可视化基础 |
| Geometry Finder 基础用法 | STKCraft-Geocalc | ✅ 必须 | 深化 | Phase 1 用于简单 Access |
| 多约束 / 复杂 Geometry Finder | STKCraft-Geocalc | - | ✅ | 传感器、遮挡、复杂窗口 |
| 大气 / 地磁 / 辐射环境模型 | STKCraft-SpaceEnv | 预留接口 | ✅ 接入 | Phase 1 不强制调用 |
| SGP4 传播器 | 自研（基于 sgp4 crate） | ✅ 必须 | - | Phase 1 唯一传播器 |
| HPOP / 数值积分 | 自研或外部 | - | 研究/预留 | 不属于 stkcraft 现有能力 |

### 9.2 Phase 0：基础打通（1–2 周）

**目标**：前后端跑通，数值核心可调用，三维可显示。

1. 前后端分离项目骨架（Vue 3.5 + Rust Axum）
2. PostgreSQL 连接与用户登录
3. 接入 `stkcraft-time` 与 `stkcraft-geocalc`
4. 最小 Cesium 视口（参考 Openspace-Safety-Platform-II `situation/`）
5. 实现单时刻状态计算并在三维中显示

**验收**：能登录、能显示地球、能显示一个由后端计算的天体位置。

### 9.3 Phase 1：MVP（4–6 周）

**目标**：可用的基础轨道分析与 Access 工具。

**必须实现**：
- 场景 CRUD（PostgreSQL + JSONB，支持导入/导出 JSON）
- 用户与基础权限
- `Sgp4Propagator` + 异步计算任务
- `AccessEngine`（调用 geocalc，支持卫星 + 地面站、最小仰角约束）
- 对象树、属性面板、时间线、轨道回放
- 基础 Access 报告（表格 + 简单图表）
- AI 接口预留，并实现「自然语言生成简单场景」原型

**stkcraft 使用范围**：
- 完整使用 `stkcraft-time`
- 使用 `stkcraft-geocalc` 的状态计算与基础 Geometry Finder
- `stkcraft-spaceenv` 仅预留调用接口，不纳入主流程

**明确不实现**：
- HPOP
- 复杂传感器模型与 FOV 约束 Access
- 覆盖网格
- 环境模型实际参与传播

**验收**：用户可创建场景、添加卫星与地面站、完成传播与 Access 计算、在三维中回放并查看报告。

### 9.4 Phase 2：增强（6–8 周）

**目标**：提升分析深度，并落地 AI 辅助能力。

**计划实现**：
- 多对象支持与基础传感器模型
- 基于 Geometry Finder 的多约束 Access（FOV、遮挡等）
- 接入 `stkcraft-spaceenv`（大气阻力等基础扰动展示）
- 覆盖分析基础能力（网格 + 统计）
- AI 功能落地：结果解读、参数推荐、报告自动生成
- 性能优化、权限细化、任务队列完善

**仍不承诺**：
- 完整 HPOP 力模型体系
- 姿态控制仿真
- 链路预算与雷达/光学详细模型

### 9.5 阶段原则

- Phase 1 优先保证「能用且数值可信」，把 stkcraft 已有的时间与几何能力用扎实。
- Phase 2 再扩展约束复杂度与环境模型，同时把 AI 从原型变成可用功能。
- 任何超出上述范围的功能，需 Supervisor 评审后再纳入。

---

## 10. 部署

Phase 1 使用 Docker Compose：
- `frontend`（Nginx 托管静态资源）
- `backend`（Rust）
- `postgres`
- `redis`
- 可选 `minio`

---

## 11. Supervisor 介入点

- Phase 0 完成后：架构与 Cesium 集成评审
- AccessEngine 与异步任务接口设计完成后：数值与调用链评审
- Phase 1 MVP 完成后：功能完整性与 stkcraft 使用正确性评审
- Phase 2 启动前：范围确认

---

## 12. 禁止事项

- 禁止前端实现核心轨道计算
- 禁止脱离指定参考路径自由设计三维引擎
- 禁止在未评审的情况下引入重量级新框架
- 禁止在 Phase 1 强行实现 HPOP 或复杂覆盖

---

**文档结束**

本版本已明确 Phase 1 / Phase 2 边界，并给出 stkcraft 能力的阶段归属。可直接交给 TRAE 作为开发依据。


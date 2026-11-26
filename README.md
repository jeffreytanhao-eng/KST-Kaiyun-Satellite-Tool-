# KST · Kaiyun Satellite Tool

基于 Web 的服务器端空间任务分析与仿真平台，对标 STK 核心能力。用户无需安装客户端，所有轨道计算在服务器完成，浏览器负责交互与三维可视化。

**当前阶段：Phase 5（协作与 AI 辅助）已完成**。Phase 1–4 的场景建模、Access/覆盖分析、星座/链路/雷达/报告与 AI 基础能力均已就绪。

Phase 5 新增：

- 场景只读/编辑共享（按用户共享，后端强制校验）与评论、审计日志；
- 组织角色（admin / analyst / viewer）、按用户与组织的并发/每日/AI 配额，超限返回 429；
- 任务列表（状态/进度/取消/缓存命中）与结果保留清理；
- AI 参数建议（轨道/传感器/星座，可一键应用）与结果解读（可保存并关联任务）；
- AI 可通过 `KST_AI_ENABLED=false` 完全关闭，核心计算不受影响。

## 核心特性

- **浏览器访问**：Vue 3 + CesiumJS 三维地球，离线网格底图，无需 Cesium ion Token。
- **后端计算**：Rust + Axum 承担时间系统、轨道递推、参考系转换等全部数值计算，前端只做展示。
- **数值内核**：直接复用 [stkcraft-time](https://crates.io/crates/stkcraft-time) 与 [stkcraft-geocalc](https://crates.io/crates/stkcraft-geocalc)，支持 UTC/TAI/TT/TDB/ET 等高精度时间系统与 J2000/IAU_EARTH 参考系转换。
- **无外部内核依赖**：地球半径、引力常数与 IAU_EARTH 定向参数以内联 PCK 文本提供，开箱即用。
- **多用户基础**：JWT 登录与用户/组织数据模型。

## 技术栈

| 层级 | 技术 |
| --- | --- |
| 前端 | Vue 3.5 + TypeScript + Vite 6 + Element Plus + Pinia + CesiumJS |
| 后端 | Rust + Axum + Tokio + SQLx |
| 数据库 | PostgreSQL 16 |
| 数值内核 | stkcraft-time、stkcraft-geocalc |
| 部署 | Docker Compose |

## 目录结构

```
KST/
├── backend/          # Rust 后端（Axum + SQLx）
│   ├── src/          # 路由、认证、空间计算上下文
│   ├── migrations/   # PostgreSQL 迁移脚本
│   └── .cargo/       # cargo 配置（rsproxy 镜像）
├── frontend/         # Vue 3 前端
│   └── src/          # 登录页、三维态势页、API 封装、状态管理
├── deploy/           # Docker Compose（PostgreSQL 等）
├── doc/              # 开发设计文档
└── .tools/           # 项目内构建工具链（MinGW，已 gitignore）
```

## 快速开始

### 前置条件

- Rust（版本由 `backend/rust-toolchain.toml` 固定为 `stable-x86_64-pc-windows-gnu`，项目内 `.tools/winlibs` 提供 MinGW 链接器）
- Node.js 18+
- Docker（用于本地 PostgreSQL）

### 1. 启动数据库

```bash
docker compose -f deploy/docker-compose.yml up -d
```

### 2. 启动后端

```bash
cd backend
cp .env.example .env   # 按需修改数据库与 JWT 配置
cargo run
```

后端默认监听 `http://127.0.0.1:18080`，首次启动会自动执行迁移并创建开发用管理员账号。

### 3. 启动前端

```bash
cd frontend
npm install
npm run dev
```

浏览器访问 `http://localhost:5173`，开发环境默认账号：`admin / admin123`（生产环境请务必修改）。

## API 概览

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/v1/health` | 服务健康检查 |
| POST | `/api/v1/auth/login` | 用户登录，返回 JWT |
| GET | `/api/v1/auth/me` | 获取当前用户信息 |
| POST | `/api/v1/compute/state` | 单时刻轨道状态计算（ECI/ECEF/经纬高 + 轨道轨迹） |
| GET/POST | `/api/v1/scenarios/{id}/shares` | 场景共享（只读/可编辑，仅创建者或管理员） |
| GET/POST | `/api/v1/scenarios/{id}/comments` | 场景评论 |
| GET/POST | `/api/v1/jobs` | 任务列表（状态/进度/缓存）与创建任务 |
| POST | `/api/v1/jobs/{id}/cancel` | 取消任务 |
| GET | `/api/v1/org/usage` | 组织配额用量 |
| POST | `/api/v1/ai/recommend` | AI 参数建议（轨道/传感器/星座） |
| POST | `/api/v1/ai/interpret` | AI 结果解读 |

`POST /api/v1/compute/state` 请求示例：

```json
{
  "epoch": "2026-11-25T12:00:00Z",
  "orbit": {
    "semi_major_axis_km": 6878.137,
    "eccentricity": 0.001,
    "inclination_deg": 97.6,
    "raan_deg": 30.0,
    "arg_perigee_deg": 0.0,
    "true_anomaly_deg": 0.0
  },
  "samples": 180
}
```

## 架构约定

- 所有轨道计算、帧转换与 Access 搜索只在 Rust 后端实现，禁止前端进行核心数值计算。
- 外部依赖下载优先使用国内镜像：cargo 走 rsproxy（见 `backend/.cargo/config.toml`），npm 走 npmmirror（见 `frontend/.npmrc`）。
- 构建产物统一输出到 `backend/target`，不写入用户目录。

## Phase 0 验收情况

- [x] 前后端分离骨架（Vue 3.5 + Rust Axum）
- [x] PostgreSQL 连接与用户登录
- [x] 接入 stkcraft-time 与 stkcraft-geocalc
- [x] 最小 Cesium 三维视口
- [x] 单时刻状态计算并在三维中显示

## Roadmap

- **Phase 1（MVP）**：场景 CRUD（JSONB 导入导出）、SGP4 传播、卫星-地面站 Access 引擎、异步计算任务、对象树/属性面板/时间线与轨道回放、基础报告。
- **Phase 2（增强）**：传感器模型与多约束 Access、spaceenv 环境模型、覆盖分析、AI 辅助（自然语言建场景、结果解读、报告生成）。

详细设计见 [开发设计书](doc/KST_Kaiyun_Satellite_Tool_开发设计书_v2.0_Web服务器版.md)。

## 许可证

本项目基于 [Apache License 2.0](LICENSE) 发布。

# KST Phase 3 开源项目与数据源评估

**版本**：v1.0  
**日期**：2026-11-25  
**依据**：`doc/KST_Phase3_开发计划书_v1.0.md` 第 4 节（强制评估）

本文档记录 Phase 3 推荐开源项目与数据源的评估结论、采用方式与不采用理由。所有结论与 `backend/src/numerical.rs`、`backend/src/attitude.rs`、`backend/src/routes/spacetrack.rs`、`frontend/src/composables/useCesiumViewer.ts` 中的实现保持一致。

---

## 1. 评估结论总览

| 类别 | 项目 / 来源 | 结论 | 采用方式 |
|------|-------------|------|----------|
| 传播器 | ssmichael1/satkit | 参考 | 参考其自适应积分器与力模型组织方式，自研 DOPRI5 实现 |
| 传播器 | duncaneddy/brahe | 参考 | 参考其摄动力组合与状态向量约定，不引入依赖 |
| 传播器 | neuromorphicsystems/sgp4 | 采用 | 沿用现有 `sgp4` crate（即该实现）作为 SGP4 基准 |
| 传播器 | esa/dSGP4 | 不采用 | 可微分 SGP4 当前无需求，留待 AI 梯度场景 |
| 几何/星历 | OpenSpace/Spice | 不采用 | 需分发 SPICE 内核，当前精度需求不必要 |
| 几何/星历 | arturania/cspice | 不采用 | C 库 FFI 与跨平台分发成本高，暂缓 |
| 可视化 | kaktus40/cesium-sensors | 参考 | 参考视场锥/矩形构建思路，基于 Cesium Entity 原生实现 |
| 可视化 | Flowm/satvis | 参考 | 仅参考深色场景与交互风格 |
| 数据源 | Space-Track | 采用 | 后端 REST 接入，支持按 NORAD 编号批量导入 TLE |

---

## 2. 逐项评估

### 2.1 ssmichael1/satkit（参考）

- **用途**：数值积分器与力模型结构参考。
- **评估**：satkit 的积分器抽象、状态向量与力模型累加方式清晰，适合作为设计参考。
- **结论**：**参考不引入**。原因：
  1. 项目已有 `sgp4` crate 提供初始状态，Phase 3 仅需中心引力 + J2 + 阻力的中等精度积分；
  2. 引入完整 satkit 会带来额外依赖与版本耦合，且其 Python/Rust 双栈组织与当前纯 Rust 后端不完全匹配；
  3. DOPRI5（5(4) 阶嵌入式对）实现规模可控，便于按项目需求定制步长上下限与误差控制。
- **落地**：`backend/src/numerical.rs` 实现自适应 DOPRI5，包含位置/速度独立容差、接受/拒绝步统计与最大步数保护。

### 2.2 duncaneddy/brahe（参考）

- **用途**：现代轨道力学库与力模型组织参考。
- **评估**：brahe 提供完整的轨道、坐标与摄动工具链，工程完整度高。
- **结论**：**参考不引入**。原因：
  1. brahe 体量较大，编译时间与二进制体积开销明显；
  2. 当前后端已使用 `stkcraft-time` / `stkcraft-geocalc` 完成时间与坐标转换，重复能力会造成职责重叠；
  3. Phase 3 的力模型范围明确（中心引力、J2、NRLMSISE-00 阻力），无需完整库。
- **落地**：参考其“加速度按项累加、积分器与力模型解耦”的结构，在 `numerical.rs` 中以 `acceleration()` 聚合各项加速度。

### 2.3 neuromorphicsystems/sgp4（采用）

- **用途**：SGP4 实现升级评估。
- **评估**：项目现有 `sgp4` crate（v0.2）即为该开源实现的 Rust 封装，已稳定用于 Phase 1/2 的传播与 Access。
- **结论**：**继续采用**。不额外替换或重写，避免破坏既有行为。
- **落地**：`backend/src/orbit.rs` 中 SGP4 路径保持不变；数值积分以 SGP4 在起始时刻的状态作为初值（`numerical::propagate`）。

### 2.4 esa/dSGP4（不采用）

- **用途**：可微分 SGP4，为 AI 结合预留。
- **评估**：dSGP4 面向梯度反传与参数学习，需要 PyTorch/JAX 等运行时。
- **结论**：**不采用**。Phase 3 的 AI 能力仍为文本解读/建议，不涉及轨道参数梯度优化；引入会显著增加部署复杂度。
- **后续**：若开展轨道参数辨识或强化学习任务，可单独评估。

### 2.5 OpenSpace/Spice（不采用）

- **用途**：SPICE 内核组织参考。
- **评估**：SPICE 适合高精度行星星历、天体定向与复杂参考系。
- **结论**：**不采用**。Phase 3 对日姿态仅需太阳方向，采用低精度解析太阳星历（误差约 0.01°）即可满足要求；引入 SPICE 需随应用分发内核文件并处理内核加载生命周期。
- **落地**：`backend/src/attitude.rs::sun_direction_ecef` 实现低精度太阳方向。

### 2.6 arturania/cspice（不采用）

- **用途**：CSPICE 跨平台接入评估。
- **评估**：cspice 为 C 库绑定，需要处理动态库分发、Windows 构建与 FFI 安全。
- **结论**：**不采用**，理由同 2.5；当前无高精度星历/参考系需求。
- **后续**：若需要高精度行星星历或深空任务，再评估 `cspice` 或纯 Rust 替代。

### 2.7 kaktus40/cesium-sensors（参考）

- **用途**：传感器视场锥可视化参考。
- **评估**：该项目演示了基于 Cesium 的传感器视场构建方式。
- **结论**：**参考不直接引入**。原因：
  1. 其代码为独立示例，样式与当前深色主题需要大量改造；
  2. 现有 Phase 2 已基于 Cesium Entity 实现圆锥/矩形视场，Phase 3 仅需让视场随真实姿态旋转；
  3. 直接复用会引入额外的 Three.js/Cesium 版本约束。
- **落地**：`frontend/src/composables/useCesiumViewer.ts` 在既有视场实体上按后端姿态轴旋转，视觉风格与既有场景一致。

### 2.8 Flowm/satvis（参考）

- **用途**：轨道与交互风格参考。
- **评估**：satvis 的深色地球、轨道高亮与时间线交互具有参考价值。
- **结论**：**仅风格参考**，不引入代码。现有深色 Cesium 场景、时间线回放与轨道配色已满足 Phase 1/2 视觉标准，Phase 3 新增元素继续沿用 `--kst-*` 变量与既有配色。

### 2.9 Space-Track（采用）

- **用途**：官方编目 TLE/OMM 数据源。
- **评估**：Space-Track 提供稳定的 REST 查询接口，需要注册账号并遵守速率限制。
- **结论**：**采用**。
- **落地**：
  - `backend/src/routes/spacetrack.rs` 提供 `POST /api/spacetrack/tle`；
  - 通过 `KST_SPACETRACK_USERNAME` / `KST_SPACETRACK_PASSWORD` 配置账号；
  - 后端以表单登录 `/ajaxauth/login` 并手动维护会话 Cookie，再查询 `basicspacedata/query/class/gp`；
  - 单次最多 50 个 NORAD 编号，结果以 JSON 返回，前端勾选后写入场景并可直接用于 SGP4 传播；
  - 凭据缺失时返回明确的参数错误，不在前端暴露账号信息。

---

## 3. P2 可选项评估

| 可选项 | 结论 | 说明 |
|--------|------|------|
| 第三体引力 | 暂不实现 | 对 LEO/MEO 短期预报影响小于 J2 与阻力；`numerical.rs::acceleration` 已按项解耦，可直接追加 |
| 太阳光压 | 暂不实现 | 需要面质比与光照阴影模型，当前质量/面积参数已具备，后续可扩展 |
| 简单碰撞筛选 | 暂不实现 | 需要编目级两两筛查与时间窗匹配，Phase 3 优先级为 P2 |
| SPICE / 星历扩展 | 评估后不采用 | 见 2.5 / 2.6 |

---

## 4. 架构约束落实情况

- **计算位置**：数值积分、姿态、链路预算全部在 Rust 后端完成；前端仅做参数输入与可视化。
- **异步任务**：`propagation`、`propagation_compare`、`link_budget` 均通过 `computation_jobs` 异步任务执行，使用 `spawn_blocking` 避免阻塞请求线程。
- **计算保护**：数值积分设置最大步数、最小/最大步长与采样点上限；任务参数校验限制时间窗口与步长范围。
- **数据模型**：卫星新增 `propagator_type`、`attitude_mode`、质量/面积/Cd/容差/J2/阻力开关；传感器新增 `body_boresight` 与 `body` 指向模式。
- **视觉一致性**：新增姿态轴、链路弧段与对比轨迹均使用既有深色配色与发光/虚线材质。

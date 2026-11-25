# KST Phase 1 范围与优先级调整（v2.3）

**版本**: v2.3  
**日期**: 2026-11-25  
**调整原因**: 避免 Phase 1 结束后仍停留在“单仿真界面”水平，提升分析工具属性；并补充 Phase 1 可用的开源项目参考。  
**依据**: Supervisor 评审 + 用户反馈 + GitHub 开源项目检索

---

## 1. 调整原则

1. Phase 1 结束时，用户应能完成**多对象基础分析**，而不只是单星回放。
2. 报告与结果导出的优先级高于单纯视觉炫技。
3. 视觉标准仍然严格（真实地球影像、参考 Safety Platform），但不再是唯一重点。
4. 复杂功能（覆盖网格、HPOP、详细传感器）继续留在 Phase 2。
5. Phase 1 优先复用成熟开源库，减少自研风险。

---

## 2. Phase 1 新范围定义

### 2.1 核心目标
交付一个**可进行基础多对象轨道与 Access 分析**的 Web 工具，具备可用的对象管理、计算、回放与报告能力。

### 2.2 功能优先级

| 优先级 | 功能 | 说明 | 是否必须 |
|--------|------|------|----------|
| **P0** | 真实地球影像 + 基础三维 | 禁止网格底图，参考 Safety Platform | 必须 |
| **P0** | 场景管理（CRUD + JSON 导入导出） | PostgreSQL + JSONB | 必须 |
| **P0** | SGP4 传播（异步任务） | 支持 TLE，优先使用成熟 Rust crate | 必须 |
| **P0** | 多对象支持 | 至少支持多颗卫星 + 多个地面站 | 必须 |
| **P0** | 基础 Access 计算 | 卫星-地面站，最小仰角 | 必须 |
| **P0** | 对象树 + 属性面板 | 可增删改对象 | 必须 |
| **P0** | 时间线 + 轨道回放 | 多对象同时回放 | 必须 |
| **P1** | Access 结果可视化 | 在三维中高亮 Access 弧段 | 必须 |
| **P1** | 基础报告 | Access 窗口表格、时长统计、简单图表 | 必须 |
| **P1** | 报告导出 | CSV / JSON | 必须 |
| **P2** | AI 自然语言生成简单场景 | 原型可用即可 | 建议 |
| **P2** | 简单传感器锥显示 | 可选，不阻塞验收 | 可选 |

### 2.3 明确不纳入 Phase 1

- HPOP / 数值积分
- 覆盖网格分析
- 复杂传感器模型与 FOV 约束 Access
- 环境模型实际参与计算（仅可预留接口）
- 姿态动力学

---

## 3. 详细功能要求

### 3.1 多对象（P0）
- 场景中可同时存在多颗卫星、多个地面站。
- 对象树支持增删改、启用/禁用。
- 三维中用不同颜色区分不同卫星轨道。

### 3.2 Access（P0 + P1）
- 支持选择“某卫星 × 某地面站”计算 Access。
- 支持最小仰角约束。
- 结果包含开始时间、结束时间、时长、最大仰角。
- 在时间线和三维中可视化 Access 窗口（P1）。

### 3.3 报告（P1）
- Access 结果表格（可排序）。
- 基础统计（总时长、窗口数量、最长窗口）。
- 支持导出 CSV。
- 可选简单 ECharts 图表（仰角曲线或窗口甘特图）。

### 3.4 视觉标准（P0，继承严格要求）
- 必须使用真实离线地球影像（参考 `Openspace-Safety-Platform-II/web/public/` 与 `situation/`）。
- 禁止 GridImageryProvider 作为主底图。
- 卫星与轨道显示不得退化为默认点+细线。
- 整体深色风格参考 Safety Platform。

---

## 4. Phase 1 推荐开源项目（新增）

以下项目可直接用于或参考以加速 Phase 1，减少自研：

| 用途 | 推荐项目 | 说明 | 优先级 |
|------|----------|------|--------|
| **SGP4 传播** | [neuromorphicsystems/sgp4](https://github.com/neuromorphicsystems/sgp4) | 成熟 Rust SGP4 实现，支持 TLE/OMM | **强烈推荐** |
| **轨道可视化参考** | [Flowm/satvis](https://github.com/Flowm/satvis) | Vue + Cesium 卫星跟踪与过境预测，交互可参考 | 高 |
| **传感器锥（可选）** | [kaktus40/cesium-sensors](https://github.com/kaktus40/cesium-sensors) | Cesium 传感器体可视化插件 | 中（P2） |
| **简单轨道绘制参考** | [KhaledSharif/cesium-satellites](https://github.com/KhaledSharif/cesium-satellites) | 从 TLE 绘制轨道的基础实现 | 中 |
| **中文态势参考** | [Viva525/Cesium-Satellite](https://github.com/Viva525/Cesium-Satellite) | 国内 Cesium 卫星态势项目 | 中 |

**数据来源**（Phase 1 即可使用）：
- CelesTrak：TLE / OMM（公开，无需注册）
- Space-Track：官方编目（需注册，后续可接）

---

## 5. 验收标准（Phase 1 结束必须全部满足）

- [ ] 地球为真实影像底图，非网格。
- [ ] 可创建场景并添加至少 2 颗卫星 + 2 个地面站。
- [ ] 可对任意卫星-地面站组合计算 Access。
- [ ] 时间线可回放，多对象轨道同时显示。
- [ ] Access 窗口可在界面中查看并导出 CSV。
- [ ] 对象树与属性面板可用。
- [ ] 视觉风格通过 Supervisor 确认。
- [ ] 所有轨道与 Access 计算在 Rust 后端完成（SGP4 优先使用推荐 crate）。

---

## 6. 开发顺序建议

1. 先完成地球影像与基础三维框架（解决 Phase 0 欠账）。
2. 场景与多对象模型 + 对象树。
3. 集成 `neuromorphicsystems/sgp4` + 异步任务。
4. Access 计算与结果存储。
5. 时间线回放 + Access 可视化（参考 satvis 交互）。
6. 报告与导出。
7. AI 原型（有余力再做）。

---

## 7. 评审节点

| 节点 | 检查重点 |
|------|----------|
| Phase 1 中期 | 地球影像 + 多对象 + SGP4 可跑 |
| Phase 1 结束 | 上述全部验收标准 |

未通过不得进入 Phase 2。

---

## 8. 参考路径（强制）

- 三维与地球：`Openspace-Safety-Platform-II/web/src/views/situation/` + `web/public/`
- 仿真交互参考：`SSA-Watch-Planning/frontend/components/`
- 数值：`stkcraft-time`、`stkcraft-geocalc` + **neuromorphicsystems/sgp4**
- 可视化参考：`Flowm/satvis`

---

**说明**：本文件为 Phase 1 专用范围与优先级定义。Phase 2 相关开源项目（如 satkit、brahe、dSGP4 等）将写入后续 Phase 2 计划书。

# KST Phase 0 改进方案（给 TRAE）

**评审人**：Supervisor  
**日期**：2026-11-25  
**当前问题**：功能链路已通，但三维显示与整体视觉远低于设计书要求。  
**目标**：把当前“能跑的验证页”提升到接近 Openspace Safety Platform 三维态势的基础观感。

---

## 一、必须改进项（优先级最高）

### 1. Cesium 地球底图与视觉

**现状问题**：使用 `GridImageryProvider`，地球几乎是纯色+网格，观感差。

**必须参考**：
- 仓库：`jeffreytanhao-eng/Openspace-Safety-Platform-II`
- 路径：`web/src/views/situation/`
- 重点文件/逻辑：
  - Cesium 地球初始化部分
  - 离线底图（NaturalEarth 或项目内影像）的加载方式
  - 大气、晨昏线、光照相关设置

**改进要求**：
- 替换当前网格底图，使用离线地球影像（可从 Safety Platform 的 `public/` 中借鉴资源组织方式）。
- 打开合理的大气效果（`showGroundAtmosphere` 等）。
- 调整背景色、地球基础色，使其更接近真实地球而非纯色块。
- 参考 Safety Platform 中相机默认视角与交互设置。

### 2. 卫星与轨道显示

**现状问题**：只有一个点和一条细线，过于简陋。

**必须参考**：
- 同上，`web/src/views/situation/` 中卫星点、轨道线、标签的绘制方式。
- 可同时参考 `SSA-Watch-Planning/frontend/components/` 中 `SimModal` 的航迹绘制。

**改进要求**：
- 卫星点增加更明显的样式（大小、颜色、光晕或简单模型占位）。
- 轨道线使用更合适的材质与宽度。
- 增加基本标签显示（卫星名称）。
- 计算完成后相机平滑飞到合适视角（当前已有，可优化参数）。

---

## 二、建议改进项

### 3. 整体布局与深色主题一致性

**参考**：
- Openspace-Safety-Platform-II 的整体深色航天风格（配色、面板密度、字体）。
- 当前 KST 已有深色基础，需对齐精细度。

**要求**：
- 面板、边框、文字颜色与 Safety Platform 保持相近风格。
- 确保 Cesium 控件隐藏后界面干净。

### 4. 代码组织

**要求**：
- 把 Cesium 初始化与实体渲染逻辑从 `SituationView.vue` 中适度拆分（例如 `composables/useCesiumViewer.ts`），便于后续 Phase 1 扩展。
- 保持“计算全在 Rust、前端只渲染”的原则不变。

---

## 三、明确不要求（Phase 0 范围内）

- 不要求实现完整对象树、时间线（属于 Phase 1）。
- 不要求接入 SGP4（当前二体演示可保留）。
- 不要求复杂传感器或 Access 可视化。

---

## 四、验收标准（改进后）

1. 打开页面能看到**有影像的地球**（非纯网格）。
2. 计算后能清晰看到卫星位置与轨道线。
3. 整体视觉风格接近 Openspace Safety Platform 的深色三维态势。
4. 原有功能（登录、计算、状态显示）不回退。

---

## 五、参考路径速查

| 改进点 | 参考仓库 | 具体路径 |
|--------|----------|----------|
| 地球底图与 Cesium 初始化 | Openspace-Safety-Platform-II | `web/src/views/situation/` |
| 离线影像资源组织 | Openspace-Safety-Platform-II | `web/public/`（底图相关） |
| 轨道/卫星绘制 | Openspace-Safety-Platform-II + SSA-Watch-Planning | `situation/` 与 `SimModal` |
| 深色主题风格 | Openspace-Safety-Platform-II | 整体 UI 配色 |

---

**说明**：请 TRAE 按以上方案修改，优先完成地球底图与视觉提升。完成后提交，Supervisor 再进行复审。

以后所有给 TRAE 的指导与评审意见均以 Markdown 文件形式交付。

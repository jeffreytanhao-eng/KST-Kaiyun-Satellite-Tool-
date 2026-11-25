<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref, shallowRef } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import * as Cesium from 'cesium'
import 'cesium/Build/Cesium/Widgets/widgets.css'

import { computeState, type StateResult } from '@/api/compute'
import { useAuthStore } from '@/stores/auth'

const router = useRouter()
const auth = useAuthStore()

const productName = 'KST · 开云卫星工具'
const phaseLabel = 'Phase 0 · 基础打通'

const viewerHost = ref<HTMLDivElement | null>(null)
const viewer = shallowRef<Cesium.Viewer | null>(null)
const computing = ref(false)
const result = ref<StateResult | null>(null)

const form = reactive({
  epoch: new Date(),
  semiMajorAxisKm: 6878.137,
  eccentricity: 0.001,
  inclinationDeg: 97.6,
  raanDeg: 30,
  argPerigeeDeg: 0,
  trueAnomalyDeg: 0
})

function initViewer() {
  if (!viewerHost.value) return

  const instance = new Cesium.Viewer(viewerHost.value, {
    baseLayer: false,
    animation: false,
    timeline: false,
    baseLayerPicker: false,
    fullscreenButton: false,
    geocoder: false,
    homeButton: false,
    infoBox: false,
    navigationHelpButton: false,
    sceneModePicker: false,
    selectionIndicator: false,
    creditContainer: document.createElement('div')
  })

  instance.scene.backgroundColor = Cesium.Color.fromCssColorString('#05080f')
  instance.scene.globe.baseColor = Cesium.Color.fromCssColorString('#0b1728')
  instance.scene.globe.showGroundAtmosphere = true
  instance.scene.screenSpaceCameraController.enableCollisionDetection = false
  instance.imageryLayers.addImageryProvider(
    new Cesium.GridImageryProvider({
      cells: 4,
      color: Cesium.Color.fromCssColorString('#1b3358'),
      glowColor: Cesium.Color.fromCssColorString('#0f2237'),
      backgroundColor: Cesium.Color.TRANSPARENT
    })
  )
  instance.camera.setView({
    destination: Cesium.Cartesian3.fromDegrees(110, 25, 24_000_000)
  })

  viewer.value = instance
}

function renderResult(data: StateResult) {
  const instance = viewer.value
  if (!instance) return

  instance.entities.removeAll()
  instance.entities.add({
    position: Cesium.Cartesian3.fromDegrees(
      data.longitude_deg,
      data.latitude_deg,
      data.altitude_km * 1000
    ),
    point: {
      pixelSize: 10,
      color: Cesium.Color.fromCssColorString('#4fc3f7'),
      outlineColor: Cesium.Color.WHITE,
      outlineWidth: 2,
      disableDepthTestDistance: Number.POSITIVE_INFINITY
    },
    label: {
      text: 'KST-SAT-01',
      font: '12px "Segoe UI", sans-serif',
      fillColor: Cesium.Color.WHITE,
      pixelOffset: new Cesium.Cartesian2(0, -20),
      disableDepthTestDistance: Number.POSITIVE_INFINITY
    }
  })

  if (data.orbit_track.length > 1) {
    const flat = data.orbit_track.flatMap(([longitude, latitude, altitude]) => [
      longitude,
      latitude,
      altitude * 1000
    ])
    instance.entities.add({
      polyline: {
        positions: Cesium.Cartesian3.fromDegreesArrayHeights(flat),
        width: 2,
        arcType: Cesium.ArcType.NONE,
        material: Cesium.Color.fromCssColorString('#2f9bff').withAlpha(0.85)
      }
    })
  }

  instance.camera.flyTo({
    destination: Cesium.Cartesian3.fromDegrees(
      data.longitude_deg,
      data.latitude_deg,
      16_000_000
    ),
    duration: 1.2
  })
}

async function runCompute() {
  computing.value = true
  try {
    const data = await computeState({
      epoch: form.epoch.toISOString(),
      samples: 180,
      orbit: {
        semi_major_axis_km: form.semiMajorAxisKm,
        eccentricity: form.eccentricity,
        inclination_deg: form.inclinationDeg,
        raan_deg: form.raanDeg,
        arg_perigee_deg: form.argPerigeeDeg,
        true_anomaly_deg: form.trueAnomalyDeg
      }
    })
    result.value = data
    renderResult(data)
  } catch (error) {
    ElMessage.error(error instanceof Error ? error.message : '计算失败')
  } finally {
    computing.value = false
  }
}

function vec(values: number[]) {
  return values.map((value) => value.toFixed(3)).join('  ')
}

async function onLogout() {
  auth.logout()
  await router.replace({ name: 'login' })
}

onMounted(() => {
  initViewer()
  void runCompute()
})

onBeforeUnmount(() => {
  viewer.value?.destroy()
  viewer.value = null
})
</script>

<template>
  <div class="shell">
    <header class="header">
      <span class="product">{{ productName }}</span>
      <el-tag size="small" effect="dark" type="info">{{ phaseLabel }}</el-tag>
      <span class="spacer" />
      <span class="user">{{ auth.user?.username ?? '未知用户' }}</span>
      <el-button link type="primary" @click="onLogout">退出登录</el-button>
    </header>

    <main class="workspace">
      <aside class="panel panel-left">
        <div class="panel-title">轨道参数</div>
        <el-form label-position="top" size="small" class="form">
          <el-form-item label="时刻（UTC）">
            <el-date-picker
              v-model="form.epoch"
              type="datetime"
              :clearable="false"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item label="半长轴 a（km）">
            <el-input-number
              v-model="form.semiMajorAxisKm"
              :min="6400"
              :step="10"
              :precision="3"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item label="偏心率 e">
            <el-input-number
              v-model="form.eccentricity"
              :min="0"
              :max="0.9"
              :step="0.001"
              :precision="4"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item label="倾角 i（°）">
            <el-input-number
              v-model="form.inclinationDeg"
              :min="0"
              :max="180"
              :step="0.1"
              :precision="2"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item label="升交点赤经 Ω（°）">
            <el-input-number
              v-model="form.raanDeg"
              :min="0"
              :max="360"
              :step="1"
              :precision="2"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item label="近地点幅角 ω（°）">
            <el-input-number
              v-model="form.argPerigeeDeg"
              :min="0"
              :max="360"
              :step="1"
              :precision="2"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item label="真近点角 ν（°）">
            <el-input-number
              v-model="form.trueAnomalyDeg"
              :min="0"
              :max="360"
              :step="1"
              :precision="2"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
        </el-form>
        <el-button type="primary" class="run" :loading="computing" @click="runCompute">
          计算并显示
        </el-button>
        <p class="panel-note">
          时间转换、开普勒递推与 J2000→地固系旋转均由 Rust 后端完成，前端仅负责展示。
        </p>
      </aside>

      <section class="viewport">
        <div ref="viewerHost" class="cesium-host" />
        <div class="viewport-tip">左键旋转 · 滚轮缩放 · 右键/中键平移</div>
      </section>

      <aside class="panel panel-right">
        <div class="panel-title">计算结果</div>
        <template v-if="result">
          <div class="row"><span>UTC</span><b>{{ result.epoch }}</b></div>
          <div class="row"><span>ET (s)</span><b>{{ result.et_seconds.toFixed(3) }}</b></div>
          <div class="group">ECI 位置（km）</div>
          <div class="vec">{{ vec(result.eci.position_km) }}</div>
          <div class="group">ECI 速度（km/s）</div>
          <div class="vec">{{ vec(result.eci.velocity_km_s) }}</div>
          <div class="group">地固系位置（km）</div>
          <div class="vec">{{ vec(result.ecef.position_km) }}</div>
          <div class="group">大地坐标</div>
          <div class="row"><span>经度</span><b>{{ result.longitude_deg.toFixed(3) }}°</b></div>
          <div class="row"><span>纬度</span><b>{{ result.latitude_deg.toFixed(3) }}°</b></div>
          <div class="row"><span>高度</span><b>{{ result.altitude_km.toFixed(3) }} km</b></div>
        </template>
        <el-empty v-else description="尚未计算" :image-size="72" />
      </aside>
    </main>
  </div>
</template>

<style scoped lang="scss">
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 16px;
  height: 48px;
  flex: none;
  background: var(--kst-panel);
  border-bottom: 1px solid var(--kst-border);
}

.product {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: 0.04em;
}

.spacer {
  flex: 1;
}

.user {
  color: var(--kst-text-dim);
  font-size: 13px;
}

.workspace {
  flex: 1;
  min-height: 0;
  display: flex;
}

.panel {
  width: 288px;
  flex: none;
  display: flex;
  flex-direction: column;
  padding: 14px;
  background: var(--kst-panel);
  overflow-y: auto;
}

.panel-left {
  border-right: 1px solid var(--kst-border);
}

.panel-right {
  border-left: 1px solid var(--kst-border);
}

.panel-title {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: var(--kst-text);
  margin-bottom: 12px;
}

.form {
  :deep(.el-form-item) {
    margin-bottom: 10px;
  }

  :deep(.el-form-item__label) {
    padding-bottom: 2px;
    font-size: 12px;
    color: var(--kst-text-dim);
  }
}

.run {
  width: 100%;
  margin-top: 4px;
}

.panel-note {
  margin: 12px 0 0;
  font-size: 11px;
  line-height: 1.6;
  color: var(--kst-text-dim);
}

.viewport {
  position: relative;
  flex: 1;
  min-width: 0;
  background: #05080f;
}

.cesium-host {
  position: absolute;
  inset: 0;
}

.viewport-tip {
  position: absolute;
  left: 12px;
  bottom: 12px;
  padding: 4px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 4px;
  background: rgba(9, 15, 26, 0.82);
  font-size: 11px;
  color: var(--kst-text-dim);
  pointer-events: none;
}

.row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  color: var(--kst-text-dim);
  padding: 3px 0;

  b {
    color: var(--kst-text);
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
}

.group {
  margin-top: 12px;
  margin-bottom: 4px;
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--kst-accent);
}

.vec {
  font-family: Consolas, 'Cascadia Mono', monospace;
  font-size: 12px;
  line-height: 1.6;
  color: var(--kst-text);
  font-variant-numeric: tabular-nums;
  white-space: pre;
}
</style>

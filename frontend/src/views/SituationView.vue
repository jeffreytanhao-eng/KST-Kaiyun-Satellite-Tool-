<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'

import LangSwitch from '@/components/LangSwitch.vue'
import { computeState, type StateResult } from '@/api/compute'
import { useCesiumViewer } from '@/composables/useCesiumViewer'
import { useAuthStore } from '@/stores/auth'
import { useI18n } from '@/i18n'

const router = useRouter()
const auth = useAuthStore()
const { t } = useI18n()

const viewerHost = ref<HTMLDivElement | null>(null)
const { init, renderState } = useCesiumViewer(viewerHost)
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
    renderState(data)
  } catch (error) {
    ElMessage.error(error instanceof Error ? error.message : t('computeFailed'))
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

onMounted(async () => {
  await init()
  await runCompute()
})
</script>

<template>
  <div class="shell">
    <header class="header">
      <span class="product">{{ t('appTitle') }}</span>
      <el-tag size="small" effect="dark" type="info" class="phase">{{ t('phaseLabel') }}</el-tag>
      <span class="spacer" />
      <LangSwitch />
      <span class="user">{{ auth.user?.username ?? t('unknownUser') }}</span>
      <el-button link type="primary" @click="onLogout">{{ t('logout') }}</el-button>
    </header>

    <main class="workspace">
      <aside class="panel panel-left">
        <div class="panel-title">{{ t('orbitParams') }}</div>
        <el-form label-position="top" size="small" class="form">
          <el-form-item :label="t('epochUtc')">
            <el-date-picker
              v-model="form.epoch"
              type="datetime"
              :clearable="false"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item :label="t('semiMajorAxis')">
            <el-input-number
              v-model="form.semiMajorAxisKm"
              :min="6400"
              :step="10"
              :precision="3"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item :label="t('eccentricity')">
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
          <el-form-item :label="t('inclination')">
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
          <el-form-item :label="t('raan')">
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
          <el-form-item :label="t('argPerigee')">
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
          <el-form-item :label="t('trueAnomaly')">
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
          {{ t('compute') }}
        </el-button>
        <p class="panel-note">{{ t('computeNote') }}</p>
      </aside>

      <section class="viewport">
        <div ref="viewerHost" class="cesium-host" />
        <div class="viewport-tip">{{ t('viewportTip') }}</div>
      </section>

      <aside class="panel panel-right">
        <div class="panel-title">{{ t('resultTitle') }}</div>
        <template v-if="result">
          <div class="row"><span>{{ t('utc') }}</span><b>{{ result.epoch }}</b></div>
          <div class="row"><span>{{ t('etSeconds') }}</span><b>{{ result.et_seconds.toFixed(3) }}</b></div>
          <div class="group">{{ t('eciPosition') }}</div>
          <div class="vec">{{ vec(result.eci.position_km) }}</div>
          <div class="group">{{ t('eciVelocity') }}</div>
          <div class="vec">{{ vec(result.eci.velocity_km_s) }}</div>
          <div class="group">{{ t('ecefPosition') }}</div>
          <div class="vec">{{ vec(result.ecef.position_km) }}</div>
          <div class="group">{{ t('geodetic') }}</div>
          <div class="row"><span>{{ t('longitude') }}</span><b>{{ result.longitude_deg.toFixed(3) }}°</b></div>
          <div class="row"><span>{{ t('latitude') }}</span><b>{{ result.latitude_deg.toFixed(3) }}°</b></div>
          <div class="row"><span>{{ t('altitude') }}</span><b>{{ result.altitude_km.toFixed(3) }} km</b></div>
        </template>
        <el-empty v-else :description="t('noResult')" :image-size="72" />
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

.phase {
  border-color: var(--kst-border);
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
  background: #04060b;
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
  background: rgba(9, 13, 18, 0.82);
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
  color: var(--kst-accent-strong);
}

.vec {
  font-family: 'JetBrains Mono', Consolas, 'Cascadia Mono', monospace;
  font-size: 12px;
  line-height: 1.6;
  color: var(--kst-text);
  font-variant-numeric: tabular-nums;
  white-space: pre;
}
</style>

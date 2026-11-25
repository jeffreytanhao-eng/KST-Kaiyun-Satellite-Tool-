<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Download } from '@element-plus/icons-vue'

import { downloadJobCsv } from '@/api/jobs'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const store = useScenarioStore()
const { t } = useI18n()

const form = reactive({
  satelliteId: '',
  start: new Date(),
  end: new Date(Date.now() + 3 * 3600 * 1000),
  stepSeconds: 120,
  dailyF107: 150,
  meanF107: 150,
  ap: 4,
  massKg: 500,
  areaM2: 2,
  dragCoefficient: 2.2
})

const report = computed(() => store.environmentReport)

const chart = computed(() => {
  const samples = report.value?.samples ?? []
  if (samples.length === 0) return null
  const stride = Math.max(1, Math.ceil(samples.length / 120))
  const picked = samples.filter((_, index) => index % stride === 0)
  const values = picked.map((sample) => Math.log10(Math.max(sample.density_kg_m3, 1e-15)))
  const min = Math.min(...values)
  const max = Math.max(...values)
  const span = max - min || 1
  const points = values
    .map((value, index) => {
      const x = (index / Math.max(values.length - 1, 1)) * 300
      const y = 84 - ((value - min) / span) * 70
      return `${x.toFixed(2)},${y.toFixed(2)}`
    })
    .join(' ')
  return { points, min, max }
})

function syncSatellite() {
  const satellites = store.current?.data.satellites ?? []
  if (!satellites.some((item) => item.id === form.satelliteId)) {
    form.satelliteId = satellites.find((item) => item.enabled)?.id ?? satellites[0]?.id ?? ''
  }
}

watch(() => store.current, syncSatellite, { immediate: true })

async function onRun() {
  if (!form.satelliteId) {
    ElMessage.warning(t('noSatellites'))
    return
  }
  try {
    const result = await store.runEnvironment({
      satelliteId: form.satelliteId,
      start: form.start,
      end: form.end,
      stepSeconds: form.stepSeconds,
      dailyF107: form.dailyF107,
      meanF107: form.meanF107,
      ap: form.ap,
      massKg: form.massKg,
      areaM2: form.areaM2,
      dragCoefficient: form.dragCoefficient
    })
    if (result) {
      ElMessage.success(t('environmentResult'))
    }
  } catch (error) {
    ElMessage.error(`${t('environmentFailed')}: ${errorText(error)}`)
  }
}

async function onExportCsv() {
  const job = store.environmentJob
  if (!job) return
  try {
    await downloadJobCsv(job.id, `environment-${job.id.slice(0, 8)}.csv`)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function density(value: number) {
  return value.toExponential(2)
}
</script>

<template>
  <div class="section-title">{{ t('environmentAnalysis') }}</div>
  <el-form label-position="top" size="small" class="form">
    <el-form-item :label="t('environmentSatellite')">
      <el-select v-model="form.satelliteId" style="width: 100%">
        <el-option
          v-for="satellite in store.current?.data.satellites ?? []"
          :key="satellite.id"
          :label="satellite.name"
          :value="satellite.id"
        />
      </el-select>
    </el-form-item>
    <el-form-item :label="t('accessStart')">
      <el-date-picker v-model="form.start" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessEnd')">
      <el-date-picker v-model="form.end" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessStep')">
      <el-input-number v-model="form.stepSeconds" :min="10" :max="600" :step="30" controls-position="right" style="width: 100%" />
    </el-form-item>

    <div class="grid2">
      <el-form-item :label="t('envDailyF107')">
        <el-input-number v-model="form.dailyF107" :min="50" :max="400" :step="5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('envMeanF107')">
        <el-input-number v-model="form.meanF107" :min="50" :max="400" :step="5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('envAp')">
        <el-input-number v-model="form.ap" :min="0" :max="400" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('envMass')">
        <el-input-number v-model="form.massKg" :min="1" :max="20000" :step="50" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('envArea')">
        <el-input-number v-model="form.areaM2" :min="0.1" :max="200" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('envCd')">
        <el-input-number v-model="form.dragCoefficient" :min="1" :max="5" :step="0.1" controls-position="right" style="width: 100%" />
      </el-form-item>
    </div>
  </el-form>

  <el-button type="primary" size="small" class="full" :loading="store.environmentBusy" @click="onRun">
    {{ store.environmentBusy ? t('computingEnvironment') : t('computeEnvironment') }}
  </el-button>

  <el-alert
    v-if="store.environmentJob?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="store.environmentJob.error ?? t('environmentFailed')"
  />

  <template v-if="report">
    <div class="section-title">{{ t('environmentResult') }}</div>
    <div class="model-line">{{ report.model }}</div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('envStartAltitude') }}</span>
        <b>{{ report.start_altitude_km.toFixed(1) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('envEndAltitude') }}</span>
        <b>{{ report.end_altitude_km.toFixed(1) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('envCorrectedAltitude') }}</span>
        <b>{{ report.corrected_end_altitude_km.toFixed(2) }} km</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('envMeanDensity') }}</span>
        <b class="small">{{ density(report.mean_density_kg_m3) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('envDeltaV') }}</span>
        <b>{{ report.delta_v_m_s.toFixed(2) }} m/s</b>
      </div>
      <div class="stat">
        <span>{{ t('envAltitudeLoss') }}</span>
        <b>{{ report.estimated_altitude_loss_km.toFixed(3) }} km</b>
      </div>
    </div>

    <div v-if="chart" class="chart">
      <svg viewBox="0 0 300 90" preserveAspectRatio="none">
        <polyline :points="chart.points" fill="none" stroke="#4fc3f7" stroke-width="1.6" />
      </svg>
      <div class="chart-legend">
        <span>{{ t('envDensityCurve') }}</span>
        <span>{{ density(report.min_density_kg_m3) }} → {{ density(report.max_density_kg_m3) }}</span>
      </div>
    </div>

    <div class="note">{{ report.note }}</div>

    <el-button size="small" :icon="Download" class="full" @click="onExportCsv">
      {{ t('exportCsv') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('noEnvironmentResult')" :image-size="56" />
</template>

<style scoped lang="scss">
.form {
  :deep(.el-form-item) {
    margin-bottom: 8px;
  }

  :deep(.el-form-item__label) {
    padding-bottom: 2px;
    font-size: 12px;
    color: var(--kst-text-dim);
  }
}

.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0 8px;
}

.section-title {
  margin: 12px 0 8px;
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--kst-accent-strong);
}

.full {
  width: 100%;
  margin-top: 4px;
}

.job-error {
  margin-top: 8px;
}

.model-line {
  font-size: 11px;
  color: var(--kst-text-dim);
  margin-bottom: 8px;
}

.stats {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 6px;
  margin-bottom: 8px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 7px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(79, 195, 247, 0.05);

  span {
    font-size: 10px;
    color: var(--kst-text-dim);
  }

  b {
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  b.small {
    font-size: 10.5px;
  }
}

.chart {
  margin: 6px 0 8px;
  padding: 6px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.6);

  svg {
    width: 100%;
    height: 90px;
    display: block;
  }
}

.chart-legend {
  display: flex;
  justify-content: space-between;
  font-size: 10px;
  color: var(--kst-text-dim);
  margin-top: 4px;
}

.note {
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--kst-text-dim);
  margin-bottom: 8px;
}
</style>

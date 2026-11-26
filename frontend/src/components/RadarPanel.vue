<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Download } from '@element-plus/icons-vue'

import { downloadJobCsv, type RadarOptions, type RadarReport } from '@/api/jobs'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const emit = defineEmits<{
  (event: 'show-radar', report: RadarReport | null): void
}>()

const store = useScenarioStore()
const { t } = useI18n()

const form = reactive({
  satelliteId: '',
  stationId: '',
  start: new Date(),
  end: new Date(Date.now() + 6 * 3600 * 1000),
  stepSeconds: 30,
  frequencyMhz: 3000,
  txPowerW: 1000,
  txGainDb: 35,
  rxGainDb: 35,
  systemLossDb: 3,
  noiseFigureDb: 3,
  bandwidthMhz: 1,
  targetRcs: 1,
  requiredSnrDb: 13,
  pulses: 1,
  minElevationDeg: 5
})

const report = computed(() => store.radarReport)

const chart = computed(() => {
  const samples = report.value?.samples ?? []
  if (samples.length === 0) return null
  const stride = Math.max(1, Math.ceil(samples.length / 160))
  const picked = samples.filter((_, index) => index % stride === 0)
  const values = picked.map((sample) => sample.snr_db)
  const threshold = report.value?.required_snr_db ?? 0
  const min = Math.min(...values, threshold) - 3
  const max = Math.max(...values, threshold) + 3
  const span = max - min || 1
  const width = 300
  const height = 104
  const toPoint = (value: number, index: number) => {
    const x = (index / Math.max(picked.length - 1, 1)) * width
    const y = height - ((value - min) / span) * height
    return `${x.toFixed(2)},${y.toFixed(2)}`
  }
  const points = values.map(toPoint).join(' ')
  const thresholdY = (height - ((threshold - min) / span) * height).toFixed(2)
  return { points, thresholdY, width, height, min, max }
})

function syncDefaults() {
  const data = store.current?.data
  const satellites = data?.satellites ?? []
  if (!satellites.some((item) => item.id === form.satelliteId)) {
    form.satelliteId = satellites.find((item) => item.enabled)?.id ?? satellites[0]?.id ?? ''
  }
  const stations = data?.ground_stations ?? []
  if (!stations.some((item) => item.id === form.stationId)) {
    form.stationId = stations[0]?.id ?? ''
  }
  syncMinElevation()
}

function syncMinElevation() {
  const station = store.current?.data.ground_stations.find(
    (item) => item.id === form.stationId
  )
  if (station) form.minElevationDeg = station.min_elevation_deg
}

watch(() => store.current, syncDefaults, { immediate: true })

async function onRun() {
  if (!form.satelliteId || !form.stationId) {
    ElMessage.warning(t('radarSelectRequired'))
    return
  }
  const options: RadarOptions = {
    frequency_hz: form.frequencyMhz * 1e6,
    tx_power_w: form.txPowerW,
    tx_gain_db: form.txGainDb,
    rx_gain_db: form.rxGainDb,
    system_loss_db: form.systemLossDb,
    noise_figure_db: form.noiseFigureDb,
    bandwidth_hz: form.bandwidthMhz * 1e6,
    target_rcs_m2: form.targetRcs,
    required_snr_db: form.requiredSnrDb,
    pulses_integrated: form.pulses,
    min_elevation_deg: form.minElevationDeg
  }
  emit('show-radar', null)
  try {
    const result = await store.runRadar({
      satelliteId: form.satelliteId,
      stationId: form.stationId,
      start: form.start,
      end: form.end,
      stepSeconds: form.stepSeconds,
      options
    })
    if (result) {
      emit('show-radar', result)
      ElMessage.success(
        `${t('radarResult')}: ${result.max_detection_range_km.toFixed(1)} km`
      )
    }
  } catch (error) {
    ElMessage.error(`${t('radarFailed')}: ${errorText(error)}`)
  }
}

async function onExportCsv() {
  const job = store.radarJob
  if (!job) return
  try {
    await downloadJobCsv(job.id, `radar-${job.id.slice(0, 8)}.csv`)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatDuration(seconds: number) {
  const total = Math.max(0, Math.round(seconds))
  const hours = Math.floor(total / 3600)
  const minutes = Math.floor((total % 3600) / 60)
  if (hours > 0) return `${hours}h ${minutes}m`
  if (minutes > 0) return `${minutes}m ${total % 60}s`
  return `${total}s`
}

function formatUtcString(value: string) {
  return value.replace('T', ' ').replace(/\.\d+Z$/, 'Z')
}
</script>

<template>
  <div class="section-title">{{ t('radarAnalysis') }}</div>
  <el-form label-position="top" size="small" class="form">
    <div class="grid2">
      <el-form-item :label="t('accessSatellite')">
        <el-select v-model="form.satelliteId" style="width: 100%">
          <el-option
            v-for="item in store.current?.data.satellites ?? []"
            :key="item.id"
            :label="item.name"
            :value="item.id"
          />
        </el-select>
      </el-form-item>
      <el-form-item :label="t('accessStation')">
        <el-select v-model="form.stationId" style="width: 100%" @change="syncMinElevation">
          <el-option
            v-for="item in store.current?.data.ground_stations ?? []"
            :key="item.id"
            :label="item.name"
            :value="item.id"
          />
        </el-select>
      </el-form-item>
    </div>
    <el-form-item :label="t('accessStart')">
      <el-date-picker v-model="form.start" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessEnd')">
      <el-date-picker v-model="form.end" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <div class="grid2">
      <el-form-item :label="t('accessStep')">
        <el-input-number v-model="form.stepSeconds" :min="1" :max="600" :step="5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarMinElevation')">
        <el-input-number v-model="form.minElevationDeg" :min="0" :max="90" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarFrequency')">
        <el-input-number v-model="form.frequencyMhz" :min="1" :max="1000000" :step="100" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarBandwidth')">
        <el-input-number v-model="form.bandwidthMhz" :min="0.001" :max="1000" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarPower')">
        <el-input-number v-model="form.txPowerW" :min="1" :max="1e9" :step="100" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarTxGain')">
        <el-input-number v-model="form.txGainDb" :min="-10" :max="80" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarRxGain')">
        <el-input-number v-model="form.rxGainDb" :min="-10" :max="80" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarLoss')">
        <el-input-number v-model="form.systemLossDb" :min="0" :max="40" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarNoiseFigure')">
        <el-input-number v-model="form.noiseFigureDb" :min="0" :max="30" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarRcs')">
        <el-input-number v-model="form.targetRcs" :min="0.0001" :max="1000" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarSnr')">
        <el-input-number v-model="form.requiredSnrDb" :min="0" :max="60" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('radarPulses')">
        <el-input-number v-model="form.pulses" :min="1" :max="10000" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
    </div>
  </el-form>

  <el-button type="primary" size="small" class="full" :loading="store.radarBusy" @click="onRun">
    {{ store.radarBusy ? t('radarRunning') : t('radarRun') }}
  </el-button>

  <el-alert
    v-if="store.radarJob?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="store.radarJob.error ?? t('radarFailed')"
  />

  <template v-if="report">
    <div class="section-title">{{ t('radarResult') }}</div>
    <div class="model-line">
      {{ report.satellite_name }} → {{ report.ground_station_name }} ·
      {{ (report.frequency_hz / 1e6).toFixed(1) }} MHz
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('radarMaxRange') }}</span>
        <b>{{ report.max_detection_range_km.toFixed(1) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('radarDetectedWindows') }}</span>
        <b>{{ report.detected_window_count }}/{{ report.window_count }}</b>
      </div>
      <div class="stat">
        <span>{{ t('radarDetectSeconds') }}</span>
        <b>{{ formatDuration(report.total_detection_seconds) }}</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('radarIntegrationGain') }}</span>
        <b>{{ report.integration_gain_db.toFixed(2) }} dB</b>
      </div>
      <div class="stat">
        <span>{{ t('radarRangeMin') }}</span>
        <b>{{ report.min_range_km.toFixed(1) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('radarRangeMax') }}</span>
        <b>{{ report.max_range_km.toFixed(1) }} km</b>
      </div>
    </div>

    <div v-if="chart" class="curve">
      <div class="curve-title">{{ t('radarSnrCurve') }}</div>
      <svg :viewBox="`0 0 ${chart.width} ${chart.height}`" preserveAspectRatio="none">
        <line
          x1="0"
          :y1="chart.thresholdY"
          :x2="chart.width"
          :y2="chart.thresholdY"
          class="threshold"
        />
        <polyline :points="chart.points" class="snr" />
      </svg>
      <div class="curve-axis">
        <span>{{ chart.min.toFixed(0) }} dB</span>
        <span>{{ t('radarSnr') }} ≥ {{ report.required_snr_db.toFixed(0) }} dB</span>
        <span>{{ chart.max.toFixed(0) }} dB</span>
      </div>
    </div>

    <el-table :data="report.windows" size="small" height="220" class="table">
      <el-table-column prop="index" :label="t('windowIndex')" width="48" />
      <el-table-column :label="t('windowStart')" min-width="122">
        <template #default="{ row }">{{ formatUtcString(row.start_utc) }}</template>
      </el-table-column>
      <el-table-column :label="t('windowDuration')" width="76">
        <template #default="{ row }">{{ formatDuration(row.duration_seconds) }}</template>
      </el-table-column>
      <el-table-column :label="t('radarRangeMin')" width="80">
        <template #default="{ row }">{{ row.min_range_km.toFixed(1) }}</template>
      </el-table-column>
      <el-table-column :label="t('radarRangeMax')" width="80">
        <template #default="{ row }">{{ row.max_range_km.toFixed(1) }}</template>
      </el-table-column>
      <el-table-column :label="t('radarSnr')" width="82">
        <template #default="{ row }">
          <span :class="row.max_snr_db >= report.required_snr_db ? 'ok' : 'bad'">
            {{ row.max_snr_db.toFixed(1) }}
          </span>
        </template>
      </el-table-column>
    </el-table>

    <div class="legend">
      <span class="dot ok" /> {{ t('radarLegendDetect') }}
      <span class="dot bad" /> {{ t('radarLegendMiss') }}
    </div>

    <el-button size="small" :icon="Download" class="full" @click="onExportCsv">
      {{ t('exportCsv') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('radarNoResult')" :image-size="56" />
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
    font-size: 11.5px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
}

.curve {
  margin: 4px 0 10px;
  padding: 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(6, 12, 22, 0.4);

  svg {
    width: 100%;
    height: 104px;
    display: block;
  }

  .snr {
    fill: none;
    stroke: #4fc3f7;
    stroke-width: 1.6;
    vector-effect: non-scaling-stroke;
  }

  .threshold {
    stroke: #ef5350;
    stroke-width: 1;
    stroke-dasharray: 4 3;
    vector-effect: non-scaling-stroke;
  }
}

.curve-title {
  font-size: 10.5px;
  color: var(--kst-text-dim);
  margin-bottom: 4px;
}

.curve-axis {
  display: flex;
  justify-content: space-between;
  font-size: 9.5px;
  color: var(--kst-text-dim);
  margin-top: 4px;
}

.table {
  margin-bottom: 8px;
  --el-table-border-color: var(--kst-border);
  --el-table-bg-color: transparent;
  --el-table-tr-bg-color: transparent;
  --el-table-header-bg-color: rgba(79, 195, 247, 0.06);
  --el-table-text-color: var(--kst-text);
  --el-table-header-text-color: var(--kst-text-dim);
  font-size: 11px;
}

.ok {
  color: #66bb6a;
}

.bad {
  color: #ef5350;
}

.legend {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
  color: var(--kst-text-dim);
  margin: 0 0 8px;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
  margin-left: 6px;
}

.dot.ok {
  background: #66bb6a;
}

.dot.bad {
  background: #ef5350;
}
</style>

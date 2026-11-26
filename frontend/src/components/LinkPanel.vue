<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Download } from '@element-plus/icons-vue'

import {
  downloadJobCsv,
  type AntennaPattern,
  type AtmosphereModel,
  type LinkOptions
} from '@/api/jobs'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const store = useScenarioStore()
const { t } = useI18n()

const form = reactive({
  satelliteId: '',
  stationId: '',
  start: new Date(),
  end: new Date(Date.now() + 6 * 3600 * 1000),
  stepSeconds: 30,
  frequencyMhz: 2200,
  txPowerDbw: 10,
  txGainDb: 30,
  rxGainDb: 35,
  miscLossDb: 1,
  noiseTempK: 500,
  bandwidthKhz: 1000,
  requiredCn0Dbm: 60,
  pattern: 'omni' as AntennaPattern,
  patternExponent: 2,
  atmosphere: 'simple' as AtmosphereModel,
  zenithAttenuation: 0.05
})

const report = computed(() => store.linkReport)
const job = computed(() => store.linkJob)

const interferenceForm = reactive({
  interfererIds: [] as string[],
  thresholdCiDb: 10
})

const interference = computed(() => store.interferenceReport)

const chart = computed(() => {
  const curve = report.value?.curve ?? []
  if (curve.length === 0) return null
  const stride = Math.max(1, Math.ceil(curve.length / 160))
  const picked = curve.filter((_, index) => index % stride === 0)
  const values = picked.map((point) => point.margin_db)
  const min = Math.min(...values, -3)
  const max = Math.max(...values, 3)
  const span = max - min || 1
  const width = 300
  const height = 96
  const points = values
    .map((value, index) => {
      const x = (index / Math.max(picked.length - 1, 1)) * width
      const y = height - ((value - min) / span) * height
      return `${x.toFixed(2)},${y.toFixed(2)}`
    })
    .join(' ')
  const zeroY = (height - ((0 - min) / span) * height).toFixed(2)
  return { points, zeroY, width, height, min, max }
})

function syncDefaults() {
  const data = store.current?.data
  const satellites = data?.satellites ?? []
  if (!satellites.some((item) => item.id === form.satelliteId)) {
    form.satelliteId =
      satellites.find((item) => item.enabled)?.id ?? satellites[0]?.id ?? ''
  }
  const stations = data?.ground_stations ?? []
  if (!stations.some((item) => item.id === form.stationId)) {
    form.stationId = stations[0]?.id ?? ''
  }
}

watch(() => store.current, syncDefaults, { immediate: true })

async function onRun() {
  if (!form.satelliteId || !form.stationId) {
    ElMessage.warning(t('linkSelectRequired'))
    return
  }
  const options: LinkOptions = {
    frequency_hz: form.frequencyMhz * 1e6,
    tx_power_dbw: form.txPowerDbw,
    tx_gain_db: form.txGainDb,
    rx_gain_db: form.rxGainDb,
    misc_loss_db: form.miscLossDb,
    system_noise_temp_k: form.noiseTempK,
    bandwidth_hz: form.bandwidthKhz * 1000,
    required_cn0_dbhz: form.requiredCn0Dbm,
    satellite_antenna_pattern: form.pattern,
    satellite_pattern_exponent: form.patternExponent,
    atmosphere_model: form.atmosphere,
    zenith_attenuation_db: form.zenithAttenuation
  }
  try {
    const result = await store.runLink({
      satelliteId: form.satelliteId,
      stationId: form.stationId,
      start: form.start,
      end: form.end,
      stepSeconds: form.stepSeconds,
      options
    })
    if (result) {
      ElMessage.success(t('linkResult'))
    }
  } catch (error) {
    ElMessage.error(`${t('linkFailed')}: ${errorText(error)}`)
  }
}

async function onExportCsv() {
  const current = job.value
  if (!current) return
  try {
    await downloadJobCsv(current.id, `link-budget-${current.id.slice(0, 8)}.csv`)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

async function onRunInterference() {
  if (!form.satelliteId || !form.stationId) {
    ElMessage.warning(t('linkSelectRequired'))
    return
  }
  if (interferenceForm.interfererIds.length === 0) {
    ElMessage.warning(t('interferenceSelectRequired'))
    return
  }
  const options: LinkOptions = {
    frequency_hz: form.frequencyMhz * 1e6,
    tx_power_dbw: form.txPowerDbw,
    tx_gain_db: form.txGainDb,
    rx_gain_db: form.rxGainDb,
    misc_loss_db: form.miscLossDb,
    system_noise_temp_k: form.noiseTempK,
    bandwidth_hz: form.bandwidthKhz * 1000,
    required_cn0_dbhz: form.requiredCn0Dbm,
    satellite_antenna_pattern: form.pattern,
    satellite_pattern_exponent: form.patternExponent,
    atmosphere_model: form.atmosphere,
    zenith_attenuation_db: form.zenithAttenuation
  }
  try {
    await store.runInterference({
      satelliteId: form.satelliteId,
      stationId: form.stationId,
      interfererIds: interferenceForm.interfererIds,
      start: form.start,
      end: form.end,
      stepSeconds: form.stepSeconds,
      thresholdCiDb: interferenceForm.thresholdCiDb,
      options
    })
    ElMessage.success(t('interferenceResult'))
  } catch (error) {
    ElMessage.error(`${t('interferenceFailed')}: ${errorText(error)}`)
  }
}

async function onExportInterferenceCsv() {
  const current = store.interferenceJob
  if (!current) return
  try {
    await downloadJobCsv(current.id, `interference-${current.id.slice(0, 8)}.csv`)
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
  <div class="section-title">{{ t('linkAnalysis') }}</div>
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
        <el-select v-model="form.stationId" style="width: 100%">
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
    <el-form-item :label="t('accessStep')">
      <el-input-number
        v-model="form.stepSeconds"
        :min="1"
        :max="600"
        :step="5"
        controls-position="right"
        style="width: 100%"
      />
    </el-form-item>

    <div class="grid2">
      <el-form-item :label="t('linkFrequency')">
        <el-input-number v-model="form.frequencyMhz" :min="1" :max="1000000" :step="100" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('linkBandwidth')">
        <el-input-number v-model="form.bandwidthKhz" :min="0.001" :max="1000000" :step="100" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('linkTxPower')">
        <el-input-number v-model="form.txPowerDbw" :min="-100" :max="200" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('linkTxGain')">
        <el-input-number v-model="form.txGainDb" :min="-10" :max="80" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('linkRxGain')">
        <el-input-number v-model="form.rxGainDb" :min="-10" :max="80" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('linkMiscLoss')">
        <el-input-number v-model="form.miscLossDb" :min="0" :max="30" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('linkNoiseTemp')">
        <el-input-number v-model="form.noiseTempK" :min="1" :max="100000" :step="50" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('linkRequiredCn0')">
        <el-input-number v-model="form.requiredCn0Dbm" :min="0" :max="150" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
    </div>

    <div class="section-title">{{ t('linkAdvancedModel') }}</div>
    <div class="grid2">
      <el-form-item :label="t('linkSatellitePattern')">
        <el-select v-model="form.pattern" style="width: 100%">
          <el-option :label="t('linkPatternOmni')" value="omni" />
          <el-option :label="t('linkPatternCosine')" value="cosine" />
        </el-select>
      </el-form-item>
      <el-form-item :label="t('linkPatternExponent')">
        <el-input-number
          v-model="form.patternExponent"
          :min="0.5"
          :max="8"
          :step="0.5"
          :disabled="form.pattern === 'omni'"
          controls-position="right"
          style="width: 100%"
        />
      </el-form-item>
      <el-form-item :label="t('linkAtmosphere')">
        <el-select v-model="form.atmosphere" style="width: 100%">
          <el-option :label="t('linkAtmosphereNone')" value="none" />
          <el-option :label="t('linkAtmosphereSimple')" value="simple" />
          <el-option :label="t('linkAtmosphereItu')" value="itu" />
        </el-select>
      </el-form-item>
      <el-form-item :label="t('linkZenithAttenuation')">
        <el-input-number
          v-model="form.zenithAttenuation"
          :min="0"
          :max="20"
          :step="0.01"
          :disabled="form.atmosphere === 'none'"
          controls-position="right"
          style="width: 100%"
        />
      </el-form-item>
    </div>
  </el-form>

  <el-button type="primary" size="small" class="full" :loading="store.linkBusy" @click="onRun">
    {{ store.linkBusy ? t('computingLink') : t('computeLink') }}
  </el-button>

  <el-alert
    v-if="job?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="job.error ?? t('linkFailed')"
  />

  <template v-if="report">
    <div class="section-title">{{ t('linkResult') }}</div>
    <div class="model-line">
      {{ report.satellite_name }} → {{ report.ground_station_name }} ·
      {{ (report.frequency_hz / 1e6).toFixed(1) }} MHz
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('windowCount') }}</span>
        <b>{{ report.window_count }}</b>
      </div>
      <div class="stat">
        <span>{{ t('totalDuration') }}</span>
        <b>{{ formatDuration(report.total_duration_seconds) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('linkMinMargin') }}</span>
        <b>{{ report.min_margin_db.toFixed(2) }} dB</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('linkMeanMargin') }}</span>
        <b>{{ report.mean_margin_db.toFixed(2) }} dB</b>
      </div>
      <div class="stat">
        <span>{{ t('linkMaxRange') }}</span>
        <b>{{ report.max_range_km.toFixed(1) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('linkMeanFsl') }}</span>
        <b>{{ report.mean_free_space_loss_db.toFixed(2) }} dB</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('linkMeanAtmosphere') }}</span>
        <b>{{ report.mean_atmospheric_loss_db.toFixed(2) }} dB</b>
      </div>
      <div class="stat">
        <span>{{ t('linkMaxAtmosphere') }}</span>
        <b>{{ report.max_atmospheric_loss_db.toFixed(2) }} dB</b>
      </div>
      <div class="stat">
        <span>{{ t('linkSatellitePattern') }}</span>
        <b>{{ report.satellite_antenna_pattern === 'cosine' ? 'Cosine' : 'Omni' }}</b>
      </div>
    </div>

    <div v-if="chart" class="curve">
      <div class="curve-title">{{ t('linkMarginCurve') }}</div>
      <svg :viewBox="`0 0 ${chart.width} ${chart.height}`" preserveAspectRatio="none">
        <line x1="0" :y1="chart.zeroY" :x2="chart.width" :y2="chart.zeroY" class="zero" />
        <polyline :points="chart.points" class="margin" />
      </svg>
      <div class="curve-axis">
        <span>{{ chart.min.toFixed(0) }} dB</span>
        <span>{{ t('linkCurveMargin') }}</span>
        <span>{{ chart.max.toFixed(0) }} dB</span>
      </div>
    </div>

    <el-table :data="report.windows" size="small" height="260" class="table">
      <el-table-column prop="index" :label="t('windowIndex')" width="48" />
      <el-table-column :label="t('windowStart')" min-width="126">
        <template #default="{ row }">{{ formatUtcString(row.start_utc) }}</template>
      </el-table-column>
      <el-table-column :label="t('windowDuration')" width="82">
        <template #default="{ row }">{{ formatDuration(row.duration_seconds) }}</template>
      </el-table-column>
      <el-table-column :label="t('windowMaxElevation')" width="84">
        <template #default="{ row }">{{ row.max_elevation_deg.toFixed(1) }}°</template>
      </el-table-column>
      <el-table-column :label="t('linkMinMargin')" width="88">
        <template #default="{ row }">
          <span :class="row.min_margin_db >= 0 ? 'ok' : 'bad'">
            {{ row.min_margin_db.toFixed(2) }}
          </span>
        </template>
      </el-table-column>
      <el-table-column :label="t('linkMeanMargin')" width="88">
        <template #default="{ row }">{{ row.mean_margin_db.toFixed(2) }}</template>
      </el-table-column>
    </el-table>

    <div class="legend">
      <span class="dot ok" /> {{ t('linkLegendGood') }}
      <span class="dot warn" /> {{ t('linkLegendWarn') }}
      <span class="dot bad" /> {{ t('linkLegendBad') }}
    </div>

    <el-button size="small" :icon="Download" class="full" @click="onExportCsv">
      {{ t('exportCsv') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('noLinkResult')" :image-size="56" />

  <div class="section-title">{{ t('interferenceAnalysis') }}</div>
  <el-form label-position="top" size="small" class="form">
    <el-form-item :label="t('interferenceSources')">
      <el-select
        v-model="interferenceForm.interfererIds"
        multiple
        collapse-tags
        collapse-tags-tooltip
        :placeholder="t('interferenceSourcesPlaceholder')"
        style="width: 100%"
      >
        <el-option
          v-for="item in (store.current?.data.satellites ?? []).filter((satellite) => satellite.id !== form.satelliteId)"
          :key="item.id"
          :label="item.name"
          :value="item.id"
        />
      </el-select>
    </el-form-item>
    <el-form-item :label="t('interferenceThreshold')">
      <el-input-number v-model="interferenceForm.thresholdCiDb" :min="-50" :max="100" :step="1" controls-position="right" style="width: 100%" />
    </el-form-item>
  </el-form>

  <el-button type="primary" size="small" class="full" :loading="store.interferenceBusy" @click="onRunInterference">
    {{ store.interferenceBusy ? t('interferenceRunning') : t('interferenceRun') }}
  </el-button>

  <el-alert
    v-if="store.interferenceJob?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="store.interferenceJob.error ?? t('interferenceFailed')"
  />

  <template v-if="interference">
    <div class="section-title">{{ t('interferenceResult') }}</div>
    <div class="model-line">
      {{ interference.desired_satellite_name }} → {{ interference.ground_station_name }} ·
      {{ interference.interferers.length }} {{ t('interferenceSources') }}
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('interferenceMinCi') }}</span>
        <b :class="interference.min_ci_db >= interference.threshold_ci_db ? 'ok' : 'bad'">
          {{ interference.min_ci_db.toFixed(2) }} dB
        </b>
      </div>
      <div class="stat">
        <span>{{ t('interferenceMeanCi') }}</span>
        <b>{{ interference.mean_ci_db.toFixed(2) }} dB</b>
      </div>
      <div class="stat">
        <span>{{ t('interferenceDegraded') }}</span>
        <b>{{ (interference.degraded_ratio * 100).toFixed(1) }}%</b>
      </div>
    </div>
    <el-table :data="interference.interferers" size="small" height="150" class="table">
      <el-table-column prop="satellite_name" :label="t('accessSatellite')" min-width="110" />
      <el-table-column :label="t('interferenceMinCi')" width="92">
        <template #default="{ row }">{{ row.min_ci_db.toFixed(2) }}</template>
      </el-table-column>
      <el-table-column :label="t('interferenceMeanCi')" width="92">
        <template #default="{ row }">{{ row.mean_ci_db.toFixed(2) }}</template>
      </el-table-column>
      <el-table-column :label="t('interferenceSamples')" min-width="74">
        <template #default="{ row }">{{ row.visible_samples }}</template>
      </el-table-column>
    </el-table>
    <el-button size="small" :icon="Download" class="full" @click="onExportInterferenceCsv">
      {{ t('exportCsv') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('interferenceNoResult')" :image-size="56" />
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
    height: 96px;
    display: block;
  }

  .margin {
    fill: none;
    stroke: #4fc3f7;
    stroke-width: 1.6;
    vector-effect: non-scaling-stroke;
  }

  .zero {
    stroke: #ffb74d;
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
  margin: 6px 0 8px;
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

.dot.warn {
  background: #ffb74d;
}

.dot.bad {
  background: #ef5350;
}
</style>

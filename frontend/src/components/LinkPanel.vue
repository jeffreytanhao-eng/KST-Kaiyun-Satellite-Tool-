<script setup lang="ts">
import { computed, reactive, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Download } from '@element-plus/icons-vue'

import { downloadJobCsv, type LinkOptions } from '@/api/jobs'
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
  requiredCn0Dbm: 60
})

const report = computed(() => store.linkReport)
const job = computed(() => store.linkJob)

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
    required_cn0_dbhz: form.requiredCn0Dbm
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

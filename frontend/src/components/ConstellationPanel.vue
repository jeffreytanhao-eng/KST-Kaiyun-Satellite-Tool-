<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Download } from '@element-plus/icons-vue'

import {
  generateConstellation,
  type ConstellationGenerationReport,
  type ConstellationRequest
} from '@/api/constellations'
import { downloadJobCsv, type CoverageReport } from '@/api/jobs'
import type { ConstellationType } from '@/api/scenarios'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const emit = defineEmits<{
  (event: 'show-coverage', report: CoverageReport | null): void
}>()

const store = useScenarioStore()
const { t } = useI18n()

const form = reactive({
  name: 'KST-Walker',
  type: 'walker_delta' as ConstellationType,
  altitudeKm: 550,
  inclinationDeg: 53,
  totalSatellites: 24,
  planes: 6,
  phasing: 1,
  satellitesPerPlane: 4,
  colorByPlane: true
})

const preview = ref<ConstellationGenerationReport | null>(null)
const generating = ref(false)
const adding = ref(false)

const analysisForm = reactive({
  satelliteIds: [] as string[],
  stationIds: [] as string[],
  start: new Date(),
  end: new Date(Date.now() + 6 * 3600 * 1000),
  stepSeconds: 60,
  minLat: -60,
  maxLat: 60,
  minLon: -100,
  maxLon: 140,
  gridStep: 5,
  minElevation: 5
})

const analysis = computed(() => store.constellationAnalysis)

const topRevisitPoints = computed(() => {
  const points = analysis.value?.revisit_points ?? []
  return [...points]
    .filter((point) => point.covered)
    .sort((a, b) => b.access_count - a.access_count)
    .slice(0, 8)
})

function constellationTypeLabel(value: ConstellationType | string) {
  if (value === 'walker_star') return t('constellationWalkerStar')
  if (value === 'streets') return t('constellationStreets')
  return t('constellationWalkerDelta')
}

async function onGenerate() {
  if (!form.name.trim()) {
    ElMessage.warning(t('constellationNameRequired'))
    return
  }
  if (form.altitudeKm < 160 || form.altitudeKm > 50000) {
    ElMessage.warning(t('constellationAltitudeInvalid'))
    return
  }
  if (form.totalSatellites < 2 || form.totalSatellites > 100) {
    ElMessage.warning(t('constellationTotalInvalid'))
    return
  }
  if (form.planes < 1 || form.planes > form.totalSatellites) {
    ElMessage.warning(t('constellationPlanesInvalid'))
    return
  }
  if (form.type !== 'streets' && form.totalSatellites % form.planes !== 0) {
    ElMessage.warning(t('constellationDivisible'))
    return
  }
  if (form.type === 'streets' && form.satellitesPerPlane * form.planes !== form.totalSatellites) {
    ElMessage.warning(t('constellationStreetsInvalid'))
    return
  }
  const payload: ConstellationRequest = {
    name: form.name.trim(),
    constellation_type: form.type,
    altitude_km: form.altitudeKm,
    inclination_deg: form.inclinationDeg,
    total_satellites: form.totalSatellites,
    planes: form.planes,
    phasing: form.phasing,
    color_by_plane: form.colorByPlane
  }
  if (form.type === 'streets') payload.satellites_per_plane = form.satellitesPerPlane

  generating.value = true
  try {
    preview.value = await generateConstellation(payload)
    ElMessage.success(
      `${t('constellationGenerated')} · ${preview.value.total_satellites} ${t('constellationSatelliteCount')}`
    )
  } catch (error) {
    ElMessage.error(`${t('constellationGenerateFailed')}: ${errorText(error)}`)
  } finally {
    generating.value = false
  }
}

async function onAddToScenario() {
  if (!preview.value) return
  adding.value = true
  try {
    const added = await store.addConstellation(preview.value)
    ElMessage.success(t('constellationAdded').replace('{count}', String(added)))
  } catch (error) {
    ElMessage.error(`${t('constellationAddFailed')}: ${errorText(error)}`)
  } finally {
    adding.value = false
  }
}

async function onRunAnalysis() {
  if (!store.current) return
  if (analysisForm.minLat >= analysisForm.maxLat || analysisForm.minLon >= analysisForm.maxLon) {
    ElMessage.warning(t('coverageRegionInvalid'))
    return
  }
  emit('show-coverage', null)
  try {
    const result = await store.runConstellationAnalysis({
      satelliteIds: analysisForm.satelliteIds,
      stationIds: analysisForm.stationIds,
      start: analysisForm.start,
      end: analysisForm.end,
      stepSeconds: analysisForm.stepSeconds,
      region: {
        min_latitude_deg: analysisForm.minLat,
        max_latitude_deg: analysisForm.maxLat,
        min_longitude_deg: analysisForm.minLon,
        max_longitude_deg: analysisForm.maxLon
      },
      gridStepDeg: analysisForm.gridStep,
      minElevationDeg: analysisForm.minElevation
    })
    if (result) {
      emit('show-coverage', result.coverage)
      ElMessage.success(
        `${t('constellationResult')}: ${(result.coverage_ratio * 100).toFixed(1)}%`
      )
    }
  } catch (error) {
    ElMessage.error(`${t('constellationFailed')}: ${errorText(error)}`)
  }
}

async function onExportCsv() {
  const job = store.constellationAnalysisJob
  if (!job) return
  try {
    await downloadJobCsv(job.id, `constellation-${job.id.slice(0, 8)}.csv`)
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
</script>

<template>
  <div class="section-title">{{ t('constellationGenerate') }}</div>
  <el-form label-position="top" size="small" class="form">
    <el-form-item :label="t('constellationName')">
      <el-input v-model="form.name" maxlength="60" />
    </el-form-item>
    <el-form-item :label="t('constellationType')">
      <el-select v-model="form.type" style="width: 100%">
        <el-option :label="t('constellationWalkerDelta')" value="walker_delta" />
        <el-option :label="t('constellationWalkerStar')" value="walker_star" />
        <el-option :label="t('constellationStreets')" value="streets" />
      </el-select>
    </el-form-item>
    <div class="grid2">
      <el-form-item :label="t('constellationAltitude')">
        <el-input-number v-model="form.altitudeKm" :min="160" :max="50000" :step="50" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('constellationInclination')">
        <el-input-number v-model="form.inclinationDeg" :min="0" :max="180" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('constellationTotal')">
        <el-input-number v-model="form.totalSatellites" :min="2" :max="100" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('constellationPlanes')">
        <el-input-number v-model="form.planes" :min="1" :max="form.totalSatellites" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item v-if="form.type !== 'streets'" :label="t('constellationPhasing')">
        <el-input-number v-model="form.phasing" :min="0" :max="form.planes - 1" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item v-else :label="t('constellationPerPlane')">
        <el-input-number v-model="form.satellitesPerPlane" :min="1" :max="100" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
    </div>
    <el-form-item :label="t('constellationColorByPlane')">
      <el-switch v-model="form.colorByPlane" />
    </el-form-item>
  </el-form>

  <el-button type="primary" size="small" class="full" :loading="generating" @click="onGenerate">
    {{ generating ? t('constellationGenerating') : t('constellationGenerateBtn') }}
  </el-button>

  <template v-if="preview">
    <div class="section-title">{{ t('constellationPreview') }}</div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('constellationType') }}</span>
        <b>{{ constellationTypeLabel(preview.constellation_type) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('constellationSatelliteCount') }}</span>
        <b>{{ preview.total_satellites }}</b>
      </div>
      <div class="stat">
        <span>{{ t('constellationPlaneCount') }}</span>
        <b>{{ preview.planes }}</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('constellationPeriod') }}</span>
        <b>{{ preview.orbital_period_minutes.toFixed(1) }} min</b>
      </div>
      <div class="stat">
        <span>{{ t('constellationAltitude') }}</span>
        <b>{{ preview.altitude_km.toFixed(0) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('constellationInclination') }}</span>
        <b>{{ preview.inclination_deg.toFixed(1) }}°</b>
      </div>
    </div>
    <div class="hint-line">{{ preview.note }}</div>
    <el-button size="small" class="full" :loading="adding" @click="onAddToScenario">
      {{ t('constellationAdd') }}
    </el-button>
  </template>

  <div class="section-title">{{ t('constellationAnalysis') }}</div>
  <el-form label-position="top" size="small" class="form">
    <el-form-item :label="t('constellationSats')">
      <el-select
        v-model="analysisForm.satelliteIds"
        multiple
        collapse-tags
        collapse-tags-tooltip
        :placeholder="t('coverageAllSatellites')"
        style="width: 100%"
      >
        <el-option
          v-for="satellite in store.current?.data.satellites ?? []"
          :key="satellite.id"
          :label="satellite.name"
          :value="satellite.id"
        />
      </el-select>
    </el-form-item>
    <el-form-item :label="t('constellationStations')">
      <el-select
        v-model="analysisForm.stationIds"
        multiple
        collapse-tags
        collapse-tags-tooltip
        :placeholder="t('constellationAllStations')"
        style="width: 100%"
      >
        <el-option
          v-for="station in store.current?.data.ground_stations ?? []"
          :key="station.id"
          :label="station.name"
          :value="station.id"
        />
      </el-select>
    </el-form-item>
    <el-form-item :label="t('accessStart')">
      <el-date-picker v-model="analysisForm.start" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessEnd')">
      <el-date-picker v-model="analysisForm.end" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessStep')">
      <el-input-number v-model="analysisForm.stepSeconds" :min="1" :max="600" :step="10" controls-position="right" style="width: 100%" />
    </el-form-item>
    <div class="grid2">
      <el-form-item :label="t('coverageMinLat')">
        <el-input-number v-model="analysisForm.minLat" :min="-90" :max="89" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMaxLat')">
        <el-input-number v-model="analysisForm.maxLat" :min="-89" :max="90" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMinLon')">
        <el-input-number v-model="analysisForm.minLon" :min="-180" :max="179" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMaxLon')">
        <el-input-number v-model="analysisForm.maxLon" :min="-179" :max="180" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageGridStep')">
        <el-input-number v-model="analysisForm.gridStep" :min="0.5" :max="20" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMinElevation')">
        <el-input-number v-model="analysisForm.minElevation" :min="0" :max="90" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
    </div>
  </el-form>

  <el-button type="primary" size="small" class="full" :loading="store.constellationAnalysisBusy" @click="onRunAnalysis">
    {{ store.constellationAnalysisBusy ? t('constellationRunning') : t('constellationRun') }}
  </el-button>

  <el-alert
    v-if="store.constellationAnalysisJob?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="store.constellationAnalysisJob.error ?? t('constellationFailed')"
  />

  <template v-if="analysis">
    <div class="section-title">{{ t('constellationResult') }}</div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('coverageRatio') }}</span>
        <b>{{ (analysis.coverage_ratio * 100).toFixed(1) }}%</b>
      </div>
      <div class="stat">
        <span>{{ t('constellationMeanRevisit') }}</span>
        <b>{{ formatDuration(analysis.mean_revisit_seconds) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('constellationMaxGap') }}</span>
        <b>{{ formatDuration(analysis.max_gap_seconds) }}</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('coveredPoints') }}</span>
        <b>{{ analysis.covered_points }}/{{ analysis.total_points }}</b>
      </div>
      <div class="stat">
        <span>{{ t('constellationAccessCount') }}</span>
        <b>{{ analysis.mean_access_count.toFixed(2) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('coverageMultiCovered') }}</span>
        <b>{{ analysis.multi_covered_points }}</b>
      </div>
    </div>

    <div class="section-title">{{ t('constellationPerSatellite') }}</div>
    <el-table :data="analysis.coverage.satellite_stats" size="small" height="190" class="table">
      <el-table-column prop="satellite_name" :label="t('accessSatellite')" min-width="112" />
      <el-table-column :label="t('coverageRatio')" width="82">
        <template #default="{ row }">{{ (row.coverage_ratio * 100).toFixed(1) }}%</template>
      </el-table-column>
      <el-table-column :label="t('constellationStationTotal')" width="86">
        <template #default="{ row }">{{ formatDuration(row.total_visible_seconds) }}</template>
      </el-table-column>
      <el-table-column :label="t('constellationMaxWindow')" min-width="78">
        <template #default="{ row }">{{ formatDuration(row.max_visible_seconds) }}</template>
      </el-table-column>
    </el-table>

    <div v-if="analysis.station_stats.length > 0" class="section-title">
      {{ t('constellationStationStats') }}
    </div>
    <el-table
      v-if="analysis.station_stats.length > 0"
      :data="analysis.station_stats"
      size="small"
      height="170"
      class="table"
    >
      <el-table-column prop="station_name" :label="t('constellationStationName')" min-width="96" />
      <el-table-column :label="t('constellationStationWindows')" width="64">
        <template #default="{ row }">{{ row.window_count }}</template>
      </el-table-column>
      <el-table-column :label="t('constellationStationTotal')" width="82">
        <template #default="{ row }">{{ formatDuration(row.total_access_seconds) }}</template>
      </el-table-column>
      <el-table-column :label="t('constellationStationGap')" min-width="78">
        <template #default="{ row }">{{ formatDuration(row.max_gap_seconds) }}</template>
      </el-table-column>
    </el-table>

    <div class="section-title">{{ t('constellationRevisitTop') }}</div>
    <el-table :data="topRevisitPoints" size="small" height="180" class="table">
      <el-table-column :label="t('latitude')" width="70">
        <template #default="{ row }">{{ row.latitude_deg.toFixed(1) }}</template>
      </el-table-column>
      <el-table-column :label="t('longitude')" width="70">
        <template #default="{ row }">{{ row.longitude_deg.toFixed(1) }}</template>
      </el-table-column>
      <el-table-column :label="t('constellationAccessCount')" width="76">
        <template #default="{ row }">{{ row.access_count }}</template>
      </el-table-column>
      <el-table-column :label="t('constellationMeanRevisit')" width="86">
        <template #default="{ row }">{{ formatDuration(row.mean_gap_seconds) }}</template>
      </el-table-column>
      <el-table-column :label="t('constellationMaxGap')" min-width="78">
        <template #default="{ row }">{{ formatDuration(row.max_gap_seconds) }}</template>
      </el-table-column>
    </el-table>

    <div class="hint-line">{{ analysis.note }}</div>

    <el-button size="small" :icon="Download" class="full" @click="onExportCsv">
      {{ t('exportCsv') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('constellationNoResult')" :image-size="56" />
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

.hint-line {
  font-size: 10.5px;
  line-height: 1.5;
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
</style>

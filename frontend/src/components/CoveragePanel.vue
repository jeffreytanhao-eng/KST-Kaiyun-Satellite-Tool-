<script setup lang="ts">
import { computed, reactive } from 'vue'
import { ElMessage } from 'element-plus'
import { Download } from '@element-plus/icons-vue'

import { downloadJobCsv, type CoverageReport } from '@/api/jobs'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const emit = defineEmits<{
  (event: 'show-coverage', report: CoverageReport | null): void
}>()

const store = useScenarioStore()
const { t } = useI18n()

const form = reactive({
  satelliteIds: [] as string[],
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

const report = computed(() => store.coverageReport)

const topPoints = computed(() => {
  const points = report.value?.points ?? []
  return [...points]
    .filter((point) => point.covered)
    .sort((a, b) => b.visible_seconds - a.visible_seconds)
    .slice(0, 8)
})

async function onRun() {
  if (!store.current) return
  if (form.minLat >= form.maxLat || form.minLon >= form.maxLon) {
    ElMessage.warning(t('coverageRegionInvalid'))
    return
  }
  emit('show-coverage', null)
  try {
    const result = await store.runCoverage({
      satelliteIds: form.satelliteIds,
      start: form.start,
      end: form.end,
      stepSeconds: form.stepSeconds,
      region: {
        min_latitude_deg: form.minLat,
        max_latitude_deg: form.maxLat,
        min_longitude_deg: form.minLon,
        max_longitude_deg: form.maxLon
      },
      gridStepDeg: form.gridStep,
      minElevationDeg: form.minElevation
    })
    if (result) {
      emit('show-coverage', result)
      ElMessage.success(`${t('coverageResult')}: ${(result.coverage_ratio * 100).toFixed(1)}%`)
    }
  } catch (error) {
    ElMessage.error(`${t('coverageFailed')}: ${errorText(error)}`)
  }
}

async function onExportCsv() {
  const job = store.coverageJob
  if (!job) return
  try {
    await downloadJobCsv(job.id, `coverage-${job.id.slice(0, 8)}.csv`)
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
  const rest = total % 60
  if (hours > 0) return `${hours}h ${minutes}m`
  if (minutes > 0) return `${minutes}m ${rest}s`
  return `${rest}s`
}
</script>

<template>
  <div class="section-title">{{ t('coverageAnalysis') }}</div>
  <el-form label-position="top" size="small" class="form">
    <el-form-item :label="t('coverageSatellites')">
      <el-select
        v-model="form.satelliteIds"
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
    <el-form-item :label="t('accessStart')">
      <el-date-picker v-model="form.start" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessEnd')">
      <el-date-picker v-model="form.end" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessStep')">
      <el-input-number v-model="form.stepSeconds" :min="1" :max="600" :step="10" controls-position="right" style="width: 100%" />
    </el-form-item>

    <div class="grid2">
      <el-form-item :label="t('coverageMinLat')">
        <el-input-number v-model="form.minLat" :min="-90" :max="89" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMaxLat')">
        <el-input-number v-model="form.maxLat" :min="-89" :max="90" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMinLon')">
        <el-input-number v-model="form.minLon" :min="-180" :max="179" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMaxLon')">
        <el-input-number v-model="form.maxLon" :min="-179" :max="180" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageGridStep')">
        <el-input-number v-model="form.gridStep" :min="0.5" :max="20" :step="0.5" controls-position="right" style="width: 100%" />
      </el-form-item>
      <el-form-item :label="t('coverageMinElevation')">
        <el-input-number v-model="form.minElevation" :min="0" :max="90" :step="1" controls-position="right" style="width: 100%" />
      </el-form-item>
    </div>
  </el-form>

  <el-button type="primary" size="small" class="full" :loading="store.coverageBusy" @click="onRun">
    {{ store.coverageBusy ? t('computingCoverage') : t('computeCoverage') }}
  </el-button>

  <el-alert
    v-if="store.coverageJob?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="store.coverageJob.error ?? t('coverageFailed')"
  />

  <template v-if="report">
    <div class="section-title">{{ t('coverageResult') }}</div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('coverageRatio') }}</span>
        <b>{{ (report.coverage_ratio * 100).toFixed(1) }}%</b>
      </div>
      <div class="stat">
        <span>{{ t('coveredPoints') }}</span>
        <b>{{ report.covered_points }}/{{ report.total_points }}</b>
      </div>
      <div class="stat">
        <span>{{ t('avgVisible') }}</span>
        <b>{{ formatDuration(report.average_visible_seconds) }}</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('maxVisible') }}</span>
        <b>{{ formatDuration(report.max_visible_seconds) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('coverageGridStep') }}</span>
        <b>{{ report.grid_step_deg }}°</b>
      </div>
      <div class="stat">
        <span>{{ t('coverageMinElevation') }}</span>
        <b>{{ report.min_elevation_deg }}°</b>
      </div>
    </div>

    <el-table :data="topPoints" size="small" height="180" class="table">
      <el-table-column :label="t('latitude')" width="76">
        <template #default="{ row }">{{ row.latitude_deg.toFixed(1) }}</template>
      </el-table-column>
      <el-table-column :label="t('longitude')" width="76">
        <template #default="{ row }">{{ row.longitude_deg.toFixed(1) }}</template>
      </el-table-column>
      <el-table-column :label="t('avgVisible')" width="90">
        <template #default="{ row }">{{ formatDuration(row.visible_seconds) }}</template>
      </el-table-column>
      <el-table-column :label="t('windowCount')" width="64">
        <template #default="{ row }">{{ row.access_count }}</template>
      </el-table-column>
      <el-table-column :label="t('windowMaxElevation')" min-width="72">
        <template #default="{ row }">{{ row.max_elevation_deg.toFixed(0) }}°</template>
      </el-table-column>
    </el-table>

    <el-button size="small" :icon="Download" class="full" @click="onExportCsv">
      {{ t('exportCsv') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('noCoverageResult')" :image-size="56" />
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

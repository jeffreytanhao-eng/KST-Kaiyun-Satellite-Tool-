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
  end: new Date(Date.now() + 6 * 3600 * 1000),
  stepSeconds: 60
})

const report = computed(() => store.compareReport)
const job = computed(() => store.compareJob)

const satellite = computed(() =>
  store.current?.data.satellites.find((item) => item.id === form.satelliteId)
)

const propagatorLabel = computed(() =>
  satellite.value?.propagator_type === 'numerical' ? t('propagatorNumerical') : t('propagatorSgp4')
)

function syncSatellite() {
  const satellites = store.current?.data.satellites ?? []
  if (!satellites.some((item) => item.id === form.satelliteId)) {
    form.satelliteId =
      satellites.find((item) => item.enabled)?.id ?? satellites[0]?.id ?? ''
  }
}

watch(() => store.current, syncSatellite, { immediate: true })

async function onRun() {
  if (!form.satelliteId) {
    ElMessage.warning(t('noSatellites'))
    return
  }
  try {
    const result = await store.runComparison({
      satelliteId: form.satelliteId,
      start: form.start,
      end: form.end,
      stepSeconds: form.stepSeconds
    })
    if (result) {
      ElMessage.success(t('compareResult'))
    }
  } catch (error) {
    ElMessage.error(`${t('compareFailed')}: ${errorText(error)}`)
  }
}

async function onExportCsv() {
  const current = job.value
  if (!current) return
  try {
    await downloadJobCsv(current.id, `propagation-compare-${current.id.slice(0, 8)}.csv`)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function seconds(value: number) {
  return `${value.toFixed(1)} s`
}
</script>

<template>
  <div class="section-title">{{ t('compareTitle') }}</div>
  <div class="hint-line">{{ t('compareHint') }}</div>
  <el-form label-position="top" size="small" class="form">
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
    <div class="current-propagator">
      <span>{{ t('currentPropagator') }}</span>
      <el-tag size="small" effect="plain">{{ propagatorLabel }}</el-tag>
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
        :min="10"
        :max="600"
        :step="10"
        controls-position="right"
        style="width: 100%"
      />
    </el-form-item>
  </el-form>

  <el-button
    type="primary"
    size="small"
    class="full"
    :loading="store.compareBusy"
    @click="onRun"
  >
    {{ store.compareBusy ? t('comparing') : t('runCompare') }}
  </el-button>

  <el-alert
    v-if="job?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="job.error ?? t('compareFailed')"
  />

  <template v-if="report">
    <div class="section-title">{{ t('compareResult') }}</div>
    <div class="model-line">{{ report.numerical_model }}</div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('compareMeanPosition') }}</span>
        <b>{{ report.statistics.mean_position_difference_km.toFixed(3) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('compareMaxPosition') }}</span>
        <b>{{ report.statistics.max_position_difference_km.toFixed(3) }} km</b>
      </div>
      <div class="stat">
        <span>{{ t('compareRmsPosition') }}</span>
        <b>{{ report.statistics.rms_position_difference_km.toFixed(3) }} km</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('compareMeanVelocity') }}</span>
        <b>{{ report.statistics.mean_velocity_difference_km_s.toFixed(6) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('compareMaxVelocity') }}</span>
        <b>{{ report.statistics.max_velocity_difference_km_s.toFixed(6) }}</b>
      </div>
      <div class="stat">
        <span>{{ t('compareRmsVelocity') }}</span>
        <b>{{ report.statistics.rms_velocity_difference_km_s.toFixed(6) }}</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('compareAcceptedSteps') }}</span>
        <b>{{ report.integration.accepted_steps }}</b>
      </div>
      <div class="stat">
        <span>{{ t('compareRejectedSteps') }}</span>
        <b>{{ report.integration.rejected_steps }}</b>
      </div>
      <div class="stat">
        <span>{{ t('compareMeanStep') }}</span>
        <b>{{ seconds(report.integration.mean_step_seconds) }}</b>
      </div>
    </div>
    <div class="model-line">
      J2: {{ report.j2_enabled ? t('enabledState') : t('disabledState') }} · {{ t('drag') }}:
      {{ report.drag_enabled ? t('enabledState') : t('disabledState') }}
    </div>

    <el-table :data="report.samples" size="small" height="220" class="table">
      <el-table-column :label="t('compareTime')" width="80">
        <template #default="{ row }">{{ row.t_seconds.toFixed(0) }}</template>
      </el-table-column>
      <el-table-column :label="t('comparePositionDiff')" min-width="110">
        <template #default="{ row }">
          {{ row.position_difference_km.toFixed(3) }}
        </template>
      </el-table-column>
      <el-table-column :label="t('compareVelocityDiff')" min-width="110">
        <template #default="{ row }">
          {{ row.velocity_difference_km_s.toFixed(6) }}
        </template>
      </el-table-column>
    </el-table>

    <el-button size="small" :icon="Download" class="full" @click="onExportCsv">
      {{ t('exportCsv') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('noCompareResult')" :image-size="56" />
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

.current-propagator {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  font-size: 11px;
  color: var(--kst-text-dim);
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
</style>

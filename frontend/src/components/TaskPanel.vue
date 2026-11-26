<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Refresh } from '@element-plus/icons-vue'

import {
  cancelJob,
  downloadJobCsv,
  downloadJobReport,
  listJobs,
  type JobKind,
  type JobStatus,
  type JobSummary
} from '@/api/jobs'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const props = defineProps<{ active?: boolean }>()

const store = useScenarioStore()
const { t } = useI18n()

const jobs = ref<JobSummary[]>([])
const loading = ref(false)
const scope = ref<'scenario' | 'all'>('scenario')
const cancelling = ref<string | null>(null)
let timer: number | null = null

const scenarioId = computed(() => store.current?.id ?? '')

const kindLabels: Record<JobKind, string> = {
  propagation: 'jobKindPropagation',
  access: 'jobKindAccess',
  coverage: 'jobKindCoverage',
  environment: 'jobKindEnvironment',
  propagation_compare: 'jobKindCompare',
  link_budget: 'jobKindLink',
  constellation_analysis: 'jobKindConstellation',
  radar: 'jobKindRadar',
  report: 'jobKindReport',
  interference: 'jobKindInterference'
}

const statusMeta: Record<JobStatus, { type: 'info' | 'warning' | 'success' | 'danger'; key: string }> = {
  queued: { type: 'info', key: 'taskStatusQueued' },
  running: { type: 'warning', key: 'taskStatusRunning' },
  succeeded: { type: 'success', key: 'taskStatusSucceeded' },
  failed: { type: 'danger', key: 'taskStatusFailed' },
  cancelled: { type: 'info', key: 'taskStatusCancelled' }
}

const csvKinds = new Set<JobKind>([
  'propagation',
  'access',
  'coverage',
  'environment',
  'propagation_compare',
  'link_budget',
  'constellation_analysis',
  'radar',
  'interference'
])

function kindLabel(kind: JobKind) {
  const key = kindLabels[kind]
  return key ? t(key as Parameters<typeof t>[0]) : kind
}

function statusLabel(status: JobStatus) {
  const meta = statusMeta[status]
  return meta ? t(meta.key as Parameters<typeof t>[0]) : status
}

function statusType(status: JobStatus) {
  return statusMeta[status]?.type ?? 'info'
}

function duration(job: JobSummary) {
  if (!job.started_at) return '—'
  const end = job.finished_at ? new Date(job.finished_at).getTime() : Date.now()
  const start = new Date(job.started_at).getTime()
  const seconds = Math.max(0, (end - start) / 1000)
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`
  const minutes = Math.floor(seconds / 60)
  const rest = Math.round(seconds % 60)
  return `${minutes}m ${rest}s`
}

function displayProgress(job: JobSummary) {
  return job.status === 'succeeded' ? 100 : Math.min(100, Math.max(0, job.progress))
}

function progressStatus(job: JobSummary) {
  if (job.status === 'failed') return 'exception'
  if (job.status === 'succeeded') return 'success'
  if (job.status === 'cancelled') return 'warning'
  return undefined
}

async function load() {
  loading.value = true
  try {
    jobs.value = await listJobs({
      scenario_id: scope.value === 'scenario' && scenarioId.value ? scenarioId.value : undefined,
      limit: 100
    })
  } catch (error) {
    ElMessage.error(`${t('taskLoadFailed')}: ${errorText(error)}`)
  } finally {
    loading.value = false
  }
}

async function onCancel(job: JobSummary) {
  try {
    await ElMessageBox.confirm(t('taskCancelConfirm'), t('taskCancel'), {
      type: 'warning'
    })
  } catch {
    return
  }
  cancelling.value = job.id
  try {
    const updated = await cancelJob(job.id)
    const index = jobs.value.findIndex((item) => item.id === job.id)
    if (index >= 0) jobs.value[index] = updated
    ElMessage.success(t('taskCancelDone'))
  } catch (error) {
    ElMessage.error(`${t('taskCancelFailed')}: ${errorText(error)}`)
  } finally {
    cancelling.value = null
  }
}

async function onCsv(job: JobSummary) {
  try {
    await downloadJobCsv(job.id, `${job.kind}-${job.id.slice(0, 8)}.csv`)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

async function onReport(job: JobSummary) {
  try {
    await downloadJobReport(job.id, `${job.kind}-${job.id.slice(0, 8)}.html`, 'html')
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

onMounted(() => {
  void load()
  timer = window.setInterval(() => {
    if (!loading.value && props.active !== false) void load()
  }, 3000)
})

onUnmounted(() => {
  if (timer !== null) window.clearInterval(timer)
})

watch(
  () => props.active,
  (active) => {
    if (active) void load()
  }
)

watch(scope, load)
watch(scenarioId, load)
</script>

<template>
  <div class="section-title task-title">
    <span>{{ t('taskTitle') }}</span>
    <div class="task-tools">
      <el-radio-group v-model="scope" size="small">
        <el-radio-button value="scenario">{{ t('taskScenarioOnly') }}</el-radio-button>
        <el-radio-button value="all">{{ t('taskAll') }}</el-radio-button>
      </el-radio-group>
      <el-button link size="small" :loading="loading" :icon="Refresh" @click="load">
        {{ t('refresh') }}
      </el-button>
    </div>
  </div>

  <div v-if="jobs.length === 0 && !loading" class="hint">{{ t('taskEmpty') }}</div>

  <div v-else class="task-list">
    <div v-for="job in jobs" :key="job.id" class="task-item">
      <div class="task-head">
        <span class="task-kind">{{ kindLabel(job.kind) }}</span>
        <el-tag size="small" :type="statusType(job.status)" effect="plain">
          {{ statusLabel(job.status) }}
        </el-tag>
        <el-tag v-if="job.cached" size="small" type="success" effect="dark" :title="t('taskCachedHint')">
          {{ t('taskCached') }}
        </el-tag>
        <span class="task-time">{{ new Date(job.created_at).toLocaleString() }}</span>
      </div>

      <div class="task-progress">
        <el-progress
          :percentage="displayProgress(job)"
          :status="progressStatus(job)"
          :stroke-width="6"
          :show-text="false"
        />
        <span class="task-percent">{{ displayProgress(job) }}%</span>
      </div>

      <div class="task-meta">
        <span>{{ t('taskDuration') }}: {{ duration(job) }}</span>
        <span v-if="job.error" class="task-error">{{ job.error }}</span>
      </div>

      <div class="task-actions">
        <el-button
          v-if="job.status === 'queued' || job.status === 'running'"
          size="small"
          type="danger"
          plain
          :loading="cancelling === job.id"
          @click="onCancel(job)"
        >
          {{ t('taskCancel') }}
        </el-button>
        <el-button
          v-if="job.status === 'succeeded' && csvKinds.has(job.kind)"
          size="small"
          plain
          @click="onCsv(job)"
        >
          {{ t('taskDownloadCsv') }}
        </el-button>
        <el-button
          v-if="job.status === 'succeeded'"
          size="small"
          plain
          @click="onReport(job)"
        >
          {{ t('taskDownloadReport') }}
        </el-button>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.section-title {
  margin: 0 0 10px;
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--kst-accent-strong);
}

.task-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.task-tools {
  display: flex;
  align-items: center;
  gap: 8px;

  :deep(.el-radio-button__inner) {
    padding: 4px 8px;
    font-size: 10px;
  }
}

.hint {
  margin-top: 6px;
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--kst-text-dim);
}

.task-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.task-item {
  padding: 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.55);
}

.task-head {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.task-kind {
  font-size: 11.5px;
  font-weight: 600;
}

.task-time {
  margin-left: auto;
  font-size: 10px;
  color: var(--kst-text-dim);
}

.task-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;

  :deep(.el-progress) {
    flex: 1;
  }
}

.task-percent {
  font-size: 10px;
  color: var(--kst-text-dim);
  min-width: 30px;
  text-align: right;
}

.task-meta {
  display: flex;
  gap: 8px;
  margin-top: 6px;
  font-size: 10px;
  color: var(--kst-text-dim);
  flex-wrap: wrap;
}

.task-error {
  color: var(--el-color-danger);
}

.task-actions {
  display: flex;
  gap: 6px;
  margin-top: 8px;
  flex-wrap: wrap;

  :deep(.el-button) {
    margin-left: 0;
  }
}
</style>

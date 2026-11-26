<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { Download, MagicStick, Printer } from '@element-plus/icons-vue'

import { polishReport } from '@/api/ai'
import { downloadJobReport, type ReportRequest } from '@/api/jobs'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

const store = useScenarioStore()
const { t, locale } = useI18n()

const form = reactive({
  title: '',
  start: new Date(),
  end: new Date(Date.now() + 6 * 3600 * 1000),
  stepSeconds: 60,
  includeAccess: true,
  includeCoverage: true,
  includeLink: true,
  includeRadar: true,
  minLat: -60,
  maxLat: 60,
  minLon: -100,
  maxLon: 140,
  gridStep: 5,
  minElevation: 5
})

const view = ref<'html' | 'markdown'>('html')
const polished = ref('')
const polishing = ref(false)

const report = computed(() => store.reportDocument)

async function onRun() {
  if (!store.current) return
  if (form.minLat >= form.maxLat || form.minLon >= form.maxLon) {
    ElMessage.warning(t('coverageRegionInvalid'))
    return
  }
  if (!form.includeAccess && !form.includeCoverage && !form.includeLink && !form.includeRadar) {
    ElMessage.warning(t('reportIncludeRequired'))
    return
  }
  const payload: ReportRequest = {
    title: form.title.trim() || undefined,
    start: form.start.toISOString(),
    end: form.end.toISOString(),
    step_seconds: form.stepSeconds,
    region: {
      min_latitude_deg: form.minLat,
      max_latitude_deg: form.maxLat,
      min_longitude_deg: form.minLon,
      max_longitude_deg: form.maxLon
    },
    grid_step_deg: form.gridStep,
    min_elevation_deg: form.minElevation,
    include_access: form.includeAccess,
    include_coverage: form.includeCoverage,
    include_link: form.includeLink,
    include_radar: form.includeRadar
  }
  polished.value = ''
  try {
    await store.runReport(payload)
    ElMessage.success(t('reportReady'))
  } catch (error) {
    ElMessage.error(`${t('reportFailed')}: ${errorText(error)}`)
  }
}

async function onDownload(format: 'html' | 'markdown') {
  const job = store.reportJob
  if (!job) return
  try {
    const extension = format === 'html' ? 'html' : 'md'
    await downloadJobReport(job.id, `kst-report-${job.id.slice(0, 8)}.${extension}`, format)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function onPrint() {
  const document_ = report.value
  if (!document_) return
  const win = window.open('', '_blank', 'noopener,noreferrer,width=1100,height=800')
  if (!win) {
    ElMessage.warning(t('reportPrintBlocked'))
    return
  }
  win.document.open()
  win.document.write(document_.html)
  win.document.close()
  win.focus()
  window.setTimeout(() => win.print(), 350)
}

async function onPolish() {
  const document_ = report.value
  if (!document_) return
  polishing.value = true
  try {
    const response = await polishReport(document_.markdown, locale.value)
    polished.value = response.content
    ElMessage.success(t('reportPolishApplied'))
  } catch (error) {
    ElMessage.error(`${t('reportPolishFailed')}: ${errorText(error)}`)
  } finally {
    polishing.value = false
  }
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatUtcString(value: string) {
  return value.replace('T', ' ').replace(/\.\d+Z$/, 'Z')
}
</script>

<template>
  <div class="section-title">{{ t('reportGenerate') }}</div>
  <el-form label-position="top" size="small" class="form">
    <el-form-item :label="t('reportTitle')">
      <el-input v-model="form.title" :placeholder="t('reportTitlePlaceholder')" maxlength="80" />
    </el-form-item>
    <el-form-item :label="t('accessStart')">
      <el-date-picker v-model="form.start" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessEnd')">
      <el-date-picker v-model="form.end" type="datetime" :clearable="false" style="width: 100%" />
    </el-form-item>
    <el-form-item :label="t('accessStep')">
      <el-input-number v-model="form.stepSeconds" :min="10" :max="600" :step="10" controls-position="right" style="width: 100%" />
    </el-form-item>

    <div class="section-title inner">{{ t('reportSections') }}</div>
    <div class="checks">
      <el-checkbox v-model="form.includeAccess">{{ t('reportIncludeAccess') }}</el-checkbox>
      <el-checkbox v-model="form.includeCoverage">{{ t('reportIncludeCoverage') }}</el-checkbox>
      <el-checkbox v-model="form.includeLink">{{ t('reportIncludeLink') }}</el-checkbox>
      <el-checkbox v-model="form.includeRadar">{{ t('reportIncludeRadar') }}</el-checkbox>
    </div>

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

  <el-button type="primary" size="small" class="full" :loading="store.reportBusy" @click="onRun">
    {{ store.reportBusy ? t('reportBuilding') : t('reportBuild') }}
  </el-button>

  <el-alert
    v-if="store.reportJob?.status === 'failed'"
    type="error"
    :closable="false"
    class="job-error"
    :title="store.reportJob.error ?? t('reportFailed')"
  />

  <template v-if="report">
    <div class="section-title">{{ t('reportStats') }}</div>
    <div class="report-meta">{{ report.title }} · {{ formatUtcString(report.generated_at) }}</div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('reportSatellites') }}</span>
        <b>{{ report.stats.satellite_count }}</b>
      </div>
      <div class="stat">
        <span>{{ t('reportStations') }}</span>
        <b>{{ report.stats.station_count }}</b>
      </div>
      <div class="stat">
        <span>{{ t('reportAccessWindows') }}</span>
        <b>{{ report.stats.access_window_count }}</b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('reportCoverage') }}</span>
        <b>
          {{ report.stats.coverage_ratio == null ? '—' : `${(report.stats.coverage_ratio * 100).toFixed(1)}%` }}
        </b>
      </div>
      <div class="stat">
        <span>{{ t('reportMaxGap') }}</span>
        <b>
          {{ report.stats.max_gap_seconds == null ? '—' : `${(report.stats.max_gap_seconds / 60).toFixed(1)} min` }}
        </b>
      </div>
      <div class="stat">
        <span>{{ t('reportMinLink') }}</span>
        <b>
          {{ report.stats.min_link_margin_db == null ? '—' : `${report.stats.min_link_margin_db.toFixed(2)} dB` }}
        </b>
      </div>
    </div>
    <div class="stats">
      <div class="stat">
        <span>{{ t('reportAccessPairs') }}</span>
        <b>{{ report.stats.access_pair_count }}</b>
      </div>
      <div class="stat wide">
        <span>{{ t('reportRadarRange') }}</span>
        <b>
          {{ report.stats.radar_max_detection_range_km == null ? '—' : `${report.stats.radar_max_detection_range_km.toFixed(1)} km` }}
        </b>
      </div>
    </div>

    <el-radio-group v-model="view" size="small" class="view-switch">
      <el-radio-button value="html">{{ t('reportHtmlView') }}</el-radio-button>
      <el-radio-button value="markdown">{{ t('reportMarkdownView') }}</el-radio-button>
    </el-radio-group>

    <iframe
      v-if="view === 'html'"
      :srcdoc="report.html"
      sandbox="allow-same-origin"
      class="report-frame"
      :title="report.title"
    />
    <pre v-else class="markdown">{{ report.markdown }}</pre>

    <div class="actions">
      <el-button size="small" :icon="Download" @click="onDownload('html')">
        {{ t('reportDownloadHtml') }}
      </el-button>
      <el-button size="small" :icon="Download" @click="onDownload('markdown')">
        {{ t('reportDownloadMd') }}
      </el-button>
      <el-button size="small" :icon="Printer" @click="onPrint">
        {{ t('reportPrintPdf') }}
      </el-button>
      <el-button size="small" :icon="MagicStick" :loading="polishing" @click="onPolish">
        {{ t('reportAiPolish') }}
      </el-button>
    </div>

    <template v-if="polished">
      <div class="section-title">{{ t('reportPolished') }}</div>
      <pre class="markdown">{{ polished }}</pre>
    </template>
  </template>
  <el-empty v-else :description="t('reportNoResult')" :image-size="56" />
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

  &.inner {
    margin-top: 4px;
  }
}

.full {
  width: 100%;
  margin-top: 4px;
}

.job-error {
  margin-top: 8px;
}

.checks {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0 8px;
  margin-bottom: 6px;

  :deep(.el-checkbox) {
    height: 22px;
  }
}

.report-meta {
  font-size: 10.5px;
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

  &.wide {
    grid-column: span 2;
  }

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

.view-switch {
  margin-bottom: 8px;
}

.report-frame {
  width: 100%;
  height: 360px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: #0b1526;
  margin-bottom: 8px;
}

.markdown {
  max-height: 320px;
  overflow: auto;
  padding: 8px;
  margin: 0 0 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(6, 12, 22, 0.5);
  color: var(--kst-text);
  font-size: 10.5px;
  line-height: 1.55;
  white-space: pre-wrap;
  word-break: break-word;
}

.actions {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;

  :deep(.el-button) {
    width: 100%;
    margin-left: 0;
  }
}
</style>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'
import { Download, MagicStick } from '@element-plus/icons-vue'

import {
  generateJobReport,
  getAiConfig,
  interpretJob,
  recommendScenario,
  type AiConfig,
  type AiResponse
} from '@/api/ai'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'

type Action = 'interpret' | 'recommend' | 'report'

const store = useScenarioStore()
const { t, locale } = useI18n()

const config = ref<AiConfig | null>(null)
const configLoading = ref(false)
const busy = ref<Action | null>(null)
const objective = ref('')
const selectedJobKey = ref<'access' | 'coverage' | 'environment'>('access')
const output = ref<AiResponse | null>(null)
const lastAction = ref<Action>('interpret')

const presets = computed(() => {
  const english = locale.value === 'en'
  return (config.value?.presets ?? []).map((preset) => ({
    id: preset.id,
    name: english ? preset.name_en || preset.name : preset.name,
    description: english ? preset.description_en || preset.description : preset.description
  }))
})

const jobOptions = computed<{ key: 'access' | 'coverage' | 'environment'; label: string; id: string }[]>(() => {
  const options: { key: 'access' | 'coverage' | 'environment'; label: string; id: string }[] = []
  if (store.accessJob?.status === 'succeeded') {
    options.push({ key: 'access', label: t('aiJobAccess'), id: store.accessJob.id })
  }
  if (store.coverageJob?.status === 'succeeded') {
    options.push({ key: 'coverage', label: t('aiJobCoverage'), id: store.coverageJob.id })
  }
  if (store.environmentJob?.status === 'succeeded') {
    options.push({ key: 'environment', label: t('aiJobEnvironment'), id: store.environmentJob.id })
  }
  return options
})

const selectedJob = computed(
  () => jobOptions.value.find((item) => item.key === selectedJobKey.value) ?? null
)

const enabled = computed(() => config.value?.enabled === true)

watch(
  jobOptions,
  (options) => {
    if (options.length === 0) return
    if (!options.some((item) => item.key === selectedJobKey.value)) {
      selectedJobKey.value = options[0].key
    }
  },
  { immediate: true }
)

onMounted(loadConfig)

async function loadConfig() {
  configLoading.value = true
  try {
    config.value = await getAiConfig()
  } catch (error) {
    ElMessage.error(`${t('aiConfigFailed')}: ${errorText(error)}`)
  } finally {
    configLoading.value = false
  }
}

async function onInterpret() {
  const job = selectedJob.value
  if (!job || busy.value) return
  busy.value = 'interpret'
  lastAction.value = 'interpret'
  try {
    output.value = await interpretJob(job.id, locale.value)
    ElMessage.success(t('aiResult'))
  } catch (error) {
    ElMessage.error(`${t('aiFailed')}: ${errorText(error)}`)
  } finally {
    busy.value = null
  }
}

async function onReport() {
  const job = selectedJob.value
  if (!job || busy.value) return
  busy.value = 'report'
  lastAction.value = 'report'
  try {
    output.value = await generateJobReport(job.id, locale.value)
    ElMessage.success(t('aiResult'))
  } catch (error) {
    ElMessage.error(`${t('aiFailed')}: ${errorText(error)}`)
  } finally {
    busy.value = null
  }
}

async function onRecommend() {
  if (!store.current || busy.value) return
  const text = objective.value.trim()
  if (!text) {
    ElMessage.warning(t('aiObjectiveRequired'))
    return
  }
  busy.value = 'recommend'
  lastAction.value = 'recommend'
  try {
    output.value = await recommendScenario(store.current.id, text, locale.value)
    ElMessage.success(t('aiResult'))
  } catch (error) {
    ElMessage.error(`${t('aiFailed')}: ${errorText(error)}`)
  } finally {
    busy.value = null
  }
}

function downloadMarkdown() {
  if (!output.value) return
  const blob = new Blob([output.value.content], { type: 'text/markdown;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = `ai-${lastAction.value}-${output.value.request_id.slice(0, 8)}.md`
  anchor.click()
  URL.revokeObjectURL(url)
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}
</script>

<template>
  <div class="section-title ai-title">
    <span>{{ t('aiAssistant') }}</span>
    <el-button link size="small" :loading="configLoading" @click="loadConfig">
      {{ t('refresh') }}
    </el-button>
  </div>

  <div class="status">
    <el-tag size="small" :type="enabled ? 'success' : 'info'" effect="plain">
      {{ enabled ? t('aiEnabled') : t('aiDisabled') }}
    </el-tag>
    <span v-if="config?.model" class="model">{{ t('aiModel') }}: {{ config.model }}</span>
  </div>
  <div v-if="!enabled && !configLoading" class="hint">{{ t('aiDisabledHint') }}</div>

  <div class="section-title">{{ t('aiPresets') }}</div>
  <div class="presets">
    <div v-for="preset in presets" :key="preset.id" class="preset">
      <div class="preset-name">{{ preset.name }}</div>
      <div class="preset-desc">{{ preset.description }}</div>
    </div>
    <div v-if="presets.length === 0" class="hint">{{ t('aiNoPresets') }}</div>
  </div>

  <div class="section-title">{{ t('aiJobSource') }}</div>
  <el-select
    v-if="jobOptions.length > 0"
    v-model="selectedJobKey"
    size="small"
    style="width: 100%"
  >
    <el-option v-for="item in jobOptions" :key="item.key" :label="item.label" :value="item.key" />
  </el-select>
  <div v-else class="hint">{{ t('aiNoJob') }}</div>

  <div class="actions">
    <el-button
      size="small"
      type="primary"
      :icon="MagicStick"
      :disabled="!enabled || !selectedJob"
      :loading="busy === 'interpret'"
      @click="onInterpret"
    >
      {{ t('aiInterpret') }}
    </el-button>
    <el-button
      size="small"
      :disabled="!enabled || !selectedJob"
      :loading="busy === 'report'"
      @click="onReport"
    >
      {{ t('aiReport') }}
    </el-button>
  </div>

  <div class="section-title">{{ t('aiRecommend') }}</div>
  <el-input
    v-model="objective"
    type="textarea"
    :rows="3"
    maxlength="500"
    show-word-limit
    :placeholder="t('aiObjectivePlaceholder')"
  />
  <el-button
    size="small"
    class="full"
    :icon="MagicStick"
    :disabled="!enabled || !store.current"
    :loading="busy === 'recommend'"
    @click="onRecommend"
  >
    {{ t('aiRecommend') }}
  </el-button>

  <div class="section-title">{{ t('aiResult') }}</div>
  <template v-if="output">
    <div class="meta">
      <span>{{ output.model }}</span>
      <span>{{ t('aiLatency') }}: {{ output.latency_ms }} ms</span>
    </div>
    <pre class="output">{{ output.content }}</pre>
    <div class="disclaimer">{{ t('aiDisclaimer') }}</div>
    <el-button size="small" :icon="Download" class="full" @click="downloadMarkdown">
      {{ t('aiDownloadMd') }}
    </el-button>
  </template>
  <el-empty v-else :description="t('aiEmpty')" :image-size="56" />
</template>

<style scoped lang="scss">
.section-title {
  margin: 12px 0 8px;
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--kst-accent-strong);
}

.ai-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.status {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.model {
  font-size: 10.5px;
  color: var(--kst-text-dim);
  word-break: break-all;
}

.hint {
  margin-top: 6px;
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--kst-text-dim);
}

.presets {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.preset {
  padding: 7px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(79, 195, 247, 0.04);
}

.preset-name {
  font-size: 11.5px;
  font-weight: 600;
  margin-bottom: 2px;
}

.preset-desc {
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--kst-text-dim);
}

.actions {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
  margin-top: 8px;

  :deep(.el-button) {
    margin-left: 0;
    width: 100%;
  }
}

.full {
  width: 100%;
  margin-top: 8px;
}

.meta {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 10px;
  color: var(--kst-text-dim);
  margin-bottom: 6px;
}

.output {
  margin: 0;
  padding: 8px;
  max-height: 240px;
  overflow: auto;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.7);
  font-family: 'JetBrains Mono', Consolas, 'Cascadia Mono', monospace;
  font-size: 10.5px;
  line-height: 1.55;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--kst-text);
}

.disclaimer {
  margin-top: 6px;
  font-size: 10px;
  color: var(--kst-text-dim);
}
</style>

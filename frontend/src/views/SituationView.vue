<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  Aim,
  Delete,
  Download,
  Edit,
  Plus,
  Refresh,
  Upload,
  VideoPause,
  VideoPlay
} from '@element-plus/icons-vue'

import LangSwitch from '@/components/LangSwitch.vue'
import ObjectTree from '@/components/ObjectTree.vue'
import { useCesiumViewer } from '@/composables/useCesiumViewer'
import { useAuthStore } from '@/stores/auth'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'
import type { GroundStationObject, SatelliteObject, ScenarioData } from '@/api/scenarios'
import { downloadAccessCsv } from '@/api/jobs'

type ObjectType = 'satellite' | 'station'
interface Selection {
  type: ObjectType
  id: string
}

const router = useRouter()
const auth = useAuthStore()
const store = useScenarioStore()
const { t } = useI18n()

const viewerHost = ref<HTMLDivElement | null>(null)
const cesium = useCesiumViewer(viewerHost)

const scenario = computed(() => store.current)
const playback = computed(() => store.propagation)
const accessReport = computed(() => store.accessReport)
const accessJob = computed(() => store.accessJob)

const selection = ref<Selection | null>(null)
const activeTab = ref<'props' | 'access'>('props')
const importInput = ref<HTMLInputElement | null>(null)

const scenarioId = computed({
  get: () => store.current?.id ?? '',
  set: (value: string) => {
    if (value && value !== store.current?.id) {
      void selectScenario(value)
    }
  }
})

async function selectScenario(id: string) {
  try {
    await store.select(id)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

/* ------------------------------ 场景管理 ------------------------------ */

const scenarioDialog = reactive({
  visible: false,
  mode: 'create' as 'create' | 'edit',
  name: '',
  description: ''
})

function openScenarioDialog(mode: 'create' | 'edit') {
  if (mode === 'edit' && !scenario.value) return
  scenarioDialog.mode = mode
  scenarioDialog.name = mode === 'edit' ? scenario.value?.name ?? '' : ''
  scenarioDialog.description = mode === 'edit' ? scenario.value?.description ?? '' : ''
  scenarioDialog.visible = true
}

async function submitScenarioDialog() {
  const name = scenarioDialog.name.trim()
  if (!name) {
    ElMessage.warning(t('scenarioNameRequired'))
    return
  }
  try {
    if (scenarioDialog.mode === 'create') {
      await store.create(name, scenarioDialog.description.trim())
      ElMessage.success(t('scenarioSaved'))
    } else {
      await store.save({ name, description: scenarioDialog.description.trim() })
      ElMessage.success(t('scenarioSaved'))
    }
    scenarioDialog.visible = false
  } catch (error) {
    ElMessage.error(`${t('scenarioSaved')}: ${errorText(error)}`)
  }
}

async function onDemoScenario() {
  try {
    await store.createDemo()
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

async function onDeleteScenario() {
  if (!scenario.value) return
  try {
    await ElMessageBox.confirm(t('confirmDeleteScenario'), t('deleteScenario'), {
      type: 'warning'
    })
  } catch {
    return
  }
  try {
    await store.remove(scenario.value.id)
    ElMessage.success(t('scenarioDeleted'))
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function triggerImport() {
  importInput.value?.click()
}

async function onImportChange(event: Event) {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file) return
  try {
    const text = await file.text()
    await store.importFromJson(JSON.parse(text))
    ElMessage.success(t('scenarioSaved'))
  } catch (error) {
    ElMessage.error(`${t('importFailed')}: ${errorText(error)}`)
  } finally {
    input.value = ''
  }
}

function exportScenario() {
  const current = scenario.value
  if (!current) return
  try {
    const payload = {
      name: current.name,
      description: current.description,
      data: current.data
    }
    const blob = new Blob([JSON.stringify(payload, null, 2)], {
      type: 'application/json'
    })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = `${current.name || 'scenario'}.json`
    anchor.click()
    URL.revokeObjectURL(url)
  } catch (error) {
    ElMessage.error(`${t('exportFailed')}: ${errorText(error)}`)
  }
}

/* ------------------------------ 对象操作 ------------------------------ */

const selectedSatellite = computed<SatelliteObject | null>(() => {
  if (selection.value?.type !== 'satellite') return null
  return (
    scenario.value?.data.satellites.find((item) => item.id === selection.value?.id) ??
    null
  )
})

const selectedStation = computed<GroundStationObject | null>(() => {
  if (selection.value?.type !== 'station') return null
  return (
    scenario.value?.data.ground_stations.find(
      (item) => item.id === selection.value?.id
    ) ?? null
  )
})

const objectForm = reactive({
  name: '',
  color: '#4fc3f7',
  enabled: true,
  line1: '',
  line2: '',
  latitude: 0,
  longitude: 0,
  altitude: 0,
  minElevation: 5
})

function syncObjectForm() {
  const satellite = selectedSatellite.value
  if (satellite) {
    objectForm.name = satellite.name
    objectForm.color = satellite.color || '#4fc3f7'
    objectForm.enabled = satellite.enabled
    objectForm.line1 = satellite.tle_line1
    objectForm.line2 = satellite.tle_line2
    return
  }
  const station = selectedStation.value
  if (station) {
    objectForm.name = station.name
    objectForm.color = station.color || '#ffd54f'
    objectForm.enabled = station.enabled
    objectForm.latitude = station.latitude_deg
    objectForm.longitude = station.longitude_deg
    objectForm.altitude = station.altitude_km
    objectForm.minElevation = station.min_elevation_deg
  }
}

watch([() => store.current, selection], syncObjectForm)

const selectedNorad = computed(() => selectedSatellite.value?.norad_id ?? '—')

function onSelectObject(payload: Selection) {
  selection.value = payload
  activeTab.value = 'props'
}

function cloneData(): ScenarioData | null {
  if (!scenario.value) return null
  return JSON.parse(JSON.stringify(scenario.value.data)) as ScenarioData
}

async function onSaveObject() {
  const current = scenario.value
  if (!current || !selection.value) return
  const data = cloneData()
  if (!data) return

  if (selection.value.type === 'satellite') {
    const satellite = data.satellites.find((item) => item.id === selection.value?.id)
    if (!satellite) return
    satellite.name = objectForm.name.trim() || satellite.name
    satellite.color = objectForm.color
    satellite.enabled = objectForm.enabled
    satellite.tle_line1 = objectForm.line1.trim()
    satellite.tle_line2 = objectForm.line2.trim()
  } else {
    const station = data.ground_stations.find((item) => item.id === selection.value?.id)
    if (!station) return
    station.name = objectForm.name.trim() || station.name
    station.color = objectForm.color
    station.enabled = objectForm.enabled
    station.latitude_deg = objectForm.latitude
    station.longitude_deg = objectForm.longitude
    station.altitude_km = objectForm.altitude
    station.min_elevation_deg = objectForm.minElevation
  }

  try {
    await store.saveData(data)
    ElMessage.success(t('objectSaved'))
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

async function onToggleObject(payload: Selection & { enabled: boolean }) {
  const data = cloneData()
  if (!data) return
  if (payload.type === 'satellite') {
    const satellite = data.satellites.find((item) => item.id === payload.id)
    if (satellite) satellite.enabled = payload.enabled
  } else {
    const station = data.ground_stations.find((item) => item.id === payload.id)
    if (station) station.enabled = payload.enabled
  }
  try {
    await store.saveData(data)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

async function onDeleteObject(payload: Selection) {
  try {
    await ElMessageBox.confirm(t('confirmDeleteObject'), t('delete'), {
      type: 'warning'
    })
  } catch {
    return
  }
  const data = cloneData()
  if (!data) return
  if (payload.type === 'satellite') {
    data.satellites = data.satellites.filter((item) => item.id !== payload.id)
  } else {
    data.ground_stations = data.ground_stations.filter((item) => item.id !== payload.id)
  }
  if (selection.value?.type === payload.type && selection.value.id === payload.id) {
    selection.value = null
  }
  try {
    await store.saveData(data)
    ElMessage.success(t('objectDeleted'))
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

const objectDialog = reactive({
  visible: false,
  type: 'satellite' as ObjectType,
  name: '',
  color: '#4fc3f7',
  line1: '',
  line2: '',
  latitude: 39.9,
  longitude: 116.4,
  altitude: 0,
  minElevation: 5
})

function openAddObject(type: ObjectType) {
  objectDialog.type = type
  objectDialog.name = ''
  objectDialog.color = type === 'satellite' ? '#4fc3f7' : '#ffd54f'
  objectDialog.line1 = ''
  objectDialog.line2 = ''
  objectDialog.latitude = 39.9
  objectDialog.longitude = 116.4
  objectDialog.altitude = 0
  objectDialog.minElevation = 5
  objectDialog.visible = true
}

async function submitAddObject() {
  const data = cloneData()
  if (!data) return
  const name = objectDialog.name.trim()
  if (!name) {
    ElMessage.warning(t('scenarioNameRequired'))
    return
  }

  if (objectDialog.type === 'satellite') {
    if (!objectDialog.line1.trim() || !objectDialog.line2.trim()) {
      ElMessage.warning(t('importFailed'))
      return
    }
    data.satellites.push({
      id: crypto.randomUUID(),
      name,
      enabled: true,
      color: objectDialog.color,
      tle_line1: objectDialog.line1.trim(),
      tle_line2: objectDialog.line2.trim()
    })
  } else {
    data.ground_stations.push({
      id: crypto.randomUUID(),
      name,
      enabled: true,
      color: objectDialog.color,
      latitude_deg: objectDialog.latitude,
      longitude_deg: objectDialog.longitude,
      altitude_km: objectDialog.altitude,
      min_elevation_deg: objectDialog.minElevation
    })
  }

  try {
    await store.saveData(data)
    objectDialog.visible = false
    ElMessage.success(t('objectAdded'))
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function onLocateObject() {
  const current = selection.value
  if (!current) return
  if (current.type === 'station') {
    const station = selectedStation.value
    if (station) {
      cesium.flyToLocation(station.longitude_deg, station.latitude_deg, 0)
    }
    return
  }
  const track = playback.value?.satellites.find(
    (item) => item.satellite_id === current.id
  )
  if (!track || track.samples.length === 0) return
  const step = track.samples[1]?.t_seconds - track.samples[0]?.t_seconds || 1
  const index = Math.min(
    Math.max(Math.floor(playhead.value / step), 0),
    track.samples.length - 1
  )
  const sample = track.samples[index]
  cesium.flyToLocation(sample.longitude_deg, sample.latitude_deg, sample.altitude_km)
}

/* ------------------------------ 时间线回放 ------------------------------ */

const playhead = ref(0)
const playing = ref(false)
const speed = ref(30)
let rafId = 0
let lastTimestamp = 0

const playbackDuration = computed(() => {
  if (!playback.value) return 0
  const start = new Date(playback.value.start).getTime()
  const end = new Date(playback.value.end).getTime()
  return Math.max((end - start) / 1000, 1)
})

const currentUtc = computed(() => {
  if (!playback.value) return '—'
  const milliseconds = new Date(playback.value.start).getTime() + playhead.value * 1000
  return formatUtc(milliseconds)
})

watch(playhead, (value) => cesium.setPlayhead(value))

watch(
  () => store.propagation,
  (report) => {
    const current = scenario.value
    if (!current) return
    cesium.renderScenario(current.data, report?.satellites ?? null)
    playhead.value = 0
    stopPlayback()
    const access = store.accessReport
    if (access) {
      cesium.highlightAccess(access, accessTimeOffset(access))
    } else {
      cesium.highlightAccess(null)
    }
  }
)

watch(
  () => store.accessReport,
  (report) => cesium.highlightAccess(report, accessTimeOffset(report))
)

/** Access 结果与传播结果的起始时刻之差（秒）。 */
function accessTimeOffset(report: typeof accessReport.value) {
  if (!report || !playback.value) return 0
  return (
    (new Date(report.start).getTime() - new Date(playback.value.start).getTime()) / 1000
  )
}

function startPlayback() {
  if (!playback.value || playing.value) return
  playing.value = true
  lastTimestamp = 0
  rafId = requestAnimationFrame(tick)
}

function tick(timestamp: number) {
  if (!playing.value) return
  if (!lastTimestamp) lastTimestamp = timestamp
  const delta = (timestamp - lastTimestamp) / 1000
  lastTimestamp = timestamp
  let next = playhead.value + delta * speed.value
  if (next >= playbackDuration.value) {
    next -= playbackDuration.value
  }
  playhead.value = next
  rafId = requestAnimationFrame(tick)
}

function stopPlayback() {
  playing.value = false
  cancelAnimationFrame(rafId)
  lastTimestamp = 0
}

function togglePlayback() {
  if (playing.value) stopPlayback()
  else startPlayback()
}

async function triggerPropagation() {
  try {
    await store.runPropagation()
  } catch (error) {
    ElMessage.error(`${t('propagationFailed')}: ${errorText(error)}`)
  }
}

/* ------------------------------ Access 分析 ------------------------------ */

const accessForm = reactive({
  satelliteId: '',
  stationId: '',
  start: new Date(),
  end: new Date(Date.now() + 12 * 3600 * 1000),
  stepSeconds: 30,
  minElevation: 5
})

function syncAccessDefaults() {
  const data = scenario.value?.data
  accessForm.satelliteId =
    data?.satellites.find((item) => item.enabled)?.id ??
    data?.satellites[0]?.id ??
    ''
  accessForm.stationId = data?.ground_stations[0]?.id ?? ''
  accessForm.start = new Date()
  accessForm.end = new Date(accessForm.start.getTime() + 12 * 3600 * 1000)
  syncMinElevation()
}

function syncMinElevation() {
  const station = scenario.value?.data.ground_stations.find(
    (item) => item.id === accessForm.stationId
  )
  if (station) {
    accessForm.minElevation = station.min_elevation_deg
  }
}

async function onComputeAccess() {
  if (!scenario.value) return
  if (!accessForm.satelliteId || !accessForm.stationId) {
    ElMessage.warning(t('noSatellites'))
    return
  }
  try {
    const report = await store.runAccess({
      satelliteId: accessForm.satelliteId,
      stationId: accessForm.stationId,
      start: accessForm.start,
      end: accessForm.end,
      stepSeconds: accessForm.stepSeconds,
      minElevationDeg: accessForm.minElevation
    })
    if (report) {
      ElMessage.success(`${t('accessResult')}: ${report.window_count}`)
    }
  } catch (error) {
    ElMessage.error(`${t('accessFailed')}: ${errorText(error)}`)
  }
}

async function onExportCsv() {
  const job = accessJob.value
  if (!job) return
  try {
    await downloadAccessCsv(job.id, `access-${job.id.slice(0, 8)}.csv`)
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

/* ------------------------------ 生命周期 ------------------------------ */

let lastScenarioId = ''

watch(
  () => store.current,
  async (current) => {
    if (!current) {
      lastScenarioId = ''
      selection.value = null
      cesium.renderScenario({ satellites: [], ground_stations: [] }, null)
      return
    }

    const changed = current.id !== lastScenarioId
    if (changed) {
      lastScenarioId = current.id
      selection.value = null
      syncAccessDefaults()
    }

    cesium.renderScenario(current.data, store.propagation?.satellites ?? null)
    if (changed) {
      cesium.flyToScenario()
    }
    syncObjectForm()

    if (current.data.satellites.some((item) => item.enabled)) {
      await triggerPropagation()
    }
  }
)

onMounted(async () => {
  await cesium.init()
  try {
    await store.loadList()
    if (store.scenarios.length === 0) {
      await store.createDemo()
    } else {
      await store.select(store.scenarios[0].id)
    }
  } catch (error) {
    ElMessage.error(errorText(error))
  }
})

onBeforeUnmount(() => {
  stopPlayback()
})

async function onLogout() {
  auth.logout()
  await router.replace({ name: 'login' })
}

/* ------------------------------ 工具函数 ------------------------------ */

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatUtc(milliseconds: number) {
  return `${new Date(milliseconds).toISOString().replace('T', ' ').slice(0, 19)}Z`
}

function formatUtcString(value: string) {
  return value.replace('T', ' ').replace(/\.\d+Z$/, 'Z')
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
  <div class="shell">
    <header class="header">
      <span class="product">{{ t('appTitle') }}</span>
      <el-tag size="small" effect="dark" type="info" class="phase">{{ t('phaseLabel') }}</el-tag>
      <span v-if="store.propagationBusy" class="busy">{{ t('propagating') }}</span>
      <span class="spacer" />
      <LangSwitch />
      <span class="user">{{ auth.user?.username ?? t('unknownUser') }}</span>
      <el-button link type="primary" @click="onLogout">{{ t('logout') }}</el-button>
    </header>

    <main class="workspace">
      <aside class="panel panel-left">
        <div class="panel-title">{{ t('scenarioTitle') }}</div>
        <el-select
          v-model="scenarioId"
          size="small"
          class="scenario-select"
          :placeholder="t('noScenario')"
        >
          <el-option
            v-for="item in store.scenarios"
            :key="item.id"
            :label="item.name"
            :value="item.id"
          />
        </el-select>

        <div class="button-grid">
          <el-button size="small" :icon="Plus" @click="openScenarioDialog('create')">
            {{ t('newScenario') }}
          </el-button>
          <el-button size="small" @click="onDemoScenario">{{ t('demoScenario') }}</el-button>
          <el-button size="small" :icon="Upload" @click="triggerImport">
            {{ t('importScenario') }}
          </el-button>
          <el-button
            size="small"
            :icon="Download"
            :disabled="!scenario"
            @click="exportScenario"
          >
            {{ t('exportScenario') }}
          </el-button>
          <el-button
            size="small"
            :icon="Edit"
            :disabled="!scenario"
            @click="openScenarioDialog('edit')"
          >
            {{ t('scenarioSettings') }}
          </el-button>
          <el-button
            size="small"
            type="danger"
            plain
            :icon="Delete"
            :disabled="!scenario"
            @click="onDeleteScenario"
          >
            {{ t('deleteScenario') }}
          </el-button>
        </div>
        <input
          ref="importInput"
          type="file"
          accept="application/json,.json"
          class="file-input"
          @change="onImportChange"
        />

        <div class="panel-title objects-title">{{ t('objectsTitle') }}</div>
        <ObjectTree
          :satellites="scenario?.data.satellites ?? []"
          :stations="scenario?.data.ground_stations ?? []"
          :selected="selection"
          @select="onSelectObject"
          @add="openAddObject"
          @delete="onDeleteObject"
          @toggle="onToggleObject"
        />
      </aside>

      <section class="viewport">
        <div ref="viewerHost" class="cesium-host" />
        <div class="viewport-tip">{{ t('viewportTip') }}</div>

        <div class="viewport-actions">
          <el-button
            size="small"
            :icon="Refresh"
            :loading="store.propagationBusy"
            @click="triggerPropagation"
          >
            {{ t('propagate') }}
          </el-button>
        </div>

        <div v-if="playback" class="timeline">
          <el-button
            circle
            size="small"
            :icon="playing ? VideoPause : VideoPlay"
            @click="togglePlayback"
          />
          <span class="clock">{{ currentUtc }}</span>
          <el-slider
            v-model="playhead"
            class="slider"
            :min="0"
            :max="playbackDuration"
            :step="1"
            :show-tooltip="false"
          />
          <span class="duration">{{ formatDuration(playbackDuration) }}</span>
          <el-select v-model="speed" size="small" class="speed">
            <el-option label="1×" :value="1" />
            <el-option label="10×" :value="10" />
            <el-option label="60×" :value="60" />
            <el-option label="300×" :value="300" />
          </el-select>
        </div>
        <div v-else-if="scenario" class="timeline-empty">
          <span>{{ t('noTrack') }}</span>
          <el-button
            size="small"
            :icon="Refresh"
            :loading="store.propagationBusy"
            @click="triggerPropagation"
          >
            {{ t('propagate') }}
          </el-button>
        </div>
      </section>

      <aside class="panel panel-right">
        <el-tabs v-model="activeTab" class="tabs">
          <el-tab-pane :label="t('properties')" name="props">
            <template v-if="selection && (selectedSatellite || selectedStation)">
              <div class="prop-head">
                <span class="prop-name">{{ objectForm.name }}</span>
                <el-tag size="small" effect="plain">
                  {{ selection.type === 'satellite' ? t('satellites') : t('groundStations') }}
                </el-tag>
              </div>

              <el-form label-position="top" size="small" class="form">
                <el-form-item :label="t('name')">
                  <el-input v-model="objectForm.name" />
                </el-form-item>
                <el-form-item :label="t('color')">
                  <el-color-picker v-model="objectForm.color" />
                </el-form-item>
                <el-form-item :label="t('enabled')">
                  <el-switch v-model="objectForm.enabled" />
                </el-form-item>

                <template v-if="selection.type === 'satellite'">
                  <el-form-item :label="t('tleLine1')">
                    <el-input v-model="objectForm.line1" type="textarea" :rows="2" class="mono" />
                  </el-form-item>
                  <el-form-item :label="t('tleLine2')">
                    <el-input v-model="objectForm.line2" type="textarea" :rows="2" class="mono" />
                  </el-form-item>
                  <div class="row">
                    <span>{{ t('noradId') }}</span>
                    <b>{{ selectedNorad }}</b>
                  </div>
                </template>

                <template v-else>
                  <el-form-item :label="t('latitude')">
                    <el-input-number
                      v-model="objectForm.latitude"
                      :min="-90"
                      :max="90"
                      :precision="4"
                      :step="0.1"
                      controls-position="right"
                      style="width: 100%"
                    />
                  </el-form-item>
                  <el-form-item :label="t('longitude')">
                    <el-input-number
                      v-model="objectForm.longitude"
                      :min="-180"
                      :max="180"
                      :precision="4"
                      :step="0.1"
                      controls-position="right"
                      style="width: 100%"
                    />
                  </el-form-item>
                  <el-form-item :label="t('altitudeKm')">
                    <el-input-number
                      v-model="objectForm.altitude"
                      :min="-1"
                      :max="10000"
                      :precision="3"
                      :step="0.1"
                      controls-position="right"
                      style="width: 100%"
                    />
                  </el-form-item>
                  <el-form-item :label="t('minElevation')">
                    <el-input-number
                      v-model="objectForm.minElevation"
                      :min="0"
                      :max="90"
                      :precision="2"
                      :step="0.5"
                      controls-position="right"
                      style="width: 100%"
                    />
                  </el-form-item>
                </template>
              </el-form>

              <div class="prop-actions">
                <el-button
                  type="primary"
                  size="small"
                  :loading="store.saving"
                  @click="onSaveObject"
                >
                  {{ t('save') }}
                </el-button>
                <el-button size="small" :icon="Aim" @click="onLocateObject">
                  {{ t('locate') }}
                </el-button>
                <el-button
                  size="small"
                  type="danger"
                  plain
                  :icon="Delete"
                  @click="onDeleteObject(selection)"
                >
                  {{ t('delete') }}
                </el-button>
              </div>
            </template>
            <el-empty v-else :description="t('selectObjectHint')" :image-size="64" />
          </el-tab-pane>

          <el-tab-pane :label="t('accessTab')" name="access">
            <div class="section-title">{{ t('accessAnalysis') }}</div>
            <el-form label-position="top" size="small" class="form">
              <el-form-item :label="t('accessSatellite')">
                <el-select v-model="accessForm.satelliteId" style="width: 100%">
                  <el-option
                    v-for="satellite in scenario?.data.satellites ?? []"
                    :key="satellite.id"
                    :label="satellite.name"
                    :value="satellite.id"
                  />
                </el-select>
              </el-form-item>
              <el-form-item :label="t('accessStation')">
                <el-select
                  v-model="accessForm.stationId"
                  style="width: 100%"
                  @change="syncMinElevation"
                >
                  <el-option
                    v-for="station in scenario?.data.ground_stations ?? []"
                    :key="station.id"
                    :label="station.name"
                    :value="station.id"
                  />
                </el-select>
              </el-form-item>
              <el-form-item :label="t('accessStart')">
                <el-date-picker
                  v-model="accessForm.start"
                  type="datetime"
                  :clearable="false"
                  style="width: 100%"
                />
              </el-form-item>
              <el-form-item :label="t('accessEnd')">
                <el-date-picker
                  v-model="accessForm.end"
                  type="datetime"
                  :clearable="false"
                  style="width: 100%"
                />
              </el-form-item>
              <el-form-item :label="t('accessStep')">
                <el-input-number
                  v-model="accessForm.stepSeconds"
                  :min="1"
                  :max="600"
                  :step="5"
                  controls-position="right"
                  style="width: 100%"
                />
              </el-form-item>
              <el-form-item :label="t('accessMinElevation')">
                <el-input-number
                  v-model="accessForm.minElevation"
                  :min="0"
                  :max="90"
                  :precision="2"
                  :step="0.5"
                  controls-position="right"
                  style="width: 100%"
                />
              </el-form-item>
            </el-form>
            <el-button
              type="primary"
              size="small"
              class="full"
              :loading="store.accessBusy"
              @click="onComputeAccess"
            >
              {{ store.accessBusy ? t('computingAccess') : t('computeAccess') }}
            </el-button>

            <el-alert
              v-if="accessJob?.status === 'failed'"
              type="error"
              :closable="false"
              class="job-error"
              :title="accessJob.error ?? t('accessFailed')"
            />

            <template v-if="accessReport">
              <div class="section-title">{{ t('accessResult') }}</div>
              <div class="stats">
                <div class="stat">
                  <span>{{ t('windowCount') }}</span>
                  <b>{{ accessReport.window_count }}</b>
                </div>
                <div class="stat">
                  <span>{{ t('totalDuration') }}</span>
                  <b>{{ formatDuration(accessReport.total_duration_seconds) }}</b>
                </div>
                <div class="stat">
                  <span>{{ t('longestWindow') }}</span>
                  <b>{{ formatDuration(accessReport.longest_window_seconds) }}</b>
                </div>
              </div>

              <el-table
                :data="accessReport.windows"
                size="small"
                height="230"
                class="table"
              >
                <el-table-column prop="index" :label="t('windowIndex')" width="48" />
                <el-table-column :label="t('windowStart')" min-width="128">
                  <template #default="{ row }">
                    {{ formatUtcString(row.start_utc) }}
                  </template>
                </el-table-column>
                <el-table-column :label="t('windowEnd')" min-width="128">
                  <template #default="{ row }">
                    {{ formatUtcString(row.end_utc) }}
                  </template>
                </el-table-column>
                <el-table-column :label="t('windowDuration')" width="86">
                  <template #default="{ row }">
                    {{ formatDuration(row.duration_seconds) }}
                  </template>
                </el-table-column>
                <el-table-column :label="t('windowMaxElevation')" width="86">
                  <template #default="{ row }">
                    {{ row.max_elevation_deg.toFixed(1) }}°
                  </template>
                </el-table-column>
              </el-table>

              <el-button size="small" :icon="Download" class="full" @click="onExportCsv">
                {{ t('exportCsv') }}
              </el-button>
            </template>
            <el-empty v-else :description="t('noAccessResult')" :image-size="56" />
          </el-tab-pane>
        </el-tabs>
      </aside>
    </main>

    <el-dialog
      v-model="scenarioDialog.visible"
      :title="scenarioDialog.mode === 'create' ? t('newScenario') : t('scenarioSettings')"
      width="460px"
    >
      <el-form label-position="top" size="small" class="form">
        <el-form-item :label="t('scenarioName')">
          <el-input v-model="scenarioDialog.name" />
        </el-form-item>
        <el-form-item :label="t('scenarioDescription')">
          <el-input v-model="scenarioDialog.description" type="textarea" :rows="3" />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button size="small" @click="scenarioDialog.visible = false">
          {{ t('cancel') }}
        </el-button>
        <el-button size="small" type="primary" @click="submitScenarioDialog">
          {{ t('save') }}
        </el-button>
      </template>
    </el-dialog>

    <el-dialog
      v-model="objectDialog.visible"
      :title="
        objectDialog.type === 'satellite' ? t('addSatelliteTitle') : t('addStationTitle')
      "
      width="520px"
    >
      <el-form label-position="top" size="small" class="form">
        <el-form-item :label="t('name')">
          <el-input v-model="objectDialog.name" />
        </el-form-item>
        <el-form-item :label="t('color')">
          <el-color-picker v-model="objectDialog.color" />
        </el-form-item>

        <template v-if="objectDialog.type === 'satellite'">
          <el-form-item :label="t('tleLine1')">
            <el-input v-model="objectDialog.line1" type="textarea" :rows="2" class="mono" />
          </el-form-item>
          <el-form-item :label="t('tleLine2')">
            <el-input v-model="objectDialog.line2" type="textarea" :rows="2" class="mono" />
          </el-form-item>
        </template>

        <template v-else>
          <el-form-item :label="t('latitude')">
            <el-input-number
              v-model="objectDialog.latitude"
              :min="-90"
              :max="90"
              :precision="4"
              :step="0.1"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item :label="t('longitude')">
            <el-input-number
              v-model="objectDialog.longitude"
              :min="-180"
              :max="180"
              :precision="4"
              :step="0.1"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item :label="t('altitudeKm')">
            <el-input-number
              v-model="objectDialog.altitude"
              :min="-1"
              :max="10000"
              :precision="3"
              :step="0.1"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <el-form-item :label="t('minElevation')">
            <el-input-number
              v-model="objectDialog.minElevation"
              :min="0"
              :max="90"
              :precision="2"
              :step="0.5"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
        </template>
      </el-form>
      <template #footer>
        <el-button size="small" @click="objectDialog.visible = false">
          {{ t('cancel') }}
        </el-button>
        <el-button
          size="small"
          type="primary"
          :loading="store.saving"
          @click="submitAddObject"
        >
          {{ t('save') }}
        </el-button>
      </template>
    </el-dialog>
  </div>
</template>

<style scoped lang="scss">
.shell {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 16px;
  height: 48px;
  flex: none;
  background: var(--kst-panel);
  border-bottom: 1px solid var(--kst-border);
}

.product {
  font-size: 14px;
  font-weight: 600;
  letter-spacing: 0.04em;
  white-space: nowrap;
}

.phase {
  border-color: var(--kst-border);
}

.busy {
  font-size: 11px;
  color: var(--kst-accent-strong);
}

.spacer {
  flex: 1;
}

.user {
  color: var(--kst-text-dim);
  font-size: 13px;
}

.workspace {
  flex: 1;
  min-height: 0;
  display: flex;
}

.panel {
  flex: none;
  display: flex;
  flex-direction: column;
  padding: 14px;
  background: var(--kst-panel);
  overflow-y: auto;
}

.panel-left {
  width: 300px;
  border-right: 1px solid var(--kst-border);
}

.panel-right {
  width: 340px;
  border-left: 1px solid var(--kst-border);
}

.panel-title {
  font-size: 13px;
  font-weight: 600;
  letter-spacing: 0.06em;
  color: var(--kst-text);
  margin-bottom: 10px;
}

.objects-title {
  margin-top: 16px;
}

.scenario-select {
  width: 100%;
}

.button-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
  margin-top: 8px;

  :deep(.el-button) {
    margin-left: 0;
    width: 100%;
  }
}

.file-input {
  display: none;
}

.viewport {
  position: relative;
  flex: 1;
  min-width: 0;
  background: #04060b;
}

.cesium-host {
  position: absolute;
  inset: 0;
}

.viewport-tip {
  position: absolute;
  left: 12px;
  bottom: 12px;
  padding: 4px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 4px;
  background: rgba(9, 13, 18, 0.82);
  font-size: 11px;
  color: var(--kst-text-dim);
  pointer-events: none;
}

.viewport-actions {
  position: absolute;
  top: 12px;
  right: 12px;
  display: flex;
  gap: 8px;
}

.timeline {
  position: absolute;
  left: 50%;
  bottom: 14px;
  transform: translateX(-50%);
  width: min(820px, calc(100% - 48px));
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: 1px solid var(--kst-border);
  border-radius: 8px;
  background: rgba(9, 13, 18, 0.9);
  backdrop-filter: blur(6px);
}

.clock {
  font-family: 'JetBrains Mono', Consolas, 'Cascadia Mono', monospace;
  font-size: 11px;
  color: var(--kst-text);
  white-space: nowrap;
  min-width: 150px;
}

.slider {
  flex: 1;
  margin: 0 4px;
}

.duration {
  font-size: 11px;
  color: var(--kst-text-dim);
  white-space: nowrap;
}

.speed {
  width: 82px;
  flex: none;
}

.timeline-empty {
  position: absolute;
  left: 50%;
  bottom: 14px;
  transform: translateX(-50%);
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  border: 1px solid var(--kst-border);
  border-radius: 8px;
  background: rgba(9, 13, 18, 0.9);
  font-size: 11px;
  color: var(--kst-text-dim);
}

.tabs {
  :deep(.el-tabs__header) {
    margin-bottom: 10px;
  }

  :deep(.el-tabs__nav-wrap::after) {
    background-color: var(--kst-border);
  }

  :deep(.el-tabs__item) {
    font-size: 12px;
    padding: 0 12px;
  }
}

.form {
  :deep(.el-form-item) {
    margin-bottom: 9px;
  }

  :deep(.el-form-item__label) {
    padding-bottom: 2px;
    font-size: 12px;
    color: var(--kst-text-dim);
  }
}

.mono {
  :deep(textarea) {
    font-family: 'JetBrains Mono', Consolas, 'Cascadia Mono', monospace;
    font-size: 11px;
    line-height: 1.5;
  }
}

.prop-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 10px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--kst-border);
}

.prop-name {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  font-size: 12px;
  color: var(--kst-text-dim);
  padding: 3px 0 8px;

  b {
    color: var(--kst-text);
    font-weight: 500;
  }
}

.prop-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 4px;

  :deep(.el-button) {
    margin-left: 0;
  }
}

.section-title {
  margin: 14px 0 8px;
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
  margin-bottom: 10px;
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
    font-size: 13px;
    font-weight: 600;
    color: var(--kst-text);
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

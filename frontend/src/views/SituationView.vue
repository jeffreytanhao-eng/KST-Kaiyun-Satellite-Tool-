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

import AiPanel from '@/components/AiPanel.vue'
import CoveragePanel from '@/components/CoveragePanel.vue'
import EnvironmentPanel from '@/components/EnvironmentPanel.vue'
import LangSwitch from '@/components/LangSwitch.vue'
import ObjectTree from '@/components/ObjectTree.vue'
import { useCesiumViewer } from '@/composables/useCesiumViewer'
import { useAuthStore } from '@/stores/auth'
import { useScenarioStore } from '@/stores/scenario'
import { useI18n } from '@/i18n'
import type {
  GroundStationObject,
  SatelliteObject,
  ScenarioData,
  SensorObject,
  SensorPointingMode,
  SensorShape
} from '@/api/scenarios'
import { downloadJobCsv, type CoverageReport } from '@/api/jobs'

type ObjectType = 'satellite' | 'station' | 'sensor'
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
const activeTab = ref<'props' | 'access' | 'coverage' | 'environment' | 'ai'>('props')
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

const selectedSensor = computed<SensorObject | null>(() => {
  if (selection.value?.type !== 'sensor') return null
  return (
    scenario.value?.data.sensors.find((item) => item.id === selection.value?.id) ?? null
  )
})

const selectionTypeLabel = computed(() => {
  switch (selection.value?.type) {
    case 'satellite':
      return t('satellites')
    case 'station':
      return t('groundStations')
    case 'sensor':
      return t('sensors')
    default:
      return ''
  }
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
  minElevation: 5,
  satelliteId: '',
  shape: 'conical' as SensorShape,
  halfAngle: 30,
  hfov: 20,
  vfov: 20,
  pointingMode: 'nadir' as SensorPointingMode,
  fixedAzimuth: 0,
  offNadir: 0,
  targetStationId: '',
  maxRange: 3000
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
    return
  }
  const sensor = selectedSensor.value
  if (sensor) {
    objectForm.name = sensor.name
    objectForm.color = sensor.color || '#b39ddb'
    objectForm.enabled = sensor.enabled
    objectForm.satelliteId = sensor.satellite_id
    objectForm.shape = sensor.shape
    objectForm.halfAngle = sensor.half_angle_deg
    objectForm.hfov = sensor.horizontal_fov_deg
    objectForm.vfov = sensor.vertical_fov_deg
    objectForm.pointingMode = sensor.pointing_mode
    objectForm.fixedAzimuth = sensor.fixed_azimuth_deg
    objectForm.offNadir = sensor.fixed_off_nadir_deg
    objectForm.targetStationId = sensor.target_station_id ?? ''
    objectForm.maxRange = sensor.max_range_km
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
  } else if (selection.value.type === 'station') {
    const station = data.ground_stations.find((item) => item.id === selection.value?.id)
    if (!station) return
    station.name = objectForm.name.trim() || station.name
    station.color = objectForm.color
    station.enabled = objectForm.enabled
    station.latitude_deg = objectForm.latitude
    station.longitude_deg = objectForm.longitude
    station.altitude_km = objectForm.altitude
    station.min_elevation_deg = objectForm.minElevation
  } else {
    const sensor = data.sensors.find((item) => item.id === selection.value?.id)
    if (!sensor) return
    if (!objectForm.satelliteId) {
      ElMessage.warning(t('sensorNoSatellite'))
      return
    }
    if (objectForm.pointingMode === 'target' && !objectForm.targetStationId) {
      ElMessage.warning(t('sensorTargetRequired'))
      return
    }
    sensor.name = objectForm.name.trim() || sensor.name
    sensor.color = objectForm.color
    sensor.enabled = objectForm.enabled
    sensor.satellite_id = objectForm.satelliteId
    sensor.shape = objectForm.shape
    sensor.half_angle_deg = objectForm.halfAngle
    sensor.horizontal_fov_deg = objectForm.hfov
    sensor.vertical_fov_deg = objectForm.vfov
    sensor.pointing_mode = objectForm.pointingMode
    sensor.fixed_azimuth_deg = objectForm.fixedAzimuth
    sensor.fixed_off_nadir_deg = objectForm.offNadir
    sensor.target_station_id =
      objectForm.pointingMode === 'target' ? objectForm.targetStationId || null : null
    sensor.max_range_km = objectForm.maxRange
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
  } else if (payload.type === 'station') {
    const station = data.ground_stations.find((item) => item.id === payload.id)
    if (station) station.enabled = payload.enabled
  } else {
    const sensor = data.sensors.find((item) => item.id === payload.id)
    if (sensor) sensor.enabled = payload.enabled
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
    data.sensors = data.sensors.filter((item) => item.satellite_id !== payload.id)
  } else if (payload.type === 'station') {
    data.ground_stations = data.ground_stations.filter((item) => item.id !== payload.id)
    for (const sensor of data.sensors) {
      if (sensor.target_station_id === payload.id) {
        sensor.target_station_id = null
        if (sensor.pointing_mode === 'target') sensor.pointing_mode = 'nadir'
      }
    }
  } else {
    data.sensors = data.sensors.filter((item) => item.id !== payload.id)
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
  minElevation: 5,
  satelliteId: '',
  shape: 'conical' as SensorShape,
  halfAngle: 30,
  hfov: 20,
  vfov: 20,
  pointingMode: 'nadir' as SensorPointingMode,
  fixedAzimuth: 0,
  offNadir: 0,
  targetStationId: '',
  maxRange: 3000
})

const addDialogTitle = computed(() => {
  if (objectDialog.type === 'satellite') return t('addSatelliteTitle')
  if (objectDialog.type === 'station') return t('addStationTitle')
  return t('addSensorTitle')
})

function openAddObject(type: ObjectType) {
  if (type === 'sensor' && (scenario.value?.data.satellites.length ?? 0) === 0) {
    ElMessage.warning(t('sensorNoSatellite'))
    return
  }
  objectDialog.type = type
  objectDialog.name = ''
  objectDialog.color =
    type === 'satellite' ? '#4fc3f7' : type === 'station' ? '#ffd54f' : '#b39ddb'
  objectDialog.line1 = ''
  objectDialog.line2 = ''
  objectDialog.latitude = 39.9
  objectDialog.longitude = 116.4
  objectDialog.altitude = 0
  objectDialog.minElevation = 5
  objectDialog.satelliteId = scenario.value?.data.satellites[0]?.id ?? ''
  objectDialog.shape = 'conical'
  objectDialog.halfAngle = 30
  objectDialog.hfov = 20
  objectDialog.vfov = 20
  objectDialog.pointingMode = 'nadir'
  objectDialog.fixedAzimuth = 0
  objectDialog.offNadir = 0
  objectDialog.targetStationId = ''
  objectDialog.maxRange = 3000
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
  } else if (objectDialog.type === 'station') {
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
  } else {
    if (!objectDialog.satelliteId) {
      ElMessage.warning(t('sensorNoSatellite'))
      return
    }
    if (objectDialog.pointingMode === 'target' && !objectDialog.targetStationId) {
      ElMessage.warning(t('sensorTargetRequired'))
      return
    }
    data.sensors.push({
      id: crypto.randomUUID(),
      name,
      enabled: true,
      color: objectDialog.color,
      satellite_id: objectDialog.satelliteId,
      shape: objectDialog.shape,
      half_angle_deg: objectDialog.halfAngle,
      horizontal_fov_deg: objectDialog.hfov,
      vertical_fov_deg: objectDialog.vfov,
      pointing_mode: objectDialog.pointingMode,
      fixed_azimuth_deg: objectDialog.fixedAzimuth,
      fixed_off_nadir_deg: objectDialog.offNadir,
      target_station_id:
        objectDialog.pointingMode === 'target' ? objectDialog.targetStationId || null : null,
      max_range_km: objectDialog.maxRange
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
  const satelliteId =
    current.type === 'sensor' ? selectedSensor.value?.satellite_id : current.id
  if (!satelliteId) return
  const track = playback.value?.satellites.find(
    (item) => item.satellite_id === satelliteId
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
    cesium.renderScenario(current.data, report)
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
  minElevation: 5,
  azimuthMin: '',
  azimuthMax: '',
  sensorId: ''
})

const accessSensors = computed(() => {
  const data = scenario.value?.data
  if (!data || !accessForm.satelliteId) return []
  return data.sensors.filter((sensor) => sensor.satellite_id === accessForm.satelliteId)
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
  accessForm.azimuthMin = ''
  accessForm.azimuthMax = ''
  accessForm.sensorId = ''
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

function syncAccessSensor() {
  if (
    accessForm.sensorId &&
    !accessSensors.value.some((sensor) => sensor.id === accessForm.sensorId)
  ) {
    accessForm.sensorId = ''
  }
}

/** 解析可选的方位角输入：空为 null，非法为 undefined。 */
function parseOptionalAngle(value: string): number | null | undefined {
  const trimmed = value.trim()
  if (!trimmed) return null
  const parsed = Number(trimmed)
  if (!Number.isFinite(parsed) || parsed < 0 || parsed > 360) return undefined
  return parsed
}

async function onComputeAccess() {
  if (!scenario.value) return
  if (!accessForm.satelliteId || !accessForm.stationId) {
    ElMessage.warning(t('noSatellites'))
    return
  }
  const azimuthMin = parseOptionalAngle(accessForm.azimuthMin)
  const azimuthMax = parseOptionalAngle(accessForm.azimuthMax)
  if (azimuthMin === undefined || azimuthMax === undefined) {
    ElMessage.warning(t('accessAzimuthInvalid'))
    return
  }
  if ((azimuthMin == null) !== (azimuthMax == null)) {
    ElMessage.warning(t('accessAzimuthIncomplete'))
    return
  }
  try {
    const report = await store.runAccess({
      satelliteId: accessForm.satelliteId,
      stationId: accessForm.stationId,
      start: accessForm.start,
      end: accessForm.end,
      stepSeconds: accessForm.stepSeconds,
      minElevationDeg: accessForm.minElevation,
      azimuthMinDeg: azimuthMin,
      azimuthMaxDeg: azimuthMax,
      sensorId: accessForm.sensorId || null
    })
    if (report) {
      ElMessage.success(`${t('accessResult')}: ${report.window_count}`)
    }
  } catch (error) {
    ElMessage.error(`${t('accessFailed')}: ${errorText(error)}`)
  }
}

function constraintLabel(constraint: string) {
  switch (constraint) {
    case 'min_elevation':
      return t('constraintElevation')
    case 'azimuth_range':
      return t('constraintAzimuth')
    case 'earth_occlusion':
      return t('constraintOcclusion')
    case 'sensor_fov':
      return t('constraintSensor')
    default:
      return constraint
  }
}

function onShowCoverage(report: CoverageReport | null) {
  cesium.showCoverage(report)
}

async function onExportCsv() {
  const job = accessJob.value
  if (!job) return
  try {
    await downloadJobCsv(job.id, `access-${job.id.slice(0, 8)}.csv`)
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
      cesium.clearCoverage()
      cesium.renderScenario({ satellites: [], ground_stations: [], sensors: [] }, null)
      return
    }

    const changed = current.id !== lastScenarioId
    if (changed) {
      lastScenarioId = current.id
      selection.value = null
      cesium.clearCoverage()
      syncAccessDefaults()
    }

    cesium.renderScenario(current.data, store.propagation)
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
          :sensors="scenario?.data.sensors ?? []"
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
          <el-tab-pane :label="t('tabProps')" name="props">
            <template v-if="selection && (selectedSatellite || selectedStation || selectedSensor)">
              <div class="prop-head">
                <span class="prop-name">{{ objectForm.name }}</span>
                <el-tag size="small" effect="plain">{{ selectionTypeLabel }}</el-tag>
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

                <template v-else-if="selection.type === 'station'">
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

                <template v-else>
                  <el-form-item :label="t('sensorSatellite')">
                    <el-select v-model="objectForm.satelliteId" style="width: 100%">
                      <el-option
                        v-for="satellite in scenario?.data.satellites ?? []"
                        :key="satellite.id"
                        :label="satellite.name"
                        :value="satellite.id"
                      />
                    </el-select>
                  </el-form-item>
                  <el-form-item :label="t('sensorShape')">
                    <el-select v-model="objectForm.shape" style="width: 100%">
                      <el-option :label="t('sensorConical')" value="conical" />
                      <el-option :label="t('sensorRectangular')" value="rectangular" />
                    </el-select>
                  </el-form-item>
                  <el-form-item
                    v-if="objectForm.shape === 'conical'"
                    :label="t('sensorHalfAngle')"
                  >
                    <el-input-number
                      v-model="objectForm.halfAngle"
                      :min="0.1"
                      :max="90"
                      :precision="1"
                      :step="1"
                      controls-position="right"
                      style="width: 100%"
                    />
                  </el-form-item>
                  <template v-else>
                    <el-form-item :label="t('sensorHFov')">
                      <el-input-number
                        v-model="objectForm.hfov"
                        :min="0.1"
                        :max="180"
                        :precision="1"
                        :step="1"
                        controls-position="right"
                        style="width: 100%"
                      />
                    </el-form-item>
                    <el-form-item :label="t('sensorVFov')">
                      <el-input-number
                        v-model="objectForm.vfov"
                        :min="0.1"
                        :max="180"
                        :precision="1"
                        :step="1"
                        controls-position="right"
                        style="width: 100%"
                      />
                    </el-form-item>
                  </template>
                  <el-form-item :label="t('sensorPointing')">
                    <el-select v-model="objectForm.pointingMode" style="width: 100%">
                      <el-option :label="t('sensorNadir')" value="nadir" />
                      <el-option :label="t('sensorFixed')" value="fixed" />
                      <el-option :label="t('sensorTarget')" value="target" />
                    </el-select>
                  </el-form-item>
                  <template v-if="objectForm.pointingMode === 'fixed'">
                    <el-form-item :label="t('sensorFixedAzimuth')">
                      <el-input-number
                        v-model="objectForm.fixedAzimuth"
                        :min="0"
                        :max="360"
                        :precision="1"
                        :step="1"
                        controls-position="right"
                        style="width: 100%"
                      />
                    </el-form-item>
                    <el-form-item :label="t('sensorOffNadir')">
                      <el-input-number
                        v-model="objectForm.offNadir"
                        :min="0"
                        :max="180"
                        :precision="1"
                        :step="1"
                        controls-position="right"
                        style="width: 100%"
                      />
                    </el-form-item>
                  </template>
                  <el-form-item
                    v-else-if="objectForm.pointingMode === 'target'"
                    :label="t('sensorTargetStation')"
                  >
                    <el-select v-model="objectForm.targetStationId" style="width: 100%">
                      <el-option
                        v-for="station in scenario?.data.ground_stations ?? []"
                        :key="station.id"
                        :label="station.name"
                        :value="station.id"
                      />
                    </el-select>
                  </el-form-item>
                  <el-form-item :label="t('sensorMaxRange')">
                    <el-input-number
                      v-model="objectForm.maxRange"
                      :min="1"
                      :max="100000"
                      :step="100"
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

          <el-tab-pane :label="t('tabAccess')" name="access">
            <div class="section-title">{{ t('accessAnalysis') }}</div>
            <el-form label-position="top" size="small" class="form">
              <el-form-item :label="t('accessSatellite')">
                <el-select
                  v-model="accessForm.satelliteId"
                  style="width: 100%"
                  @change="syncAccessSensor"
                >
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
              <el-form-item :label="t('accessSensor')">
                <el-select
                  v-model="accessForm.sensorId"
                  clearable
                  :placeholder="t('accessSensorNone')"
                  style="width: 100%"
                >
                  <el-option :label="t('accessSensorNone')" value="" />
                  <el-option
                    v-for="sensor in accessSensors"
                    :key="sensor.id"
                    :label="sensor.name"
                    :value="sensor.id"
                  />
                </el-select>
              </el-form-item>
              <div class="grid2">
                <el-form-item :label="t('accessAzimuthMin')">
                  <el-input v-model="accessForm.azimuthMin" placeholder="—" />
                </el-form-item>
                <el-form-item :label="t('accessAzimuthMax')">
                  <el-input v-model="accessForm.azimuthMax" placeholder="—" />
                </el-form-item>
              </div>
              <div class="hint-line">{{ t('accessAzimuthHint') }}</div>
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

              <div v-if="accessReport.constraints.length > 0" class="constraint-row">
                <span class="constraint-label">{{ t('accessConstraints') }}</span>
                <el-tag
                  v-for="constraint in accessReport.constraints"
                  :key="constraint"
                  size="small"
                  effect="plain"
                >
                  {{ constraintLabel(constraint) }}
                </el-tag>
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

          <el-tab-pane :label="t('tabCoverage')" name="coverage">
            <CoveragePanel @show-coverage="onShowCoverage" />
          </el-tab-pane>

          <el-tab-pane :label="t('tabEnvironment')" name="environment">
            <EnvironmentPanel />
          </el-tab-pane>

          <el-tab-pane :label="t('tabAi')" name="ai">
            <AiPanel />
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

    <el-dialog v-model="objectDialog.visible" :title="addDialogTitle" width="520px">
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

        <template v-else-if="objectDialog.type === 'station'">
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

        <template v-else>
          <el-form-item :label="t('sensorSatellite')">
            <el-select v-model="objectDialog.satelliteId" style="width: 100%">
              <el-option
                v-for="satellite in scenario?.data.satellites ?? []"
                :key="satellite.id"
                :label="satellite.name"
                :value="satellite.id"
              />
            </el-select>
          </el-form-item>
          <el-form-item :label="t('sensorShape')">
            <el-select v-model="objectDialog.shape" style="width: 100%">
              <el-option :label="t('sensorConical')" value="conical" />
              <el-option :label="t('sensorRectangular')" value="rectangular" />
            </el-select>
          </el-form-item>
          <el-form-item v-if="objectDialog.shape === 'conical'" :label="t('sensorHalfAngle')">
            <el-input-number
              v-model="objectDialog.halfAngle"
              :min="0.1"
              :max="90"
              :precision="1"
              :step="1"
              controls-position="right"
              style="width: 100%"
            />
          </el-form-item>
          <template v-else>
            <el-form-item :label="t('sensorHFov')">
              <el-input-number
                v-model="objectDialog.hfov"
                :min="0.1"
                :max="180"
                :precision="1"
                :step="1"
                controls-position="right"
                style="width: 100%"
              />
            </el-form-item>
            <el-form-item :label="t('sensorVFov')">
              <el-input-number
                v-model="objectDialog.vfov"
                :min="0.1"
                :max="180"
                :precision="1"
                :step="1"
                controls-position="right"
                style="width: 100%"
              />
            </el-form-item>
          </template>
          <el-form-item :label="t('sensorPointing')">
            <el-select v-model="objectDialog.pointingMode" style="width: 100%">
              <el-option :label="t('sensorNadir')" value="nadir" />
              <el-option :label="t('sensorFixed')" value="fixed" />
              <el-option :label="t('sensorTarget')" value="target" />
            </el-select>
          </el-form-item>
          <template v-if="objectDialog.pointingMode === 'fixed'">
            <el-form-item :label="t('sensorFixedAzimuth')">
              <el-input-number
                v-model="objectDialog.fixedAzimuth"
                :min="0"
                :max="360"
                :precision="1"
                :step="1"
                controls-position="right"
                style="width: 100%"
              />
            </el-form-item>
            <el-form-item :label="t('sensorOffNadir')">
              <el-input-number
                v-model="objectDialog.offNadir"
                :min="0"
                :max="180"
                :precision="1"
                :step="1"
                controls-position="right"
                style="width: 100%"
              />
            </el-form-item>
          </template>
          <el-form-item
            v-else-if="objectDialog.pointingMode === 'target'"
            :label="t('sensorTargetStation')"
          >
            <el-select v-model="objectDialog.targetStationId" style="width: 100%">
              <el-option
                v-for="station in scenario?.data.ground_stations ?? []"
                :key="station.id"
                :label="station.name"
                :value="station.id"
              />
            </el-select>
          </el-form-item>
          <el-form-item :label="t('sensorMaxRange')">
            <el-input-number
              v-model="objectDialog.maxRange"
              :min="1"
              :max="100000"
              :step="100"
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
  width: 360px;
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
    padding: 0 8px;
  }
}

.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0 8px;
}

.hint-line {
  margin: -2px 0 8px;
  font-size: 10px;
  line-height: 1.5;
  color: var(--kst-text-dim);
}

.constraint-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: 8px;
}

.constraint-label {
  font-size: 10.5px;
  color: var(--kst-text-dim);
  margin-right: 2px;
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

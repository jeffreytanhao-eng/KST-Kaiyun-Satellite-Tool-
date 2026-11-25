import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import {
  createScenario,
  deleteScenario,
  getScenario,
  importScenario,
  listScenarios,
  updateScenario,
  type Scenario,
  type ScenarioData,
  type ScenarioPayload,
  type ScenarioSummary
} from '@/api/scenarios'
import {
  createJob,
  waitForJob,
  type AccessReport,
  type Job,
  type PropagationReport
} from '@/api/jobs'

const DEMO_TLE = {
  leo: {
    line1: '1 90001U 26001A   26329.50000000  .00001000  00000-0  20000-4 0  9993',
    line2: '2 90001  97.6000  30.0000 0010000  90.0000  10.0000 15.20000000    17'
  },
  meo: {
    line1: '1 90002U 26002B   26329.50000000  .00000000  00000+0  00000+0 0  9985',
    line2: '2 90002  55.0000 120.0000 0050000  50.0000 310.0000  2.00560000    14'
  },
  geo: {
    line1: '1 90003U 26003C   26329.50000000  .00000000  00000+0  00000+0 0  9976',
    line2: '2 90003   0.0500   0.0000 0010000   0.0000 100.0000  1.00270000    12'
  }
}

const PALETTE = ['#4fc3f7', '#ffb74d', '#81c784', '#e57373', '#ba68c8', '#4db6ac']

function newId() {
  return crypto.randomUUID()
}

export const useScenarioStore = defineStore('scenario', () => {
  const scenarios = ref<ScenarioSummary[]>([])
  const current = ref<Scenario | null>(null)
  const loading = ref(false)
  const saving = ref(false)

  const propagation = ref<PropagationReport | null>(null)
  const propagationJob = ref<Job | null>(null)
  const propagationBusy = ref(false)

  const accessReport = ref<AccessReport | null>(null)
  const accessJob = ref<Job | null>(null)
  const accessBusy = ref(false)

  const hasSatellites = computed(() => (current.value?.data.satellites.length ?? 0) > 0)
  const hasStations = computed(
    () => (current.value?.data.ground_stations.length ?? 0) > 0
  )

  function resetResults() {
    propagation.value = null
    propagationJob.value = null
    accessReport.value = null
    accessJob.value = null
  }

  async function loadList() {
    scenarios.value = await listScenarios()
  }

  async function select(id: string) {
    loading.value = true
    try {
      current.value = await getScenario(id)
      resetResults()
    } finally {
      loading.value = false
    }
  }

  async function refreshCurrent() {
    if (!current.value) return
    current.value = await getScenario(current.value.id)
  }

  async function create(name: string, description = '', data: ScenarioData = { satellites: [], ground_stations: [] }) {
    const created = await createScenario({ name, description, data })
    await loadList()
    await select(created.id)
    return created
  }

  async function createDemo() {
    const data: ScenarioData = {
      satellites: [
        {
          id: newId(),
          name: 'KST-SAT-01 · LEO',
          enabled: true,
          color: PALETTE[0],
          tle_line1: DEMO_TLE.leo.line1,
          tle_line2: DEMO_TLE.leo.line2,
          norad_id: 90001
        },
        {
          id: newId(),
          name: 'KST-SAT-02 · MEO',
          enabled: true,
          color: PALETTE[1],
          tle_line1: DEMO_TLE.meo.line1,
          tle_line2: DEMO_TLE.meo.line2,
          norad_id: 90002
        },
        {
          id: newId(),
          name: 'KST-SAT-03 · GEO',
          enabled: true,
          color: PALETTE[2],
          tle_line1: DEMO_TLE.geo.line1,
          tle_line2: DEMO_TLE.geo.line2,
          norad_id: 90003
        }
      ],
      ground_stations: [
        {
          id: newId(),
          name: '北京站',
          enabled: true,
          color: '#ffd54f',
          latitude_deg: 39.9042,
          longitude_deg: 116.4074,
          altitude_km: 0.05,
          min_elevation_deg: 5
        },
        {
          id: newId(),
          name: '三亚站',
          enabled: true,
          color: '#4db6ac',
          latitude_deg: 18.2528,
          longitude_deg: 109.5119,
          altitude_km: 0.02,
          min_elevation_deg: 5
        }
      ]
    }
    return create('示例场景 · 多对象分析', '包含 3 颗卫星与 2 个地面站的演示场景', data)
  }

  async function save(payload: Partial<ScenarioPayload>) {
    if (!current.value) return null
    saving.value = true
    try {
      const updated = await updateScenario(current.value.id, {
        name: payload.name ?? current.value.name,
        description: payload.description ?? current.value.description,
        data: payload.data ?? current.value.data
      })
      current.value = updated
      await loadList()
      return updated
    } finally {
      saving.value = false
    }
  }

  async function saveData(data: ScenarioData) {
    const updated = await save({ data })
    resetResults()
    return updated
  }

  async function remove(id: string) {
    await deleteScenario(id)
    if (current.value?.id === id) {
      current.value = null
      resetResults()
    }
    await loadList()
    if (!current.value && scenarios.value.length > 0) {
      await select(scenarios.value[0].id)
    }
  }

  async function importFromJson(raw: unknown) {
    if (!raw || typeof raw !== 'object') {
      throw new Error('JSON 格式无效')
    }
    const candidate = raw as Partial<Scenario>
    const data = (candidate.data ?? raw) as ScenarioData
    if (!Array.isArray(data.satellites) || !Array.isArray(data.ground_stations)) {
      throw new Error('JSON 中缺少 satellites / ground_stations')
    }
    const created = await importScenario({
      name: candidate.name?.trim() || '导入场景',
      description: candidate.description ?? '',
      data
    })
    await loadList()
    await select(created.id)
    return created
  }

  async function runPropagation(windowSeconds = 43_200, stepSeconds = 30) {
    if (!current.value || propagationBusy.value) return null
    const satelliteIds = current.value.data.satellites
      .filter((satellite) => satellite.enabled)
      .map((satellite) => satellite.id)
    if (satelliteIds.length === 0) {
      propagation.value = null
      return null
    }

    propagationBusy.value = true
    accessReport.value = null
    try {
      const start = new Date()
      const end = new Date(start.getTime() + windowSeconds * 1000)
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'propagation',
        params: {
          start: start.toISOString(),
          end: end.toISOString(),
          step_seconds: stepSeconds,
          satellite_ids: satelliteIds
        }
      })
      propagationJob.value = job
      const done = await waitForJob(job.id)
      propagationJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '传播任务失败')
      }
      propagation.value = done.result as PropagationReport
      return propagation.value
    } finally {
      propagationBusy.value = false
    }
  }

  async function runAccess(params: {
    satelliteId: string
    stationId: string
    start: Date
    end: Date
    stepSeconds?: number
    minElevationDeg?: number | null
  }) {
    if (!current.value || accessBusy.value) return null
    accessBusy.value = true
    accessReport.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'access',
        params: {
          satellite_id: params.satelliteId,
          ground_station_id: params.stationId,
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 30,
          ...(params.minElevationDeg != null
            ? { min_elevation_deg: params.minElevationDeg }
            : {})
        }
      })
      accessJob.value = job
      const done = await waitForJob(job.id)
      accessJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? 'Access 计算失败')
      }
      accessReport.value = done.result as AccessReport
      return accessReport.value
    } finally {
      accessBusy.value = false
    }
  }

  return {
    scenarios,
    current,
    loading,
    saving,
    propagation,
    propagationJob,
    propagationBusy,
    accessReport,
    accessJob,
    accessBusy,
    hasSatellites,
    hasStations,
    loadList,
    select,
    refreshCurrent,
    create,
    createDemo,
    save,
    saveData,
    remove,
    importFromJson,
    runPropagation,
    runAccess,
    resetResults
  }
})

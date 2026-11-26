import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import {
  createScenario,
  deleteScenario,
  emptyScenarioData,
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
  type ConstellationAnalysisReport,
  type CoverageRegion,
  type CoverageReport,
  type EnvironmentReport,
  type InterferenceReport,
  type Job,
  type LinkBudgetReport,
  type LinkOptions,
  type PropagationCompareReport,
  type PropagationReport,
  type RadarOptions,
  type RadarReport,
  type ReportDocument,
  type ReportRequest
} from '@/api/jobs'
import type { ConstellationGenerationReport } from '@/api/constellations'

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

  const coverageReport = ref<CoverageReport | null>(null)
  const coverageJob = ref<Job | null>(null)
  const coverageBusy = ref(false)

  const environmentReport = ref<EnvironmentReport | null>(null)
  const environmentJob = ref<Job | null>(null)
  const environmentBusy = ref(false)

  const compareReport = ref<PropagationCompareReport | null>(null)
  const compareJob = ref<Job | null>(null)
  const compareBusy = ref(false)

  const linkReport = ref<LinkBudgetReport | null>(null)
  const linkJob = ref<Job | null>(null)
  const linkBusy = ref(false)

  const constellationAnalysis = ref<ConstellationAnalysisReport | null>(null)
  const constellationAnalysisJob = ref<Job | null>(null)
  const constellationAnalysisBusy = ref(false)

  const radarReport = ref<RadarReport | null>(null)
  const radarJob = ref<Job | null>(null)
  const radarBusy = ref(false)

  const interferenceReport = ref<InterferenceReport | null>(null)
  const interferenceJob = ref<Job | null>(null)
  const interferenceBusy = ref(false)

  const reportDocument = ref<ReportDocument | null>(null)
  const reportJob = ref<Job | null>(null)
  const reportBusy = ref(false)

  const hasSatellites = computed(() => (current.value?.data.satellites.length ?? 0) > 0)
  const hasStations = computed(
    () => (current.value?.data.ground_stations.length ?? 0) > 0
  )

  function resetResults() {
    propagation.value = null
    propagationJob.value = null
    accessReport.value = null
    accessJob.value = null
    coverageReport.value = null
    coverageJob.value = null
    environmentReport.value = null
    environmentJob.value = null
    compareReport.value = null
    compareJob.value = null
    linkReport.value = null
    linkJob.value = null
    constellationAnalysis.value = null
    constellationAnalysisJob.value = null
    radarReport.value = null
    radarJob.value = null
    interferenceReport.value = null
    interferenceJob.value = null
    reportDocument.value = null
    reportJob.value = null
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

  async function create(
    name: string,
    description = '',
    data: ScenarioData = emptyScenarioData()
  ) {
    const created = await createScenario({ name, description, data })
    await loadList()
    await select(created.id)
    return created
  }

  async function createDemo() {
    const leoId = newId()
    const meoId = newId()
    const data: ScenarioData = {
      satellites: [
        {
          id: leoId,
          name: 'KST-SAT-01 · LEO',
          enabled: true,
          color: PALETTE[0],
          tle_line1: DEMO_TLE.leo.line1,
          tle_line2: DEMO_TLE.leo.line2,
          norad_id: 90001
        },
        {
          id: meoId,
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
      ],
      sensors: [
        {
          id: newId(),
          name: 'LEO 宽幅对地',
          enabled: true,
          color: '#4fc3f7',
          satellite_id: leoId,
          shape: 'conical',
          half_angle_deg: 45,
          horizontal_fov_deg: 30,
          vertical_fov_deg: 30,
          pointing_mode: 'nadir',
          fixed_azimuth_deg: 0,
          fixed_off_nadir_deg: 0,
          target_station_id: null,
          max_range_km: 2500
        },
        {
          id: newId(),
          name: 'MEO 区域扫描',
          enabled: true,
          color: '#ffb74d',
          satellite_id: meoId,
          shape: 'rectangular',
          half_angle_deg: 20,
          horizontal_fov_deg: 24,
          vertical_fov_deg: 12,
          pointing_mode: 'fixed',
          fixed_azimuth_deg: 25,
          fixed_off_nadir_deg: 20,
          target_station_id: null,
          max_range_km: 6000
        }
      ],
      constellations: []
    }
    return create('示例场景 · 多对象分析', '包含 3 颗卫星、2 个地面站与 2 个传感器的演示场景', data)
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

  /**
   * 保存场景数据。
   *
   * `resetResults` 默认为 true：轨道输入变化后旧的计算结果已失效。
   * 仅切换对象启用状态时传入 false，避免清空传播/分析结果并触发重新计算。
   */
  async function saveData(
    data: ScenarioData,
    options: { resetResults?: boolean } = {}
  ) {
    const updated = await save({ data })
    if (options.resetResults !== false) resetResults()
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
    if (!Array.isArray(data.sensors)) {
      data.sensors = []
    }
    if (!Array.isArray(data.constellations)) {
      data.constellations = []
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
    azimuthMinDeg?: number | null
    azimuthMaxDeg?: number | null
    sensorId?: string | null
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
            : {}),
          ...(params.azimuthMinDeg != null && params.azimuthMaxDeg != null
            ? {
                azimuth_min_deg: params.azimuthMinDeg,
                azimuth_max_deg: params.azimuthMaxDeg
              }
            : {}),
          ...(params.sensorId ? { sensor_id: params.sensorId } : {})
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

  async function runCoverage(params: {
    satelliteIds: string[]
    start: Date
    end: Date
    stepSeconds?: number
    region: CoverageRegion
    gridStepDeg?: number
    minElevationDeg?: number
  }) {
    if (!current.value || coverageBusy.value) return null
    coverageBusy.value = true
    coverageReport.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'coverage',
        params: {
          ...(params.satelliteIds.length > 0
            ? { satellite_ids: params.satelliteIds }
            : {}),
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 60,
          region: params.region,
          grid_step_deg: params.gridStepDeg ?? 2,
          min_elevation_deg: params.minElevationDeg ?? 5
        }
      })
      coverageJob.value = job
      const done = await waitForJob(job.id)
      coverageJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '覆盖计算失败')
      }
      coverageReport.value = done.result as CoverageReport
      return coverageReport.value
    } finally {
      coverageBusy.value = false
    }
  }

  async function runEnvironment(params: {
    satelliteId: string
    start: Date
    end: Date
    stepSeconds?: number
    massKg?: number
    areaM2?: number
    dragCoefficient?: number
    dailyF107?: number
    meanF107?: number
    ap?: number
  }) {
    if (!current.value || environmentBusy.value) return null
    environmentBusy.value = true
    environmentReport.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'environment',
        params: {
          satellite_id: params.satelliteId,
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 120,
          options: {
            mass_kg: params.massKg ?? 500,
            area_m2: params.areaM2 ?? 2,
            drag_coefficient: params.dragCoefficient ?? 2.2,
            daily_f107_sfu: params.dailyF107 ?? 150,
            mean81_f107_sfu: params.meanF107 ?? 150,
            ap: params.ap ?? 4
          }
        }
      })
      environmentJob.value = job
      const done = await waitForJob(job.id)
      environmentJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '环境计算失败')
      }
      environmentReport.value = done.result as EnvironmentReport
      return environmentReport.value
    } finally {
      environmentBusy.value = false
    }
  }

  async function runComparison(params: {
    satelliteId: string
    start: Date
    end: Date
    stepSeconds?: number
  }) {
    if (!current.value || compareBusy.value) return null
    compareBusy.value = true
    compareReport.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'propagation_compare',
        params: {
          satellite_id: params.satelliteId,
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 30
        }
      })
      compareJob.value = job
      const done = await waitForJob(job.id)
      compareJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '传播器对比失败')
      }
      compareReport.value = done.result as PropagationCompareReport
      return compareReport.value
    } finally {
      compareBusy.value = false
    }
  }

  async function runLink(params: {
    satelliteId: string
    stationId: string
    start: Date
    end: Date
    stepSeconds?: number
    options: LinkOptions
  }) {
    if (!current.value || linkBusy.value) return null
    linkBusy.value = true
    linkReport.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'link_budget',
        params: {
          satellite_id: params.satelliteId,
          ground_station_id: params.stationId,
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 30,
          options: params.options
        }
      })
      linkJob.value = job
      const done = await waitForJob(job.id)
      linkJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '链路预算计算失败')
      }
      linkReport.value = done.result as LinkBudgetReport
      return linkReport.value
    } finally {
      linkBusy.value = false
    }
  }

  /** 将后端生成的星座卫星与配置合并进当前场景。 */
  async function addConstellation(report: ConstellationGenerationReport) {
    if (!current.value) return 0
    const data = current.value.data
    const existing = new Set(data.satellites.map((satellite) => satellite.id))
    const additions = report.satellites.filter((satellite) => !existing.has(satellite.id))
    if (data.satellites.length + additions.length > 100) {
      throw new Error('卫星总数将超过 100 颗上限，请减少星座规模')
    }
    const next: ScenarioData = {
      satellites: [...data.satellites, ...additions],
      ground_stations: data.ground_stations,
      sensors: data.sensors,
      constellations: [...(data.constellations ?? []), report.constellation]
    }
    await saveData(next)
    return additions.length
  }

  async function runConstellationAnalysis(params: {
    satelliteIds?: string[]
    stationIds?: string[]
    start: Date
    end: Date
    stepSeconds?: number
    region: CoverageRegion
    gridStepDeg?: number
    minElevationDeg?: number
  }) {
    if (!current.value || constellationAnalysisBusy.value) return null
    constellationAnalysisBusy.value = true
    constellationAnalysis.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'constellation_analysis',
        params: {
          ...(params.satelliteIds && params.satelliteIds.length > 0
            ? { satellite_ids: params.satelliteIds }
            : {}),
          ...(params.stationIds && params.stationIds.length > 0
            ? { station_ids: params.stationIds }
            : {}),
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 60,
          region: params.region,
          grid_step_deg: params.gridStepDeg ?? 5,
          min_elevation_deg: params.minElevationDeg ?? 5
        }
      })
      constellationAnalysisJob.value = job
      const done = await waitForJob(job.id, 300_000)
      constellationAnalysisJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '星座批量分析失败')
      }
      const report = done.result as ConstellationAnalysisReport
      constellationAnalysis.value = report
      coverageReport.value = report.coverage
      return report
    } finally {
      constellationAnalysisBusy.value = false
    }
  }

  async function runRadar(params: {
    satelliteId: string
    stationId: string
    start: Date
    end: Date
    stepSeconds?: number
    options: RadarOptions
  }) {
    if (!current.value || radarBusy.value) return null
    radarBusy.value = true
    radarReport.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'radar',
        params: {
          satellite_id: params.satelliteId,
          ground_station_id: params.stationId,
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 30,
          options: params.options
        }
      })
      radarJob.value = job
      const done = await waitForJob(job.id)
      radarJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '雷达分析失败')
      }
      radarReport.value = done.result as RadarReport
      return radarReport.value
    } finally {
      radarBusy.value = false
    }
  }

  async function runInterference(params: {
    satelliteId: string
    stationId: string
    interfererIds: string[]
    start: Date
    end: Date
    stepSeconds?: number
    thresholdCiDb?: number
    options: LinkOptions
  }) {
    if (!current.value || interferenceBusy.value) return null
    interferenceBusy.value = true
    interferenceReport.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'interference',
        params: {
          satellite_id: params.satelliteId,
          ground_station_id: params.stationId,
          interferer_ids: params.interfererIds,
          start: params.start.toISOString(),
          end: params.end.toISOString(),
          step_seconds: params.stepSeconds ?? 30,
          options: params.options,
          threshold_ci_db: params.thresholdCiDb ?? 10
        }
      })
      interferenceJob.value = job
      const done = await waitForJob(job.id)
      interferenceJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '干扰分析失败')
      }
      interferenceReport.value = done.result as InterferenceReport
      return interferenceReport.value
    } finally {
      interferenceBusy.value = false
    }
  }

  async function runReport(params: ReportRequest) {    if (!current.value || reportBusy.value) return null
    reportBusy.value = true
    reportDocument.value = null
    try {
      const job = await createJob({
        scenario_id: current.value.id,
        kind: 'report',
        params: { ...params }
      })
      reportJob.value = job
      const done = await waitForJob(job.id, 300_000)
      reportJob.value = done
      if (done.status !== 'succeeded') {
        throw new Error(done.error ?? '报告生成失败')
      }
      reportDocument.value = done.result as ReportDocument
      return reportDocument.value
    } finally {
      reportBusy.value = false
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
    coverageReport,
    coverageJob,
    coverageBusy,
    environmentReport,
    environmentJob,
    environmentBusy,
    compareReport,
    compareJob,
    compareBusy,
    linkReport,
    linkJob,
    linkBusy,
    constellationAnalysis,
    constellationAnalysisJob,
    constellationAnalysisBusy,
    radarReport,
    radarJob,
    radarBusy,
    interferenceReport,
    interferenceJob,
    interferenceBusy,
    reportDocument,
    reportJob,
    reportBusy,
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
    runCoverage,
    runEnvironment,
    runComparison,
    runLink,
    addConstellation,
    runConstellationAnalysis,
    runRadar,
    runInterference,
    runReport,
    resetResults
  }
})

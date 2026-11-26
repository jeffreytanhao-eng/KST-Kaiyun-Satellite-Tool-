import { getToken, request } from './http'

export type JobStatus = 'queued' | 'running' | 'succeeded' | 'failed'
export type JobKind =
  | 'propagation'
  | 'access'
  | 'coverage'
  | 'environment'
  | 'propagation_compare'
  | 'link_budget'
  | 'constellation_analysis'
  | 'radar'
  | 'report'
  | 'interference'

export interface Job<T = unknown> {
  id: string
  scenario_id: string
  kind: JobKind
  status: JobStatus
  params: Record<string, unknown>
  result: T | null
  error: string | null
  created_at: string
  started_at: string | null
  finished_at: string | null
}

export interface TrackSample {
  t_seconds: number
  longitude_deg: number
  latitude_deg: number
  altitude_km: number
}

export interface SatelliteTrack {
  satellite_id: string
  name: string
  color: string
  samples: TrackSample[]
}

export interface SensorSample {
  t_seconds: number
  boresight: [number, number, number]
  right: [number, number, number]
  up: [number, number, number]
}

export interface SensorTrack {
  sensor_id: string
  sensor_name: string
  satellite_id: string
  color: string
  shape: 'conical' | 'rectangular'
  half_angle_deg: number
  horizontal_fov_deg: number
  vertical_fov_deg: number
  max_range_km: number
  pointing_mode: 'nadir' | 'fixed' | 'target' | 'body'
  samples: SensorSample[]
}

export interface AttitudeSample {
  t_seconds: number
  x: [number, number, number]
  y: [number, number, number]
  z: [number, number, number]
  quaternion: [number, number, number, number]
}

export interface AttitudeTrack {
  satellite_id: string
  name: string
  color: string
  mode: 'nadir' | 'sun' | 'fixed'
  samples: AttitudeSample[]
}

export interface PropagationReport {
  start: string
  end: string
  step_seconds: number
  satellites: SatelliteTrack[]
  sensor_tracks: SensorTrack[]
  attitude_tracks: AttitudeTrack[]
}

export interface AccessWindow {
  index: number
  start_utc: string
  end_utc: string
  start_seconds: number
  end_seconds: number
  duration_seconds: number
  max_elevation_deg: number
  constraints: string[]
}

export interface AccessReport {
  satellite_id: string
  satellite_name: string
  ground_station_id: string
  ground_station_name: string
  min_elevation_deg: number
  start: string
  end: string
  step_seconds: number
  window_count: number
  total_duration_seconds: number
  longest_window_seconds: number
  constraints: string[]
  azimuth_min_deg: number | null
  azimuth_max_deg: number | null
  sensor_id: string | null
  sensor_name: string | null
  windows: AccessWindow[]
}

export interface CoverageRegion {
  min_latitude_deg: number
  max_latitude_deg: number
  min_longitude_deg: number
  max_longitude_deg: number
}

export interface CoveragePoint {
  latitude_deg: number
  longitude_deg: number
  visible_seconds: number
  access_count: number
  max_elevation_deg: number
  covered: boolean
}

export interface CoverageSatelliteStat {
  satellite_id: string
  satellite_name: string
  covered_points: number
  coverage_ratio: number
  total_visible_seconds: number
  max_visible_seconds: number
}

export interface CoverageReport {
  satellite_ids: string[]
  satellite_names: string[]
  start: string
  end: string
  step_seconds: number
  min_elevation_deg: number
  region: CoverageRegion
  grid_step_deg: number
  total_points: number
  covered_points: number
  uncovered_points: number
  multi_covered_points: number
  coverage_ratio: number
  average_visible_seconds: number
  max_visible_seconds: number
  total_visible_seconds: number
  satellite_stats: CoverageSatelliteStat[]
  points: CoveragePoint[]
}

export interface CompareSample {
  t_seconds: number
  sgp4_longitude_deg: number
  sgp4_latitude_deg: number
  sgp4_altitude_km: number
  numerical_longitude_deg: number
  numerical_latitude_deg: number
  numerical_altitude_km: number
  position_difference_km: number
  velocity_difference_km_s: number
}

export interface CompareStatistics {
  mean_position_difference_km: number
  max_position_difference_km: number
  rms_position_difference_km: number
  mean_velocity_difference_km_s: number
  max_velocity_difference_km_s: number
  rms_velocity_difference_km_s: number
}

export interface IntegrationStats {
  accepted_steps: number
  rejected_steps: number
  min_step_seconds: number
  max_step_seconds: number
  mean_step_seconds: number
}

export interface PropagationCompareReport {
  satellite_id: string
  satellite_name: string
  start: string
  end: string
  step_seconds: number
  numerical_model: string
  j2_enabled: boolean
  drag_enabled: boolean
  integration: IntegrationStats
  statistics: CompareStatistics
  sgp4_track: SatelliteTrack
  numerical_track: SatelliteTrack
  samples: CompareSample[]
}

export type AntennaPattern = 'omni' | 'cosine'
export type AtmosphereModel = 'none' | 'simple' | 'itu'

export interface LinkOptions {
  frequency_hz: number
  tx_power_dbw: number
  tx_gain_db: number
  rx_gain_db: number
  misc_loss_db: number
  system_noise_temp_k: number
  bandwidth_hz: number
  required_cn0_dbhz: number
  satellite_antenna_pattern?: AntennaPattern
  satellite_pattern_exponent?: number
  atmosphere_model?: AtmosphereModel
  zenith_attenuation_db?: number
}

export interface LinkSample {
  t_seconds: number
  elevation_deg: number
  range_km: number
  off_nadir_deg: number
  pattern_gain_db: number
  free_space_loss_db: number
  atmospheric_loss_db: number
  received_power_dbw: number
  cn0_dbhz: number
  eb_n0_db: number
  margin_db: number
}

export interface LinkCurvePoint {
  t_seconds: number
  elevation_deg: number
  range_km: number
  cn0_dbhz: number
  margin_db: number
}

export interface LinkWindow {
  index: number
  start_utc: string
  end_utc: string
  start_seconds: number
  end_seconds: number
  duration_seconds: number
  min_elevation_deg: number
  max_elevation_deg: number
  min_range_km: number
  max_range_km: number
  min_margin_db: number
  mean_margin_db: number
  worst_elevation_deg: number
  samples: LinkSample[]
}

export interface LinkBudgetReport {
  satellite_id: string
  satellite_name: string
  ground_station_id: string
  ground_station_name: string
  start: string
  end: string
  step_seconds: number
  frequency_hz: number
  bandwidth_hz: number
  required_cn0_dbhz: number
  window_count: number
  total_duration_seconds: number
  min_margin_db: number
  mean_margin_db: number
  max_range_km: number
  mean_free_space_loss_db: number
  satellite_antenna_pattern: AntennaPattern
  satellite_pattern_exponent: number
  atmosphere_model: AtmosphereModel
  mean_atmospheric_loss_db: number
  max_atmospheric_loss_db: number
  curve: LinkCurvePoint[]
  windows: LinkWindow[]
}

export interface RevisitPointStat {
  latitude_deg: number
  longitude_deg: number
  access_count: number
  visible_seconds: number
  mean_gap_seconds: number
  max_gap_seconds: number
  max_elevation_deg: number
  covered: boolean
}

export interface StationAccessStat {
  station_id: string
  station_name: string
  window_count: number
  total_access_seconds: number
  mean_gap_seconds: number
  max_gap_seconds: number
  first_access_utc: string | null
  last_access_end_utc: string | null
}

export interface ConstellationAnalysisReport {
  satellite_ids: string[]
  satellite_names: string[]
  start: string
  end: string
  step_seconds: number
  min_elevation_deg: number
  region: CoverageRegion
  grid_step_deg: number
  total_points: number
  covered_points: number
  coverage_ratio: number
  multi_covered_points: number
  mean_access_count: number
  mean_revisit_seconds: number
  max_gap_seconds: number
  station_stats: StationAccessStat[]
  coverage: CoverageReport
  revisit_points: RevisitPointStat[]
  note: string
}

export interface RadarOptions {
  frequency_hz: number
  tx_power_w: number
  tx_gain_db: number
  rx_gain_db: number
  system_loss_db: number
  noise_figure_db: number
  bandwidth_hz: number
  target_rcs_m2: number
  required_snr_db: number
  pulses_integrated?: number
  min_elevation_deg?: number | null
}

export interface RadarSample {
  t_seconds: number
  elevation_deg: number
  range_km: number
  snr_db: number
  detected: boolean
}

export interface RadarWindow {
  index: number
  start_utc: string
  end_utc: string
  start_seconds: number
  end_seconds: number
  duration_seconds: number
  max_elevation_deg: number
  min_range_km: number
  max_range_km: number
  max_snr_db: number
  detected_seconds: number
  samples: RadarSample[]
}

export interface RadarReport {
  satellite_id: string
  satellite_name: string
  ground_station_id: string
  ground_station_name: string
  start: string
  end: string
  step_seconds: number
  frequency_hz: number
  bandwidth_hz: number
  tx_power_w: number
  target_rcs_m2: number
  required_snr_db: number
  integration_gain_db: number
  max_detection_range_km: number
  window_count: number
  detected_window_count: number
  total_detection_seconds: number
  min_range_km: number
  max_range_km: number
  samples: RadarSample[]
  windows: RadarWindow[]
}

export interface InterferenceSample {
  t_seconds: number
  elevation_deg: number
  range_km: number
  desired_cn0_dbhz: number
  interference_cn0_dbhz: number
  ci_db: number
  interferer_count: number
}

export interface InterfererStat {
  satellite_id: string
  satellite_name: string
  min_ci_db: number
  mean_ci_db: number
  visible_samples: number
}

export interface InterferenceReport {
  desired_satellite_id: string
  desired_satellite_name: string
  ground_station_id: string
  ground_station_name: string
  start: string
  end: string
  step_seconds: number
  frequency_hz: number
  threshold_ci_db: number
  sample_count: number
  degraded_samples: number
  degraded_ratio: number
  min_ci_db: number
  mean_ci_db: number
  interferers: InterfererStat[]
  samples: InterferenceSample[]
}

export interface ReportRequest {
  title?: string
  start: string
  end: string
  step_seconds?: number
  satellite_ids?: string[]
  station_ids?: string[]
  region?: CoverageRegion
  grid_step_deg?: number
  min_elevation_deg?: number
  include_access?: boolean
  include_coverage?: boolean
  include_link?: boolean
  include_radar?: boolean
  link_options?: LinkOptions
  radar_options?: RadarOptions
}

export interface ReportStats {
  satellite_count: number
  station_count: number
  access_pair_count: number
  access_window_count: number
  coverage_ratio: number | null
  max_gap_seconds: number | null
  min_link_margin_db: number | null
  radar_max_detection_range_km: number | null
}

export interface ReportDocument {
  title: string
  generated_at: string
  start: string
  end: string
  stats: ReportStats
  markdown: string
  html: string
}

export interface EnvironmentSample {
  t_seconds: number
  altitude_km: number
  corrected_altitude_km: number
  density_kg_m3: number
  temperature_k: number
  drag_accel_m_s2: number
  cumulative_delta_v_m_s: number
}

export interface EnvironmentReport {
  satellite_id: string
  satellite_name: string
  model: string
  start: string
  end: string
  step_seconds: number
  daily_f107_sfu: number
  mean81_f107_sfu: number
  ap: number
  mass_kg: number
  area_m2: number
  drag_coefficient: number
  mean_density_kg_m3: number
  min_density_kg_m3: number
  max_density_kg_m3: number
  delta_v_m_s: number
  estimated_altitude_loss_km: number
  start_altitude_km: number
  end_altitude_km: number
  corrected_end_altitude_km: number
  note: string
  samples: EnvironmentSample[]
}

export function createJob(payload: {
  scenario_id: string
  kind: JobKind
  params: Record<string, unknown>
}) {
  return request<Job>('/jobs', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

export function getJob(id: string) {
  return request<Job>(`/jobs/${id}`)
}

const sleep = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms))

/** 轮询任务直到终态，超时抛出异常。 */
export async function waitForJob(id: string, timeoutMs = 180_000): Promise<Job> {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    const job = await getJob(id)
    if (job.status === 'succeeded' || job.status === 'failed') {
      return job
    }
    if (Date.now() > deadline) {
      throw new Error('计算任务超时')
    }
    await sleep(600)
  }
}

/** 下载任务结果 CSV（后端生成，带鉴权）。 */
export async function downloadJobCsv(jobId: string, filename: string) {
  const token = getToken()
  const response = await fetch(`/api/v1/jobs/${jobId}/csv`, {
    headers: token ? { Authorization: `Bearer ${token}` } : undefined
  })
  if (!response.ok) {
    const data = await response.json().catch(() => null)
    throw new Error(data?.error?.message ?? `导出失败（HTTP ${response.status}）`)
  }
  const blob = await response.blob()
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.click()
  URL.revokeObjectURL(url)
}

/** 下载报告（HTML 或 Markdown，后端生成，带鉴权）。 */
export async function downloadJobReport(
  jobId: string,
  filename: string,
  format: 'html' | 'markdown'
) {
  const token = getToken()
  const response = await fetch(`/api/v1/jobs/${jobId}/report?format=${format}`, {
    headers: token ? { Authorization: `Bearer ${token}` } : undefined
  })
  if (!response.ok) {
    const data = await response.json().catch(() => null)
    throw new Error(data?.error?.message ?? `报告导出失败（HTTP ${response.status}）`)
  }
  const blob = await response.blob()
  const url = URL.createObjectURL(blob)
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.click()
  URL.revokeObjectURL(url)
}

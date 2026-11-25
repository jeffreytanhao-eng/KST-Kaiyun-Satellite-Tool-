import { getToken, request } from './http'

export type JobStatus = 'queued' | 'running' | 'succeeded' | 'failed'
export type JobKind = 'propagation' | 'access' | 'coverage' | 'environment'

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
  pointing_mode: 'nadir' | 'fixed' | 'target'
  samples: SensorSample[]
}

export interface PropagationReport {
  start: string
  end: string
  step_seconds: number
  satellites: SatelliteTrack[]
  sensor_tracks: SensorTrack[]
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
  coverage_ratio: number
  average_visible_seconds: number
  max_visible_seconds: number
  total_visible_seconds: number
  points: CoveragePoint[]
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

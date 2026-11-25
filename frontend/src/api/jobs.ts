import { getToken, request } from './http'

export type JobStatus = 'queued' | 'running' | 'succeeded' | 'failed'
export type JobKind = 'propagation' | 'access'

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

export interface PropagationReport {
  start: string
  end: string
  step_seconds: number
  satellites: SatelliteTrack[]
}

export interface AccessWindow {
  index: number
  start_utc: string
  end_utc: string
  start_seconds: number
  end_seconds: number
  duration_seconds: number
  max_elevation_deg: number
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
  windows: AccessWindow[]
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

/** 下载 Access 结果 CSV（后端生成，带鉴权）。 */
export async function downloadAccessCsv(jobId: string, filename: string) {
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

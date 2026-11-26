import { request } from './http'

export interface AiPreset {
  id: string
  name: string
  name_en: string
  description: string
  description_en: string
}

export interface AiConfig {
  enabled: boolean
  model: string
  presets: AiPreset[]
}

export interface OrbitSuggestion {
  altitude_km?: number
  inclination_deg?: number
  eccentricity?: number
}

export interface SensorSuggestion {
  shape?: 'conical' | 'rectangular'
  half_angle_deg?: number
  max_range_km?: number
  pointing_mode?: 'nadir' | 'fixed' | 'target'
}

export interface ConstellationSuggestion {
  constellation_type?: 'walker_delta' | 'walker_star' | 'streets'
  planes?: number
  total_satellites?: number
  phasing?: number
}

export interface ParameterSuggestion {
  summary?: string
  orbit?: OrbitSuggestion
  sensor?: SensorSuggestion
  constellation?: ConstellationSuggestion
  risks?: string[]
}

export interface AiResponse {
  content: string
  model: string
  latency_ms: number
  request_id: string
  suggestion?: ParameterSuggestion | null
}

export interface AiInterpretation {
  id: string
  job_id: string | null
  scenario_id: string | null
  preset: string
  model: string
  content: string
  created_by: string | null
  username: string | null
  created_at: string
}

export function getAiConfig() {
  return request<AiConfig>('/ai/config')
}

export function interpretJob(jobId: string, locale: string) {
  return request<AiResponse>('/ai/interpret', {
    method: 'POST',
    body: JSON.stringify({ job_id: jobId, locale })
  })
}

export function recommendScenario(scenarioId: string, objective: string, locale: string) {
  return request<AiResponse>('/ai/recommend', {
    method: 'POST',
    body: JSON.stringify({ scenario_id: scenarioId, objective, locale })
  })
}

export function generateJobReport(jobId: string, locale: string) {
  return request<AiResponse>('/ai/report', {
    method: 'POST',
    body: JSON.stringify({ job_id: jobId, locale })
  })
}

/** 使用 AI 润色 Markdown 报告（保持数值不变）。 */
export function polishReport(markdown: string, locale: string) {
  return request<AiResponse>('/ai/polish', {
    method: 'POST',
    body: JSON.stringify({ markdown, locale })
  })
}

/** 保存解读结果并关联到原分析任务。 */
export function saveInterpretation(payload: {
  job_id: string
  preset?: string
  content: string
  model?: string
}) {
  return request<AiInterpretation>('/ai/interpretations', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

export function listInterpretations(jobId?: string, limit = 20) {
  const params = new URLSearchParams()
  if (jobId) params.set('job_id', jobId)
  params.set('limit', String(limit))
  return request<AiInterpretation[]>(`/ai/interpretations?${params.toString()}`)
}

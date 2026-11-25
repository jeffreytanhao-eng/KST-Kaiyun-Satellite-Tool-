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

export interface AiResponse {
  content: string
  model: string
  latency_ms: number
  request_id: string
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

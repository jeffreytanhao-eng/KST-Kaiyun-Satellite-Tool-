import { request } from './http'
import type { ConstellationObject, ConstellationType, SatelliteObject } from './scenarios'

export interface ConstellationRequest {
  name: string
  constellation_type: ConstellationType
  altitude_km: number
  inclination_deg: number
  total_satellites: number
  planes: number
  phasing?: number
  color_by_plane?: boolean
  epoch_utc?: string
  satellites_per_plane?: number
}

export interface ConstellationPlane {
  index: number
  raan_deg: number
  color: string
  satellite_ids: string[]
}

export interface ConstellationGenerationReport {
  constellation_type: ConstellationType
  altitude_km: number
  inclination_deg: number
  total_satellites: number
  planes: number
  phasing: number
  satellites_per_plane: number
  orbital_period_minutes: number
  epoch_utc: string
  color_by_plane: boolean
  note: string
  constellation: ConstellationObject
  plane_details: ConstellationPlane[]
  satellites: SatelliteObject[]
}

/** 由后端生成 Walker / 街道式星座卫星对象（不自动写入场景）。 */
export function generateConstellation(payload: ConstellationRequest) {
  return request<ConstellationGenerationReport>('/constellations/generate', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

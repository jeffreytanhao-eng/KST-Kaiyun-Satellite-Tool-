import { request } from './http'

export interface SatelliteObject {
  id: string
  name: string
  enabled: boolean
  color: string
  tle_line1: string
  tle_line2: string
  norad_id?: number | null
}

export interface GroundStationObject {
  id: string
  name: string
  enabled: boolean
  color: string
  latitude_deg: number
  longitude_deg: number
  altitude_km: number
  min_elevation_deg: number
}

export type SensorShape = 'conical' | 'rectangular'
export type SensorPointingMode = 'nadir' | 'fixed' | 'target'

export interface SensorObject {
  id: string
  name: string
  enabled: boolean
  color: string
  satellite_id: string
  shape: SensorShape
  half_angle_deg: number
  horizontal_fov_deg: number
  vertical_fov_deg: number
  pointing_mode: SensorPointingMode
  fixed_azimuth_deg: number
  fixed_off_nadir_deg: number
  target_station_id?: string | null
  max_range_km: number
}

export interface ScenarioData {
  satellites: SatelliteObject[]
  ground_stations: GroundStationObject[]
  sensors: SensorObject[]
}

export interface Scenario {
  id: string
  name: string
  description: string
  data: ScenarioData
  created_at: string
  updated_at: string
}

export interface ScenarioSummary {
  id: string
  name: string
  description: string
  satellites: number
  ground_stations: number
  created_at: string
  updated_at: string
}

export interface ScenarioPayload {
  name: string
  description?: string
  data?: ScenarioData
}

export function listScenarios() {
  return request<ScenarioSummary[]>('/scenarios')
}

export function getScenario(id: string) {
  return request<Scenario>(`/scenarios/${id}`)
}

export function createScenario(payload: ScenarioPayload) {
  return request<Scenario>('/scenarios', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

export function updateScenario(id: string, payload: ScenarioPayload) {
  return request<Scenario>(`/scenarios/${id}`, {
    method: 'PUT',
    body: JSON.stringify(payload)
  })
}

export function deleteScenario(id: string) {
  return request<null>(`/scenarios/${id}`, { method: 'DELETE' })
}

export function importScenario(payload: ScenarioPayload) {
  return request<Scenario>('/scenarios/import', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

export function emptyScenarioData(): ScenarioData {
  return { satellites: [], ground_stations: [], sensors: [] }
}

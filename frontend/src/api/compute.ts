import { request } from './http'

export interface OrbitElements {
  semi_major_axis_km: number
  eccentricity: number
  inclination_deg: number
  raan_deg: number
  arg_perigee_deg: number
  true_anomaly_deg: number
}

export interface StateVector {
  position_km: [number, number, number]
  velocity_km_s: [number, number, number]
}

export interface StateResult {
  epoch: string
  et_seconds: number
  eci: StateVector
  ecef: StateVector
  longitude_deg: number
  latitude_deg: number
  altitude_km: number
  orbit_track: [number, number, number][]
}

export function computeState(payload: {
  epoch: string
  orbit: OrbitElements
  samples?: number
}) {
  return request<StateResult>('/compute/state', {
    method: 'POST',
    body: JSON.stringify(payload)
  })
}

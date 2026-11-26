import { request } from './http'

export type SharePermission = 'read' | 'write'

export interface ScenarioShare {
  id: string
  user_id: string
  username: string
  permission: SharePermission
  created_at: string
  updated_at: string
}

export interface ScenarioComment {
  id: string
  user_id: string | null
  username: string | null
  body: string
  created_at: string
}

export function listShares(scenarioId: string) {
  return request<ScenarioShare[]>(`/scenarios/${scenarioId}/shares`)
}

export function addShare(scenarioId: string, username: string, permission: SharePermission) {
  return request<ScenarioShare>(`/scenarios/${scenarioId}/shares`, {
    method: 'POST',
    body: JSON.stringify({ username, permission })
  })
}

export function updateShare(
  scenarioId: string,
  userId: string,
  permission: SharePermission
) {
  return request<ScenarioShare>(`/scenarios/${scenarioId}/shares/${userId}`, {
    method: 'PUT',
    body: JSON.stringify({ permission })
  })
}

export function removeShare(scenarioId: string, userId: string) {
  return request<null>(`/scenarios/${scenarioId}/shares/${userId}`, { method: 'DELETE' })
}

export function listComments(scenarioId: string) {
  return request<ScenarioComment[]>(`/scenarios/${scenarioId}/comments`)
}

export function addComment(scenarioId: string, body: string) {
  return request<ScenarioComment>(`/scenarios/${scenarioId}/comments`, {
    method: 'POST',
    body: JSON.stringify({ body })
  })
}

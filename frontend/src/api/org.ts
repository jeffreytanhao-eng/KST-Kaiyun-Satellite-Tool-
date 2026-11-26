import { request } from './http'

export type UserRole = 'admin' | 'analyst' | 'viewer'

export interface QuotaLimits {
  max_concurrent_jobs: number
  max_daily_jobs: number
  max_ai_daily: number
}

export interface Usage {
  org_id: string
  org_name: string
  role: UserRole
  limits: QuotaLimits
  active_jobs: number
  daily_jobs: number
  daily_ai: number
  org_active_jobs: number
  org_daily_jobs: number
  job_retention_hours: number
  job_cache_hours: number
  ai_enabled: boolean
}

export interface OrgMember {
  id: string
  username: string
  email: string | null
  role: UserRole
  max_concurrent_jobs: number | null
  max_daily_jobs: number | null
  max_ai_daily: number | null
  created_at: string
  active_jobs: number
}

export interface AuditEntry {
  id: string
  user_id: string | null
  username: string | null
  action: string
  resource_type: string
  resource_id: string | null
  details: Record<string, unknown>
  created_at: string
}

export interface MemberQuotaPayload {
  max_concurrent_jobs?: number | null
  max_daily_jobs?: number | null
  max_ai_daily?: number | null
  reset?: boolean
}

export function getUsage() {
  return request<Usage>('/org/usage')
}

export function listMembers() {
  return request<OrgMember[]>('/org/members')
}

export function updateMemberRole(memberId: string, role: UserRole) {
  return request<OrgMember>(`/org/members/${memberId}`, {
    method: 'PATCH',
    body: JSON.stringify({ role })
  })
}

export function updateMemberQuotas(memberId: string, payload: MemberQuotaPayload) {
  return request<OrgMember>(`/org/members/${memberId}/quotas`, {
    method: 'PATCH',
    body: JSON.stringify(payload)
  })
}

export function updateOrgQuotas(payload: Partial<QuotaLimits>) {
  return request<{ ok: boolean }>('/org/quotas', {
    method: 'PUT',
    body: JSON.stringify(payload)
  })
}

export function listAudit(limit = 100) {
  return request<AuditEntry[]>(`/org/audit?limit=${limit}`)
}

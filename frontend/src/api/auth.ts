import { request } from './http'

export interface UserInfo {
  id: string
  username: string
  email: string | null
  org_id: string | null
}

export interface LoginResult {
  token: string
  expires_at: string
  user: UserInfo
}

export function login(username: string, password: string) {
  return request<LoginResult>('/auth/login', {
    method: 'POST',
    body: JSON.stringify({ username, password })
  })
}

export function fetchMe() {
  return request<UserInfo>('/auth/me')
}

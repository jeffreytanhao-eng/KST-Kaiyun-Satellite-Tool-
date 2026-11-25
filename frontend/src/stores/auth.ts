import { computed, ref } from 'vue'
import { defineStore } from 'pinia'

import { fetchMe, login as loginApi, type UserInfo } from '@/api/auth'
import { getToken, setToken } from '@/api/http'

export const useAuthStore = defineStore('auth', () => {
  const token = ref<string | null>(getToken())
  const user = ref<UserInfo | null>(null)
  const isAuthenticated = computed(() => Boolean(token.value))

  async function login(username: string, password: string) {
    const result = await loginApi(username, password)
    token.value = result.token
    user.value = result.user
    setToken(result.token)
  }

  async function loadUser() {
    if (!token.value) return
    try {
      user.value = await fetchMe()
    } catch {
      logout()
    }
  }

  function logout() {
    token.value = null
    user.value = null
    setToken(null)
  }

  return { token, user, isAuthenticated, login, loadUser, logout }
})

import { createRouter, createWebHistory } from 'vue-router'

import { useAuthStore } from '@/stores/auth'

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
    { path: '/', redirect: '/situation' },
    {
      path: '/login',
      name: 'login',
      component: () => import('@/views/LoginView.vue'),
      meta: { public: true }
    },
    {
      path: '/situation',
      name: 'situation',
      component: () => import('@/views/SituationView.vue')
    }
  ]
})

router.beforeEach(async (to) => {
  const auth = useAuthStore()

  if (to.meta.public) {
    return auth.isAuthenticated ? { name: 'situation' } : true
  }
  if (!auth.isAuthenticated) {
    return { name: 'login', query: { redirect: to.fullPath } }
  }
  if (!auth.user) {
    await auth.loadUser()
    if (!auth.isAuthenticated) {
      return { name: 'login', query: { redirect: to.fullPath } }
    }
  }
  return true
})

export default router

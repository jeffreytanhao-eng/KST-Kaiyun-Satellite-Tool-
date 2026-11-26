<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { ElMessage, type FormInstance } from 'element-plus'

import LangSwitch from '@/components/LangSwitch.vue'
import { useAuthStore } from '@/stores/auth'
import { useI18n } from '@/i18n'

const router = useRouter()
const route = useRoute()
const auth = useAuthStore()
const { t } = useI18n()

const formRef = ref<FormInstance>()
const loading = ref(false)
const form = reactive({ username: '', password: '' })

const rules = computed(() => ({
  username: [{ required: true, message: t('usernameRequired'), trigger: 'blur' }],
  password: [{ required: true, message: t('passwordRequired'), trigger: 'blur' }]
}))

async function onSubmit() {
  if (!formRef.value) return
  const valid = await formRef.value.validate().catch(() => false)
  if (!valid) return

  loading.value = true
  try {
    await auth.login(form.username, form.password)
    const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : '/situation'
    await router.replace(redirect)
  } catch (error) {
    ElMessage.error(error instanceof Error ? error.message : t('loginFailed'))
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="login-page">
    <div class="topbar">
      <LangSwitch />
    </div>

    <div class="brand">
      <h1>OSKT</h1>
      <p>{{ t('brandSubtitle') }}</p>
    </div>
    <el-card class="login-card" shadow="always">
      <template #header>{{ t('login') }}</template>
      <el-form
        ref="formRef"
        :model="form"
        :rules="rules"
        label-position="top"
        @submit.prevent="onSubmit"
      >
        <el-form-item :label="t('username')" prop="username">
          <el-input
            v-model="form.username"
            autocomplete="username"
            :placeholder="t('usernamePlaceholder')"
          />
        </el-form-item>
        <el-form-item :label="t('password')" prop="password">
          <el-input
            v-model="form.password"
            type="password"
            show-password
            autocomplete="current-password"
            :placeholder="t('passwordPlaceholder')"
            @keyup.enter="onSubmit"
          />
        </el-form-item>
        <el-button type="primary" class="submit" :loading="loading" @click="onSubmit">
          {{ t('login') }}
        </el-button>
      </el-form>
    </el-card>
  </div>
</template>

<style scoped lang="scss">
.login-page {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 72px;
  height: 100%;
  background:
    radial-gradient(circle at 20% 30%, rgba(91, 155, 184, 0.14), transparent 45%),
    var(--kst-bg);
}

.topbar {
  position: absolute;
  top: 16px;
  right: 20px;
}

.brand {
  color: var(--kst-text);

  h1 {
    margin: 0 0 12px;
    font-size: 56px;
    letter-spacing: 0.16em;
  }

  p {
    margin: 0;
    color: var(--kst-text-dim);
    font-size: 14px;
  }
}

.login-card {
  width: 340px;
  background: var(--kst-panel);
  border: 1px solid var(--kst-border);
}

.submit {
  width: 100%;
}
</style>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { ElMessage } from 'element-plus'

import {
  getUsage,
  listAudit,
  listMembers,
  updateMemberQuotas,
  updateMemberRole,
  updateOrgQuotas,
  type AuditEntry,
  type OrgMember,
  type Usage,
  type UserRole
} from '@/api/org'
import { useAuthStore } from '@/stores/auth'
import { useI18n } from '@/i18n'

const props = defineProps<{ active?: boolean }>()

const auth = useAuthStore()
const { t } = useI18n()

const usage = ref<Usage | null>(null)
const members = ref<OrgMember[]>([])
const audit = ref<AuditEntry[]>([])
const loading = ref(false)
const saving = ref(false)

const quotaDialog = ref(false)
const quotaMember = ref<OrgMember | null>(null)
const quotaForm = ref<{ concurrent: number | null; daily: number | null; ai: number | null; reset: boolean }>({
  concurrent: null,
  daily: null,
  ai: null,
  reset: false
})

const orgQuotaForm = ref({ concurrent: 5, daily: 500, ai: 200 })

const isAdmin = computed(() => auth.user?.role === 'admin')

const roleKeys: Record<UserRole, string> = {
  admin: 'roleAdmin',
  analyst: 'roleAnalyst',
  viewer: 'roleViewer'
}

function roleLabel(role: UserRole) {
  const key = roleKeys[role]
  return key ? t(key as Parameters<typeof t>[0]) : role
}

async function load() {
  loading.value = true
  try {
    usage.value = await getUsage()
    members.value = await listMembers()
    if (isAdmin.value) {
      audit.value = await listAudit(50)
      orgQuotaForm.value = {
        concurrent: usage.value.limits.max_concurrent_jobs,
        daily: usage.value.limits.max_daily_jobs,
        ai: usage.value.limits.max_ai_daily
      }
    }
  } catch (error) {
    ElMessage.error(`${t('orgLoadFailed')}: ${errorText(error)}`)
  } finally {
    loading.value = false
  }
}

async function onRoleChange(member: OrgMember, role: UserRole) {
  if (member.role === role) return
  try {
    const updated = await updateMemberRole(member.id, role)
    const index = members.value.findIndex((item) => item.id === member.id)
    if (index >= 0) members.value[index] = updated
    ElMessage.success(t('quotaSaved'))
  } catch (error) {
    ElMessage.error(errorText(error))
  }
}

function openQuota(member: OrgMember) {
  quotaMember.value = member
  quotaForm.value = {
    concurrent: member.max_concurrent_jobs,
    daily: member.max_daily_jobs,
    ai: member.max_ai_daily,
    reset: false
  }
  quotaDialog.value = true
}

async function saveQuota() {
  if (!quotaMember.value) return
  saving.value = true
  try {
    const updated = await updateMemberQuotas(quotaMember.value.id, {
      max_concurrent_jobs: quotaForm.value.concurrent,
      max_daily_jobs: quotaForm.value.daily,
      max_ai_daily: quotaForm.value.ai,
      reset: quotaForm.value.reset
    })
    const index = members.value.findIndex((item) => item.id === updated.id)
    if (index >= 0) members.value[index] = updated
    quotaDialog.value = false
    ElMessage.success(t('quotaSaved'))
  } catch (error) {
    ElMessage.error(`${t('quotaFailed')}: ${errorText(error)}`)
  } finally {
    saving.value = false
  }
}

async function saveOrgQuotas() {
  saving.value = true
  try {
    await updateOrgQuotas({
      max_concurrent_jobs: orgQuotaForm.value.concurrent,
      max_daily_jobs: orgQuotaForm.value.daily,
      max_ai_daily: orgQuotaForm.value.ai
    })
    ElMessage.success(t('quotaSaved'))
    await load()
  } catch (error) {
    ElMessage.error(`${t('quotaFailed')}: ${errorText(error)}`)
  } finally {
    saving.value = false
  }
}

function errorText(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

onMounted(load)

watch(() => props.active, (active) => {
  if (active) void load()
})
</script>

<template>
  <div class="section-title org-title">
    <span>{{ t('orgTitle') }}</span>
    <el-button link size="small" :loading="loading" @click="load">{{ t('refresh') }}</el-button>
  </div>

  <template v-if="usage">
    <div class="usage-grid">
      <div class="usage-card">
        <span class="usage-label">{{ t('orgName') }}</span>
        <span class="usage-value">{{ usage.org_name }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('orgRole') }}</span>
        <span class="usage-value">{{ roleLabel(usage.role) }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('usageActive') }}</span>
        <span class="usage-value">{{ usage.active_jobs }} / {{ usage.limits.max_concurrent_jobs }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('usageDaily') }}</span>
        <span class="usage-value">{{ usage.daily_jobs }} / {{ usage.limits.max_daily_jobs }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('usageDailyAi') }}</span>
        <span class="usage-value">{{ usage.daily_ai }} / {{ usage.limits.max_ai_daily }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('usageOrgActive') }}</span>
        <span class="usage-value">{{ usage.org_active_jobs }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('usageRetention') }}</span>
        <span class="usage-value">{{ usage.job_retention_hours }}{{ t('usageHours') }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('usageCache') }}</span>
        <span class="usage-value">{{ usage.job_cache_hours }}{{ t('usageHours') }}</span>
      </div>
      <div class="usage-card">
        <span class="usage-label">{{ t('usageAiEnabled') }}</span>
        <span class="usage-value">{{ usage.ai_enabled ? t('on') : t('off') }}</span>
      </div>
    </div>
  </template>

  <div v-if="!isAdmin" class="hint">{{ t('orgAdminOnly') }}</div>

  <template v-if="isAdmin">
    <div class="section-title">{{ t('orgQuotaTitle') }}</div>
    <div class="quota-form">
      <label>
        <span>{{ t('quotaConcurrent') }}</span>
        <el-input-number v-model="orgQuotaForm.concurrent" :min="1" :max="100000" size="small" controls-position="right" />
      </label>
      <label>
        <span>{{ t('quotaDaily') }}</span>
        <el-input-number v-model="orgQuotaForm.daily" :min="1" :max="100000" size="small" controls-position="right" />
      </label>
      <label>
        <span>{{ t('quotaAi') }}</span>
        <el-input-number v-model="orgQuotaForm.ai" :min="1" :max="100000" size="small" controls-position="right" />
      </label>
      <el-button size="small" type="primary" :loading="saving" @click="saveOrgQuotas">
        {{ t('quotaSave') }}
      </el-button>
    </div>

    <div class="section-title">{{ t('orgMembers') }}</div>
    <div class="member-list">
      <div v-for="member in members" :key="member.id" class="member-item">
        <div class="member-main">
          <span class="member-name">{{ member.username }}</span>
          <span class="member-meta">{{ t('memberActive') }}: {{ member.active_jobs }}</span>
        </div>
        <el-select
          :model-value="member.role"
          size="small"
          style="width: 96px"
          @update:model-value="(value: UserRole) => onRoleChange(member, value)"
        >
          <el-option :label="t('roleAdmin')" value="admin" />
          <el-option :label="t('roleAnalyst')" value="analyst" />
          <el-option :label="t('roleViewer')" value="viewer" />
        </el-select>
        <el-button size="small" plain @click="openQuota(member)">
          {{ t('memberQuotas') }}
        </el-button>
      </div>
    </div>

    <div class="section-title">{{ t('auditTitle') }}</div>
    <div v-if="audit.length === 0" class="hint">{{ t('auditEmpty') }}</div>
    <div v-else class="audit-list">
      <div v-for="entry in audit" :key="entry.id" class="audit-item">
        <div class="audit-head">
          <span class="audit-action">{{ entry.action }}</span>
          <span class="audit-time">{{ new Date(entry.created_at).toLocaleString() }}</span>
        </div>
        <div class="audit-meta">
          <span>{{ t('auditUser') }}: {{ entry.username || t('commentUnknown') }}</span>
          <span>{{ t('auditResource') }}: {{ entry.resource_type }}</span>
        </div>
      </div>
    </div>
  </template>

  <el-dialog v-model="quotaDialog" :title="t('quotaTitle')" width="360px">
    <div class="quota-form column">
      <label>
        <span>{{ t('quotaConcurrent') }}</span>
        <el-input-number v-model="quotaForm.concurrent" :min="1" :max="100000" size="small" controls-position="right" />
      </label>
      <label>
        <span>{{ t('quotaDaily') }}</span>
        <el-input-number v-model="quotaForm.daily" :min="1" :max="100000" size="small" controls-position="right" />
      </label>
      <label>
        <span>{{ t('quotaAi') }}</span>
        <el-input-number v-model="quotaForm.ai" :min="1" :max="100000" size="small" controls-position="right" />
      </label>
      <el-checkbox v-model="quotaForm.reset">{{ t('quotaReset') }}</el-checkbox>
    </div>
    <template #footer>
      <el-button size="small" @click="quotaDialog = false">{{ t('cancel') }}</el-button>
      <el-button size="small" type="primary" :loading="saving" @click="saveQuota">
        {{ t('quotaSave') }}
      </el-button>
    </template>
  </el-dialog>
</template>

<style scoped lang="scss">
.section-title {
  margin: 12px 0 8px;
  font-size: 11px;
  letter-spacing: 0.06em;
  color: var(--kst-accent-strong);
}

.org-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 0;
}

.usage-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
}

.usage-card {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.55);
}

.usage-label {
  font-size: 9.5px;
  color: var(--kst-text-dim);
}

.usage-value {
  font-size: 11.5px;
  font-weight: 600;
}

.hint {
  margin-top: 8px;
  font-size: 10.5px;
  line-height: 1.5;
  color: var(--kst-text-dim);
}

.quota-form {
  display: flex;
  gap: 8px;
  align-items: flex-end;
  flex-wrap: wrap;

  &.column {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 10px;
    color: var(--kst-text-dim);
  }
}

.member-list,
.audit-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.member-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.55);
}

.member-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.member-name {
  font-size: 11px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.member-meta {
  font-size: 9.5px;
  color: var(--kst-text-dim);
}

.audit-item {
  padding: 6px 8px;
  border: 1px solid var(--kst-border);
  border-radius: 6px;
  background: rgba(9, 13, 18, 0.55);
}

.audit-head {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 10.5px;
}

.audit-meta {
  display: flex;
  gap: 10px;
  margin-top: 3px;
  font-size: 9.5px;
  color: var(--kst-text-dim);
}
</style>
